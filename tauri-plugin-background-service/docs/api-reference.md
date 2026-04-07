# API Reference

Complete reference for the Rust and TypeScript APIs provided by `tauri-plugin-background-service`.

---

## Rust API

### `BackgroundService<R>`

The trait you implement to define a background service. Uses [`#[async_trait]`](https://docs.rs/async-trait) for object safety, enabling the factory pattern: `Box<dyn BackgroundService<R>>`.

```rust
#[async_trait]
pub trait BackgroundService<R: Runtime>: Send + 'static {
    async fn init(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError>;
    async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError>;
}
```

#### Methods

| Method | Parameters | Returns | Description |
|--------|-----------|---------|-------------|
| `init` | `ctx: &ServiceContext<R>` | `Result<(), ServiceError>` | Called once before `run`. Use for setup that requires the Tauri context (e.g. opening database connections, registering event listeners). |
| `run` | `ctx: &ServiceContext<R>` | `Result<(), ServiceError>` | The main service loop. Must use `tokio::select!` with `ctx.shutdown.cancelled()` for cooperative cancellation. |

#### Object Safety

The trait is object-safe thanks to `#[async_trait]`. This allows the plugin to store and invoke services through `Box<dyn BackgroundService<R>>`. Do **not** add generic methods or associated types that would break `Box<dyn>` compatibility.

#### Example

```rust
use async_trait::async_trait;
use tauri::Runtime;
use tauri_plugin_background_service::{
    BackgroundService, ServiceContext, ServiceError,
};

struct MyService;

#[async_trait]
impl<R: Runtime> BackgroundService<R> for MyService {
    async fn init(&mut self, _ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        // One-time setup (open DB, register listeners, etc.)
        Ok(())
    }

    async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => {
                    // Cooperative shutdown — clean up and return
                    break;
                }
                _ = do_work() => {
                    // Your background work here
                }
            }
        }
        Ok(())
    }
}
```

---

### `ServiceContext<R>`

Passed into both `init()` and `run()`. Provides everything your service needs to interact with the outside world.

```rust
pub struct ServiceContext<R: Runtime> {
    pub notifier: Notifier<R>,
    pub app: tauri::AppHandle<R>,
    pub shutdown: CancellationToken,
    pub service_label: Option<String>,
    pub foreground_service_type: Option<String>,
}
```

#### Fields

| Field | Type | Description |
|-------|------|-------------|
| `notifier` | `Notifier<R>` | Fire a local notification. Works on all platforms. |
| `app` | `tauri::AppHandle<R>` | Emit events to the JS UI layer, access managed state. |
| `shutdown` | `CancellationToken` | Cancelled when `stopService()` is called. Always use in `tokio::select!` within `run()`. |
| `service_label` | `Option<String>` | Text shown in the Android persistent notification. Always `Some(...)` — uses the `StartConfig` default (`"Service running"`) if not overridden. Meaningful on Android only. |
| `foreground_service_type` | `Option<String>` | Android foreground service type (e.g. `"dataSync"`, `"specialUse"`). Always `Some(...)` — uses the `StartConfig` default (`"dataSync"`) if not overridden. Meaningful on Android only. |

> **Platform behavior:** Both fields are always `Some(...)` on all platforms because the plugin wraps `StartConfig` values unconditionally (manager.rs:257-263). The values have semantic effect only on Android (foreground service notification label and type).

---

### `StartConfig`

Optional startup configuration forwarded from JavaScript through the plugin. Serialized as camelCase JSON.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    pub service_label: String,
    pub foreground_service_type: String,
}
```

#### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `service_label` | `String` | Optional | `"Service running"` | Text shown in the Android persistent foreground notification. Ignored on desktop. |
| `foreground_service_type` | `String` | Optional | `"dataSync"` | Android foreground service type. Common values: `"dataSync"` (default), `"specialUse"`. Ignored on non-Android platforms. |

#### JSON format

```json
{
  "serviceLabel": "Syncing data",
  "foregroundServiceType": "dataSync"
}
```

All fields have defaults — an empty `{}` is valid and uses all defaults.

---

### `PluginConfig`

Plugin-level configuration, deserialized from the Tauri plugin config in `tauri.conf.json`. Controls iOS-specific timing parameters and desktop service mode.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConfig {
    pub ios_safety_timeout_secs: f64,
    pub ios_cancel_listener_timeout_secs: u64,
    pub ios_processing_safety_timeout_secs: f64,
    // Behind #[cfg(feature = "desktop-service")]:
    // pub desktop_service_mode: String,
    // pub desktop_service_label: Option<String>,
}
```

#### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `ios_safety_timeout_secs` | `f64` | Optional | `28.0` | iOS safety timeout for the BGAppRefreshTask expiration handler. iOS only. |
| `ios_cancel_listener_timeout_secs` | `u64` | Optional | `14400` | iOS cancel listener timeout in seconds (4 hours). iOS only. |
| `ios_processing_safety_timeout_secs` | `f64` | Optional | `0.0` | iOS safety timeout for BGProcessingTask. `0.0` means no cap (iOS manages lifetime). iOS only. |
| `desktop_service_mode` | `String` | Optional | `"inProcess"` | Desktop service mode: `"inProcess"` (default) or `"osService"`. Desktop only, requires `desktop-service` feature. |
| `desktop_service_label` | `Option<String>` | Optional | Auto-derived | Custom label for the OS service. Desktop only, requires `desktop-service` feature. |

#### Configuration example

```json
{
  "plugins": {
    "background-service": {
      "iosSafetyTimeoutSecs": 25.0,
      "iosCancelListenerTimeoutSecs": 7200,
      "iosProcessingSafetyTimeoutSecs": 600,
      "desktopServiceMode": "osService",
      "desktopServiceLabel": "com.example.myapp.background"
    }
  }
}
```

---

### `ServiceError`

Error type returned by service operations. Marked `#[non_exhaustive]` — new variants may be added in future versions.

```rust
#[derive(Debug, thiserror::Error, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ServiceError {
    #[error("Service is already running")]
    AlreadyRunning,
    #[error("Service is not running")]
    NotRunning,
    #[error("Initialisation failed: {0}")]
    Init(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
    #[error("Platform error: {0}")]
    Platform(String),
}
```

#### Variants

| Variant | Payload | When it occurs |
|---------|---------|---------------|
| `AlreadyRunning` | — | `startService()` called while a service is already active. |
| `NotRunning` | — | `stopService()` called when no service is active. |
| `Init(String)` | Error message | `init()` returned an error. |
| `Runtime(String)` | Error message | `run()` returned an error, or the actor channel closed. |
| `Platform(String)` | Error message | OS-specific failure (e.g. Android foreground service denied, iOS BGTask rejected, mobile keepalive failure). |
| `ServiceInstall(String)` | Error message | Desktop service installation failed. Requires `desktop-service` feature. |
| `ServiceUninstall(String)` | Error message | Desktop service uninstallation failed. Requires `desktop-service` feature. |
| `Ipc(String)` | Error message | Desktop IPC communication error (socket connection, framing). Requires `desktop-service` feature. |

> **Non-exhaustive:** Match with a wildcard `_` arm to handle future variants gracefully.

---

### `PluginEvent`

Built-in event types emitted by the plugin to the JS UI layer. Serialized as a tagged JSON enum with `"type"` as the tag. Marked `#[non_exhaustive]`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
#[non_exhaustive]
pub enum PluginEvent {
    Started,
    Stopped { reason: String },
    Error { message: String },
}
```

#### Variants

| Variant | Payload | JSON shape | When emitted |
|---------|---------|-----------|-------------|
| `Started` | — | `{ "type": "started" }` | After `init()` completes successfully. |
| `Stopped` | `reason: String` | `{ "type": "stopped", "reason": "..." }` | When `run()` returns `Ok(())`. Currently always emits `reason: "completed"`. |
| `Error` | `message: String` | `{ "type": "error", "message": "..." }` | When `init()` or `run()` returns an error. |

---

### `Notifier<R>`

Thin wrapper over `tauri-plugin-notification`. Fire-and-forget: errors are logged via `log::warn!` and never propagated to callers.

```rust
#[derive(Clone)]
pub struct Notifier<R: Runtime> { /* ... */ }

impl<R: Runtime> Notifier<R> {
    pub fn show(&self, title: &str, body: &str) { /* ... */ }
}
```

#### Methods

| Method | Parameters | Returns | Description |
|--------|-----------|---------|-------------|
| `show` | `title: &str`, `body: &str` | `()` | Show a local notification. Errors are logged but not returned — callers should not need to handle notification failures. |

> **Prerequisite:** `tauri-plugin-notification` must be registered before the background service plugin.

#### Example

```rust
ctx.notifier.show("Sync Complete", "All data uploaded successfully");
```

---

### `init_with_service(factory)`

Creates the Tauri plugin with your service factory. This is the main entry point for registering the plugin.

```rust
pub fn init_with_service<R, S, F>(factory: F) -> TauriPlugin<R, PluginConfig>
where
    R: Runtime,
    S: BackgroundService<R>,
    F: Fn() -> S + Send + Sync + 'static,
```

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `factory` | `F` where `F: Fn() -> S + Send + Sync + 'static` | Required | A zero-argument closure that produces a fresh `BackgroundService` instance. Called once per `startService()` invocation. |

#### Returns

`TauriPlugin<R, PluginConfig>` — pass this to `tauri::Builder::plugin()`.

#### Factory pattern

The factory creates a fresh service instance on each `startService()` call. This ensures clean state after stop-start cycles. The closure captures no mutable state — it only produces new instances.

#### Example

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_notification::init())
    .plugin(tauri_plugin_background_service::init_with_service(|| MyService::new()))
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
```

> **Order matters:** Register `tauri-plugin-notification` **before** the background service plugin, because `Notifier` depends on it.

---

### `AutoStartConfig`

Platform-specific type used for Android auto-start. Deserialized from SharedPreferences values read by the Kotlin `getAutoStartConfig` bridge. Only used on Android.

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoStartConfig {
    pub pending: bool,
    pub label: Option<String>,
    pub service_type: Option<String>,
}
```

#### Fields

| Field | Type | Description |
|-------|------|-------------|
| `pending` | `bool` | Whether an auto-start is pending (set by `LifecycleService` after `START_STICKY` restart). |
| `label` | `Option<String>` | Service label from the original `StartConfig`. |
| `service_type` | `Option<String>` | Foreground service type from the original `StartConfig`. |

#### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `into_start_config(self)` | `Option<StartConfig>` | Converts to `StartConfig` if `pending` is `true` and `label` is `Some`. Returns `None` otherwise. |

> This type is rarely used directly — the plugin handles auto-start detection internally during setup on Android.

---

## TypeScript API

Import from `tauri-plugin-background-service`:

```typescript
import {
  startService,
  stopService,
  isServiceRunning,
  onPluginEvent,
  installService,
  uninstallService,
  serviceStatus,
  type StartConfig,
  type PluginEvent,
} from 'tauri-plugin-background-service';
```

---

### `startService(config?)`

Start the background service. The service struct is already registered in Rust via `init_with_service` — this command tells the actor to begin the `init()` → `run()` lifecycle.

```typescript
async function startService(config?: StartConfig): Promise<void>
```

#### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `config` | `StartConfig` | Optional | `{}` | Startup configuration. All fields have defaults. |

#### Returns

`Promise<void>` — resolves on success, rejects with a string error message on failure.

#### Errors

| Error | When |
|-------|------|
| `"Service is already running"` | A service is already active. Call `stopService()` first. |
| `"Platform error: ..."` | OS-specific failure (e.g. Android foreground service denied). |

#### Example

```typescript
await startService({ serviceLabel: 'Syncing data' });
```

---

### `stopService()`

Stop the running background service. Cancels the shutdown token and stops mobile keepalive.

```typescript
async function stopService(): Promise<void>
```

#### Parameters

None.

#### Returns

`Promise<void>` — resolves on success, rejects with a string error message on failure.

#### Errors

| Error | When |
|-------|------|
| `"Service is not running"` | No service is currently active. |

#### Example

```typescript
await stopService();
```

---

### `isServiceRunning()`

Check whether the background service is currently running.

```typescript
async function isServiceRunning(): Promise<boolean>
```

#### Parameters

None.

#### Returns

`Promise<boolean>` — `true` if a service is active, `false` otherwise.

#### Example

```typescript
const running = await isServiceRunning();
console.log(running); // true or false
```

---

### `installService()` (Desktop only)

Install the background service as an OS-level daemon. Requires the `desktop-service` Cargo feature.

```typescript
async function installService(): Promise<void>
```

#### Parameters

None.

#### Returns

`Promise<void>` — resolves on success, rejects with a string error message on failure.

#### Errors

| Error | When |
|-------|------|
| `"Platform error: ..."` | OS-specific installation failure (permissions, service manager unavailable). |

#### Example

```typescript
await installService();
```

> **Note:** This function is only available when the `desktop-service` feature is enabled. On mobile platforms, calling it will fail with "command not found".

---

### `uninstallService()` (Desktop only)

Uninstall the OS-level daemon service. Requires the `desktop-service` Cargo feature.

```typescript
async function uninstallService(): Promise<void>
```

#### Parameters

None.

#### Returns

`Promise<void>` — resolves on success, rejects with a string error message on failure.

#### Example

```typescript
await uninstallService();
```

---

### `serviceStatus()` (Desktop only)

Query the current status of the OS-level daemon service. Requires the `desktop-service` Cargo feature.

```typescript
async function serviceStatus(): Promise<string>
```

#### Parameters

None.

#### Returns

`Promise<string>` — one of `"running"`, `"stopped"`, or `"not-installed"`.

#### Example

```typescript
const status = await serviceStatus();
console.log(status); // "running" | "stopped" | "not-installed"
```

---

### `onPluginEvent(handler)`

Listen to built-in plugin lifecycle events. Your service can emit custom events via `ctx.app.emit()` — subscribe to those separately with Tauri's `listen()`.

```typescript
async function onPluginEvent(
  handler: (event: PluginEvent) => void
): Promise<UnlistenFn>
```

#### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `handler` | `(event: PluginEvent) => void` | Required | Callback invoked for each plugin event. Receives a `PluginEvent` discriminated union. |

#### Returns

`Promise<UnlistenFn>` — call the returned function to stop listening and prevent memory leaks.

#### Example

```typescript
const unlisten = await onPluginEvent((event) => {
  switch (event.type) {
    case 'started':
      console.log('Service started');
      break;
    case 'stopped':
      console.log('Service stopped:', event.reason);
      break;
    case 'error':
      console.error('Service error:', event.message);
      break;
  }
});

// Clean up when done
unlisten();
```

---

### `StartConfig` (TypeScript)

Startup configuration passed to `startService()`. All fields are optional with sensible defaults.

```typescript
interface StartConfig {
  /** Text shown in the Android persistent foreground notification */
  serviceLabel?: string;
  /**
   * Android foreground service type. Valid values: "dataSync" (default), "specialUse".
   * Ignored on non-Android platforms.
   */
  foregroundServiceType?: string;
  /**
   * Desktop service mode. "in-process" (default) runs in the app process,
   * "os-service" routes through IPC to the OS daemon.
   * Desktop only, requires desktop-service feature.
   */
  mode?: "in-process" | "os-service";
}
```

#### Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `serviceLabel` | `string` | Optional | `"Service running"` | Text shown in the Android persistent notification. |
| `foregroundServiceType` | `string` | Optional | `"dataSync"` | Android foreground service type. `"dataSync"` for data sync, `"specialUse"` for custom use cases. Ignored on non-Android platforms. |
| `mode` | `"in-process" \| "os-service"` | Optional | `"in-process"` | Desktop service mode. `"in-process"` runs as a standard Tokio task; `"os-service"` routes through IPC to the OS-level daemon sidecar. |

---

### `PluginEvent` (TypeScript)

Discriminated union type representing plugin lifecycle events. Use the `type` field to narrow in switch statements.

```typescript
type PluginEvent =
  | { type: 'started' }
  | { type: 'stopped';  reason: string }
  | { type: 'error';    message: string };
```

#### Variants

| `type` value | Additional fields | When emitted |
|-------------|-------------------|-------------|
| `'started'` | — | After `init()` completes successfully. |
| `'stopped'` | `reason: string` | When `run()` returns `Ok(())`. Currently always emits `reason: "completed"`. |
| `'error'` | `message: string` | When `init()` or `run()` returns an error. |

#### Type narrowing

```typescript
onPluginEvent((event) => {
  if (event.type === 'stopped') {
    // TypeScript knows event.reason exists here
    console.log(event.reason);
  }
});
```
