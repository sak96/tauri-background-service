# Code Review: tauri-plugin-background-service (Iteration 2)

## Files Reviewed
- [x] src/lib.rs
- [x] src/manager.rs (new file — actor pattern)
- [x] src/runner.rs
- [x] src/models.rs
- [x] src/mobile.rs
- [x] src/service_trait.rs
- [x] src/error.rs
- [x] src/notifier.rs
- [x] ios/Sources/TauriPluginBackgroundService/BackgroundServicePlugin.swift

## Summary: REQUEST_CHANGES

The actor pattern refactoring resolved the previous review's critical ordering issue (start_keepalive before AlreadyRunning). The manager now checks AlreadyRunning first, then starts keepalive with rollback. However, new issues were found.

## Critical Issues (Must Fix)

### C1. Swift: Duplicate `safetyTimeout` property declaration
**Severity:** CRITICAL (compile error)
**Location:** `BackgroundServicePlugin.swift:17` and `:21`

Two stored properties with identical name and type:
```swift
private var safetyTimeout: TimeInterval = 28.0  // line 17
private var safetyTimeout: TimeInterval = 28.0  // line 21
```
This is a Swift compile error (redeclaration of stored property). **Blocks all iOS builds.** One must be removed.

### C2. Dead code: `runner.rs` duplicates `manager.rs` logic
**Severity:** High (architectural)
**Location:** `src/runner.rs` (entire file)

The actor pattern in `manager.rs` has fully replaced `ServiceRunner` as the lifecycle manager:
- `lib.rs` imports only from `manager.rs` (`ServiceManagerHandle`, `manager_loop`, `ServiceFactory`, `MobileKeepalive`)
- `runner.rs`'s `ServiceRunner::start_boxed()` duplicates `manager.rs`'s `handle_start()` logic
- `runner.rs` hardcodes `service_label: None` and `foreground_service_type: None` (lines 108-109), while `manager.rs` correctly passes config values (lines 195-196)
- Yet `runner.rs` is still `pub use`'d at `lib.rs:17`, exposing dead code in the public API

If someone uses `ServiceRunner` directly, config values are silently lost.

### C3. Hardcoded iOS safety timeout — PluginConfig not read
**Severity:** Medium (correctness)
**Location:** `lib.rs:187`

```rust
let ios_safety_timeout_secs = 28.0; // TODO: read from PluginConfig
```

`PluginConfig` defines `ios_safety_timeout_secs` with serde support, but it's never deserialized from the actual Tauri plugin config. Users cannot configure this value — it's always the default.

## Suggestions (Should Consider)

### S1. `handle_stop` propagates `stop_keepalive` errors
`manager.rs:274` — `mobile.stop_keepalive()?` propagates native errors. By this point the token is already cancelled (service task will stop). Consider logging and swallowing the error — the user's `stop()` call already succeeded in its primary goal.

### S2. 24-hour cancel listener timeout is excessive
`lib.rs:93` — `Duration::from_secs(86400)` for the iOS cancel listener. If the invoke is never resolved, a thread leaks for 24 hours. Consider a shorter timeout (2-4 hours) or making it configurable.

### S3. `runner.rs` should be removed or deprecated
If `ServiceRunner` is dead code, remove it from the public API and the module tree. Keeping it risks confusion and divergence.

## Positive Notes
- Actor pattern with sequential command processing correctly resolves the previous ordering issue
- Generation counter prevents stale cleanup from old tasks
- Callback captured at spawn time prevents the A/B callback bug
- Keepalive failure rollback in `handle_start` (lines 176-182) is correct: clears token, restores callback
- Excellent test coverage in manager.rs — 13+ tokio tests covering lifecycle, callbacks, rollback
- iOS safety timeout now configurable via `PluginConfig` model (just not wired up yet)
- Clean separation: models, error types, trait definition, mobile bridge

---

## Deep Analysis: Swift Dead Property & Dead Code runner.rs (Step 2)

### C1 CONFIRMED — Swift duplicate `safetyTimeout` (compile error)

**Evidence:** `BackgroundServicePlugin.swift` lines 17 and 21 both declare `private var safetyTimeout: TimeInterval = 28.0`. Swift forbids duplicate stored properties — this is an **unconditional compile error** that blocks all iOS builds.

**Root cause:** The property at line 17 has comment "Configurable safety timeout from Rust (PluginConfig). Default: 28.0s." and the duplicate at line 21 has comment "Set via `startKeepalive` args from Rust (PluginConfig)." This is a merge/copy-paste artifact: someone added an updated version of the property with a better comment without removing the original.

**Fix:** Remove lines 15-17 (the first declaration). Keep lines 19-21 (the second declaration with the more accurate comment about `startKeepalive`). The `safetyTimer` property at line 18 stays.

### C2 CONFIRMED — `runner.rs` is dead code, but still in public API + integration tests

**Evidence:**
- All production code paths go through `ServiceManagerHandle` → `manager_loop` (actor pattern). Zero production code calls `ServiceRunner` methods.
- `ServiceRunner` is still `pub use`'d at `lib.rs:17`, exposing it as public API.
- **12 integration tests** in `tests/integration.rs` use `ServiceRunner::new()` directly — these test the OLD code path that was replaced by the actor. They give a false sense of security.
- `runner.rs` has bugs the actor doesn't:
  - Lines 108-109: `service_label: None, foreground_service_type: None` — silently drops config values
  - Line 93: `let _config = config;` — suppresses the unused-config warning, hiding the data loss
  - No mobile keepalive integration at all
  - No `PluginConfig` timeout support

**Risk:** A consumer could use `ServiceRunner` directly (it's public API) and silently lose `service_label`, `foreground_service_type`, and mobile keepalive.

**Recommendation:** Remove `pub use runner::ServiceRunner` from `lib.rs`, gate `runner.rs` behind `#[cfg(test)]` or delete it, and rewrite integration tests to use the actor path via `ServiceManagerHandle`.

### C3 CONFIRMED — Hardcoded timeout, but wiring is 95% complete

**Evidence:**
- `lib.rs:187`: `let ios_safety_timeout_secs = 28.0; // TODO: read from PluginConfig`
- `PluginConfig` in `models.rs` has full serde support + default of 28.0 + 4 passing tests
- The actor-to-mobile wiring IS complete: `ServiceState.ios_safety_timeout_secs` → `start_keepalive()` → `StartKeepaliveArgs` → Swift `startKeepalive` reads `iosSafetyTimeoutSecs`
- Only missing: `init_with_service`'s setup closure never reads `PluginConfig` from Tauri plugin config

**Fix:** In `init_with_service`, use Tauri's plugin config API to deserialize `PluginConfig` and pass the timeout to `manager_loop`. This is a one-line change plus a `use crate::models::PluginConfig`.

### Additional Finding: Integration tests test wrong code path

All 12 integration tests in `tests/integration.rs` exercise `ServiceRunner` directly — a dead code path. None test the actual production actor path through `ServiceManagerHandle`. This means:
- Production code (actor) has only 13 unit tests in `manager.rs` (no integration-level tests)
- The "integration" tests are actually dead-code tests
- The manager's `ServiceContext` population (service_label, foreground_service_type) has no integration test coverage

### Rust test results
All 79 tests pass (67 unit + 12 integration + 0 doc-tests). The codebase compiles cleanly on desktop. The Swift compile error only manifests on iOS builds.
