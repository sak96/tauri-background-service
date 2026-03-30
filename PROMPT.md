# PROMPT — Implement Tauri v2 Background Service Plugin

You are implementing a Tauri v2 plugin called `tauri-plugin-background-service`. The full specification is in `/home/med/tauri-background-service/spec.md`. Read it carefully before starting.

## Context Files (READ ALL BEFORE WRITING ANY CODE)

1. **`/home/med/tauri-background-service/spec.md`** — The complete spec with all code, architecture, and platform details. This is your primary reference.
2. **`/home/med/tauri-background-service/.agents/planning/2026-03-30-tauri-background-service/research/findings.md`** — Research findings on Tauri v2 plugin system, Android foreground services, iOS background execution, and tokio CancellationToken.
3. **`/home/med/tauri-background-service/.agents/planning/2026-03-30-tauri-background-service/implementation/plan.md`** — Step-by-step implementation plan.

## Goal

Implement the complete plugin that works on **all platforms** (Android, iOS, Windows, macOS, Linux) following the spec precisely. The plugin should compile and be ready for integration into a Tauri v2 app.

## Architecture Summary

The plugin is a **lifecycle container**. Users implement a `BackgroundService` trait on their own struct with `init()` and `run()` async methods. The plugin:
- Spawns a Tokio task running the service
- Manages OS keepalive on mobile (Foreground Service on Android, BGAppRefreshTask on iOS)
- Provides a `Notifier` for local notifications via `tauri-plugin-notification`
- Provides a `ServiceContext` with `AppHandle` for events and `CancellationToken` for clean shutdown
- Exposes `startService()`, `stopService()`, `isServiceRunning()` from TypeScript

## Implementation Order (follow strictly)

### Phase 1: Rust Core
1. Create plugin directory structure with `Cargo.toml`, `build.rs`
2. Implement `src/error.rs` — `ServiceError` enum
3. Implement `src/models.rs` — `ServiceContext`, `StartConfig`, `PluginEvent`
4. Implement `src/service_trait.rs` — `BackgroundService` trait
5. Implement `src/notifier.rs` — `Notifier` wrapper over `tauri-plugin-notification`
6. Implement `src/runner.rs` — `ServiceRunner` with start/stop/is_running and the `start_boxed` method for type-erased services
7. Implement `src/mobile.rs` — `MobileLifecycle` bridge (cfg-gated to mobile)
8. Implement `src/lib.rs` — Plugin registration, `init_with_service()`, Tauri commands, `ServiceRunnerHolder` with factory pattern

### Phase 2: Android
9. Create `android/src/main/AndroidManifest.xml` with permissions and service declaration
10. Create `android/src/main/kotlin/app/tauri/backgroundservice/LifecycleService.kt` — Foreground Service
11. Create `android/src/main/kotlin/app/tauri/backgroundservice/BackgroundServicePlugin.kt` — Tauri plugin

### Phase 3: iOS
12. Create `ios/Sources/TauriPluginBackgroundService/BackgroundServicePlugin.swift` — BGTaskScheduler

### Phase 4: JavaScript
13. Create `guest-js/index.ts` — TypeScript API
14. Create `package.json` for the guest-js package

### Phase 5: Permissions & Polish
15. Create `permissions/default.toml` and individual permission files
16. Verify everything compiles together

## Key Technical Details

### Plugin Registration Pattern (lib.rs)
The spec uses `init_with_service(|| MyService::new())` — a factory closure. Internally:
- `ServiceFactory<R> = Box<dyn Fn() -> Box<dyn BackgroundService<R>> + Send + Sync>`
- `ServiceRunnerHolder<R>` stores runner + factory
- `start` command calls `factory()` to create a fresh service instance, then `runner.start_boxed()`

### Type Erasure (runner.rs)
`ServiceRunner::start<R, S>` is generic, but commands need a non-generic interface. The spec shows a `start_boxed` method:
```rust
pub fn start_boxed<R: Runtime>(
    &self,
    app: AppHandle<R>,
    service: Box<dyn BackgroundService<R>>,
    config: StartConfig,
) -> Result<(), ServiceError>
```
This has the same body as the generic `start` but accepts a pre-boxed service.

### CancellationToken Pattern
```rust
loop {
    tokio::select! {
        _ = ctx.shutdown.cancelled() => break,
        result = self.do_work() => { /* handle */ }
    }
}
```

### Android Key Points
- `foregroundServiceType="dataSync"` in manifest
- `FOREGROUND_SERVICE_DATA_SYNC` permission
- `POST_NOTIFICATIONS` runtime permission on Android 13+
- `START_STICKY` for auto-restart
- `stopWithTask="false"` to survive app swipe
- NotificationChannel with IMPORTANCE_LOW, no badge
- PendingIntent with FLAG_IMMUTABLE

### iOS Key Points
- `BGTaskSchedulerPermittedIdentifiers` in Info.plist
- `UIBackgroundModes`: fetch, processing
- Register handler in `load()` before app finishes launching
- `earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)`
- Request notification authorization once in `load()`

### Notification Plugin
- Must be registered BEFORE background-service plugin: `.plugin(tauri_plugin_notification::init())`
- Notifier wraps `app.notification().builder().title().body().show()`

## Critical Constraints

1. **DO NOT add business logic to the plugin** — the plugin is a lifecycle container only
2. **DO NOT put service logic in native code** — Kotlin/Swift only do OS bookkeeping
3. **The trait must be generic over `R: Runtime`** — not hardcoded to `tauri::Wry`
4. **All commands must be generic over `R: Runtime`**
5. **`mobile.rs` must be cfg-gated** — only compiled on mobile targets
6. **Lock the Mutex briefly** — create token, store, drop lock, THEN spawn task (avoid deadlock)
7. **Every `tokio::select!` in examples must include `ctx.shutdown.cancelled()`**

## Validation

After implementation, verify:
1. `cargo check` passes for the plugin crate
2. All file paths match the spec's file structure
3. The `init_with_service` function signature matches the spec
4. Android manifest has all required permissions
5. iOS plugin registers BGAppRefreshTask handler
6. TypeScript API exports match: `startService`, `stopService`, `isServiceRunning`, `onPluginEvent`

## Output

Create all files in `/home/med/tauri-background-service/tauri-plugin-background-service/`.
