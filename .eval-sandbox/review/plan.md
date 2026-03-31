# Review Plan

## Step 1: Primary Pass (COMPLETED)
- Read all source files (Rust, Swift, Kotlin, config)
- Run tests and clippy
- Identify highest-risk areas
- Write initial findings

## Step 2: Deep Analysis — iOS BGTask Lifecycle
Trace all state transitions in `BackgroundServicePlugin.swift` under adversarial timing scenarios:
1. Map every (state, trigger) pair in the Swift state machine
2. Verify no double `setTaskCompleted` calls possible
3. Verify no orphaned `pendingCancelInvoke` possible
4. Verify safety timer is properly cancelled in all exit paths
5. Trace Rust ↔ Swift interaction: what happens if `completeBgTask` fires while `waitForCancel` hasn't been called yet?
6. Verify thread safety of `MobileLifecycle` handle sharing (one for on_complete, one for spawn_blocking)

## Step 3: Deep Analysis — Runner Race Conditions (if needed)
Verify generation counter correctness under rapid stop→start→stop→start:
1. Can the generation counter overflow? (AtomicU64 — practically no)
2. Can stop() race with the spawned task's token cleanup?
3. Is the on_complete callback always called exactly once per task lifecycle?

## Final Step: Synthesis
- Merge all findings into final report
- Approve or request changes
