# Code Review: tauri-plugin-background-service (Primary Pass)

## Files Reviewed
- [x] tauri-plugin-background-service/src/lib.rs
- [x] tauri-plugin-background-service/src/runner.rs
- [x] tauri-plugin-background-service/src/models.rs
- [x] tauri-plugin-background-service/src/error.rs
- [x] tauri-plugin-background-service/src/mobile.rs
- [x] tauri-plugin-background-service/src/notifier.rs
- [x] tauri-plugin-background-service/src/service_trait.rs
- [x] tauri-plugin-background-service/build.rs
- [x] tauri-plugin-background-service/Cargo.toml
- [x] tauri-plugin-background-service/ios/Sources/TauriPluginBackgroundService/BackgroundServicePlugin.swift
- [x] tauri-plugin-background-service/android/src/main/kotlin/app/tauri/backgroundservice/BackgroundServicePlugin.kt
- [x] tauri-plugin-background-service/android/src/main/kotlin/app/tauri/backgroundservice/LifecycleService.kt
- [x] tauri-plugin-background-service/android/src/main/AndroidManifest.xml
- [x] tauri-plugin-background-service/permissions/default.toml
- [x] tauri-plugin-background-service/tests/integration.rs
- [x] tauri-plugin-background-service/examples/basic_service.rs

## Test Results
- **Unit tests:** 32/32 passed
- **Integration tests:** 12/12 passed
- **Clippy:** Clean (only example dead_code warnings)

## Summary
Overall the codebase is well-structured and correct. The architecture is clean: trait-based service abstraction, generation-guarded runner, and thin native bridges. Tests are thorough. **No critical bugs found, but one high-risk area needs deep analysis.**

## Highest-Risk Area (Deep Analysis Needed)

### iOS BGTask Lifecycle (Swift ↔ Rust coordination)
The Swift `BackgroundServicePlugin.swift` implements a complex state machine with 4 distinct code paths that can complete a BGTask:
1. `handleExpiration()` — OS signals time is up
2. `handleSafetyTimerExpiration()` — 25s fallback timer
3. `completeBgTask()` — Rust signals run() finished
4. `stopKeepalive()` — User-initiated stop

All run on main queue (safe from interleaving), but the **Pending Invoke pattern** (`waitForCancel`) creates a cross-language blocking dependency: Rust calls `run_mobile_plugin("waitForCancel")` which blocks a `spawn_blocking` thread until Swift resolves/rejects the invoke. If the safety timer fires while the blocking thread is mid-call, or if `completeBgTask` and the safety timer race, there's risk of:
- Dropped invoke resolutions (Rust thread hangs forever)
- Double `setTaskCompleted` calls (iOS crash)
- Leaked pendingCancelInvoke references

**Initial assessment:** The main-queue serialization prevents most interleaving, but the interaction between `completeBgTask` rejecting the cancel invoke AND `handleSafetyTimerExpiration` doing the same is worth verifying exhaustively.

## Suggestions (Should Consider)
- **lib.rs:63-120** — The `start` command has three `#[cfg(target_os = "ios")]` blocks mixed into the platform-agnostic flow. Consider extracting iOS-specific logic into a helper to reduce cfg-branching noise.
- **runner.rs:77** — `SeqCst` ordering on generation counter is stronger than needed. `Acquire`/`Release` would suffice and is cheaper on ARM (important for mobile). The correctness argument is the same: the spawn reads the generation after the start writes it.
- **mobile.rs:53** — `wait_for_cancel()` blocks a `spawn_blocking` thread indefinitely if the iOS invoke is never resolved (e.g., app force-killed). This is documented but could leak threads in edge cases.

## Deep Analysis: iOS BGTask Lifecycle (Adversarial Pass)

### Methodology
Traced every (state, trigger) pair in `BackgroundServicePlugin.swift` under adversarial timing. Verified all 4 BGTask completion paths for double-completion, orphaned invokes, and safety timer leaks. Traced cross-language Rust↔Swift coordination.

### VERIFIED SAFE: No double `setTaskCompleted`
All 4 completion paths (`handleExpiration`, `handleSafetyTimerExpiration`, `completeBgTask`, `stopKeepalive`) run on the main queue (serialized). Each calls `cleanup()` which sets `currentTask = nil`. Any subsequent path finds `currentTask == nil` and skips `setTaskCompleted`. The `completeBgTask` path uses `if let task = currentTask` (local capture), but the effect is the same — nil guard prevents double calls.

**Trace for all 6 pairwise orderings** (e.g., completeBgTask then handleExpiration):
1. First path executes: `setTaskCompleted` called, `cleanup()` → `currentTask = nil`
2. Second path executes: `currentTask?.setTaskCompleted(...)` → nil → skipped

### VERIFIED SAFE: No orphaned `pendingCancelInvoke`
Every exit path resolves or rejects `pendingCancelInvoke` and sets it to nil. `cleanup()` also nils it. Since all paths are main-queue serialized, no orphan is possible.

### VERIFIED SAFE: Safety timer properly cancelled
`cleanup()` invalidates and nils `safetyTimer`. All 4 completion paths call `cleanup()`. `handleSafetyTimerExpiration` guards with `if currentTask != nil`, making it a no-op after cleanup.

### BUG FOUND — HIGH: iOS foreground service immediately cancelled

**Location:** `lib.rs:95-117` (iOS `wait_for_cancel` spawn_blocking) + `BackgroundServicePlugin.swift:103-111` (waitForCancel resolves immediately when no BGTask)

**Description:** When `start()` is called on iOS, the sequence is:
1. `start_keepalive()` → schedules BGTask for 15+ minutes from now (line 66)
2. `holder.start()` → spawns Tokio task running `init()` → `run()` (line 88)
3. `spawn_blocking` → `wait_for_cancel()` (line 102)
4. Swift `waitForCancel` checks `currentTask` → **nil** (BGTask hasn't fired; scheduled for 15 min) → resolves immediately (line 106-107)
5. Rust blocking thread receives `Ok(())` → calls `runner.stop()` (line 109) → **cancels the CancellationToken**

**Impact:** Long-running iOS services started from the foreground are cancelled almost immediately. The service's `run()` detects `ctx.shutdown.is_cancelled()` and exits. Only short-lived services that complete before the `stop()` dispatch reaches the main thread survive (race-dependent).

**Root cause:** `waitForCancel` resolves immediately when `currentTask` is nil (no active BGTask). This triggers the expiration-handling stop path in Rust, even though no expiration occurred — the BGTask simply hasn't fired yet.

**Fix suggestions (in order of preference):**
- (a) Don't spawn `wait_for_cancel` when no BGTask is active. Add a Swift command `hasActiveBgTask` and conditionally spawn in Rust.
- (b) Change `waitForCancel` to store the invoke even when `currentTask` is nil, and resolve it when `stopKeepalive` is called or the BGTask fires. (Risk: leaked thread if service stops without `stopKeepalive`.)
- (c) Only spawn `wait_for_cancel` from within the BGTask handler (Swift → Rust callback), not from the `start()` command.

**Severity:** HIGH — iOS foreground service use case is completely broken for typical long-running services.

### Confirmed Positive: State machine correctness
The 4-completion-path state machine is well-designed:
- Main-queue serialization prevents all interleaving
- `cleanup()` is idempotent and called from every exit path
- `scheduleNext()` is harmless to call multiple times (duplicate identifier silently rejected by BGTaskScheduler)

## Positive Notes
- Generation counter pattern in runner.rs is elegant and well-tested
- Callback capture-at-spawn-time design prevents stale callback invocation
- Swift code is defensive with safety timers and cleanup
- Comprehensive test coverage including adversarial cases (init failure + generation guard)
- Clean separation between plugin core and native bridges
- Android foreground service is minimal and correct (START_STICKY, proper notification channel)
- Main-queue serialization in Swift makes the 4-path BGTask completion safe from double-action bugs
