# Desktop Platform Guide

This guide covers desktop-specific behavior for the background service plugin (Linux, macOS, Windows).

## How It Works

On desktop platforms, the background service runs as a **standard Tokio async task**. There is no OS-level keepalive mechanism — the service lives as long as the application process.

### Architecture

```
JS: startService()
  → Tauri Command (start)
    → Actor: handle_start()
      → No mobile keepalive (state.mobile is None)
      → tauri::async_runtime::spawn(service task)
        → service.init(&ctx)
        → service.run(&ctx)  ← runs until cancelled or returns
```

Unlike Android (foreground service) and iOS (BGTaskScheduler), desktop has no OS integration. The actor simply spawns the service task and tracks it with a `CancellationToken`.

## No Special Permissions

Desktop platforms require no special permissions, manifest entries, or configuration. The service runs with the same privileges as the application process.

## Service Lifecycle

1. **Start**: `handle_start()` creates a `CancellationToken`, increments the generation counter, and spawns the service task via `tauri::async_runtime::spawn()`.
2. **Run**: The service's `run()` method executes asynchronously. Use `tokio::select!` with `ctx.shutdown.cancelled()` to handle cooperative cancellation.
3. **Stop**: `handle_stop()` cancels the token. The service detects cancellation in `tokio::select!` and returns.
4. **Completion**: The spawned task emits `PluginEvent::Stopped { reason: "completed" }` and fires the `on_complete` callback.

## Cancellation

The only shutdown path is cooperative cancellation via `CancellationToken`:

```rust
async fn run(&mut self, ctx: &ServiceContext<tauri::Wry>) -> Result<(), ServiceError> {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = ctx.shutdown.cancelled() => {
                // Clean up and exit
                break;
            }
            _ = interval.tick() => {
                // Do periodic work
            }
        }
    }
    Ok(())
}
```

Always include `ctx.shutdown.cancelled()` in `tokio::select!`. Without it, `stopService()` will cancel the token but `run()` will never check it.

## Use Cases

Desktop background services are well-suited for:

- **Long-running synchronization**: Continuously sync data with a remote server
- **WebSocket connections**: Maintain persistent connections for real-time updates
- **Periodic tasks**: Run maintenance, cleanup, or polling at regular intervals
- **File watching**: Monitor filesystem changes and react
- **Local server**: Run a local HTTP/WebSocket server alongside the app

## Notification Support

Use the `Notifier` API to display desktop notifications from your background service:

```rust
async fn run(&mut self, ctx: &ServiceContext<tauri::Wry>) -> Result<(), ServiceError> {
    ctx.notifier.show("Sync Complete", "All files are up to date");
    Ok(())
}
```

`Notifier.show(title, body)` uses `tauri-plugin-notification` under the hood. On desktop, notifications appear in the system notification center (Notification Center on macOS, D-Bus notifications on Linux, Action Center on Windows).

## Limits

Desktop has essentially no OS-imposed limits on background execution:

| Aspect | Desktop | Android | iOS |
|--------|---------|---------|-----|
| Execution time | Unlimited (while app runs) | Unlimited (foreground service) | ~30 seconds per window |
| OS restart | No | Yes (`START_STICKY`) | No |
| Permissions | None | Multiple required | Info.plist entries |
| Notification | System notification center | Foreground notification | System notification |
| Keepalive | None (plain task) | Foreground service | BGTaskScheduler |

The service runs for as long as the application process is alive. When the user closes the app, the process exits and the service stops.

## Debugging

Desktop debugging is straightforward — use standard Rust logging and your IDE's debugger:

```bash
# Run with debug logging
RUST_LOG=debug cargo tauri dev

# Filter for plugin-specific logs
RUST_LOG=tauri_plugin_background_service=debug cargo tauri dev
```

### Common Issues

**Service stops when app window is closed:**
This is expected — closing the last window exits the app process on desktop. Use `tauri::Builder::on_window_event` to prevent window close if the service is running:

```rust
app.on_window_event(|window, event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        // Check if service is running before allowing close
        let manager = window.state::<ServiceManagerHandle<tauri::Wry>>();
        // Use a channel or flag to communicate with the actor
    }
});
```

**Service doesn't respond to stopService():**
Verify your `run()` implementation uses `tokio::select!` with `ctx.shutdown.cancelled()`. Without it, the cancellation token is cancelled but `run()` never checks it.
