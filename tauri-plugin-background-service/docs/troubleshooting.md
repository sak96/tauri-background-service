# Troubleshooting

Common issues and solutions when integrating `tauri-plugin-background-service`. Each entry is tagged by platform.

---

### [Android] Service dies immediately on Android 12+

**Symptom:** The background service starts but is killed within seconds. Logcat shows a `ForegroundServiceStartNotAllowedException` or a message about missing foreground service type.

**Root cause:** Android 12 (API 31) requires a foreground service type in the manifest. Android 14 (API 34) further requires declaring the specific type at runtime and handling the `onTimeout` callback.

**Solution:**

1. Add the foreground service permission with type to your `AndroidManifest.xml`:

```xml
<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_DATA_SYNC" />
```

2. Declare the service type on the `<service>` element:

```xml
<service
  android:name="app.tauri.backgroundservice.LifecycleService"
  android:foregroundServiceType="dataSync"
  android:exported="false" />
```

3. The plugin defaults to `"dataSync"`. If you use `specialUse`, also add:

```xml
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_SPECIAL_USE" />
```

4. Make sure you're calling `startService()` from a foreground context (visible Activity). Android 12+ restricts background starts.

**See also:** [Android Platform Guide](./android.md)

---

### [Android] Service does not restart after OS kills the app

**Symptom:** The service runs while the app is alive, but after Android kills the process (low memory, swipe-away), the service never comes back.

**Root cause:** The auto-restart mechanism stores a flag in `SharedPreferences` before starting the foreground service. When `LifecycleService` is re-created by `START_STICKY`, the plugin reads this flag during the Activity's `load()` lifecycle and restarts the service automatically. If the flag was never written (e.g., the service was stopped cleanly via `stopService()`), no restart occurs.

**Solution:**

1. Verify auto-restart state by checking the `SharedPreferences` flags:

```bash
adb shell run-as <your.package> cat shared_prefs/bg_service.xml
```

If auto-restart is pending, you'll see `bg_auto_start_pending` set to `true` with the original `bg_auto_start_label` and `bg_auto_start_type` values.

2. The `LifecycleService` does not produce log output during `handleOsRestart()`. To verify the mechanism is working, check that:
   - `bg_service_label` and `bg_service_type` exist in SharedPreferences (written by `BackgroundServicePlugin.startKeepalive()`)
   - The app's main Activity is launched after an OS restart (the `autoRestarting` flag is set to `true`)
   - The plugin's `load()` method reads `bg_auto_start_pending` and re-calls `startKeepalive()`

3. Check that `LifecycleService` is declared in your manifest with `android:foregroundServiceType`.

4. If you explicitly stopped the service with `stopService()`, the `bg_service_label` key is cleared and no auto-restart happens. This is expected behavior — call `startService()` again from your app UI.

**See also:** [Android Platform Guide](./android.md#auto-restart-mechanism)

---

### [Android] Notification permission not granted on Android 13+

**Symptom:** The foreground service starts but no notification appears, or the service crashes with a `SecurityException` about `POST_NOTIFICATIONS`.

**Root cause:** Android 13 (API 33) requires runtime permission for posting notifications. The plugin requests this automatically during setup, but if the user denied it, the foreground notification cannot be shown.

**Solution:**

1. The plugin auto-requests `POST_NOTIFICATIONS` in `BackgroundServicePlugin.load()`. Check your app's permission settings:

```bash
adb shell dumpsys notification policy
```

2. If denied, prompt the user to grant the permission via system settings:

```bash
adb shell am start -a android.settings.APP_NOTIFICATION_SETTINGS \
  --es android.provider.extra.APP_PACKAGE <your.package>
```

3. You can also request the permission from your Tauri app before starting the service using the `@tauri-apps/plugin-notification` permission API.

4. On Android 14+, the notification channel `"bg_keepalive"` (ID `9001`) must have at least `IMPORTANCE_LOW`. The plugin creates this channel automatically.

**See also:** [Android Platform Guide](./android.md#notification)

---

### [iOS] Background service stops after approximately 30 seconds

**Symptom:** The service starts successfully, but iOS terminates it around 28-30 seconds after the app enters the background.

**Root cause:** This is expected iOS behavior. iOS grants background execution time in short bursts (typically 30 seconds) via `BGAppRefreshTask`. The plugin uses a safety timer (default: 28 seconds) to complete the task gracefully before iOS kills it.

**Solution:**

1. This is not a bug — it is a platform limitation. Design your `run()` method to handle cooperative cancellation via `CancellationToken`:

```rust
async fn run(&self, ctx: &ServiceContext<R>) -> Result<(), Box<dyn Error + Send + Sync>> {
    tokio::select! {
        _ = ctx.shutdown.cancelled() => {
            // iOS expiration handler fired — clean up quickly
            Ok(())
        }
        _ = do_work(ctx) => {
            Ok(())
        }
    }
}
```

2. The plugin automatically schedules the next `BGAppRefreshTask` after the current one completes. iOS decides when to run it (minimum 15 minutes between invocations via `earliestBeginDate`).

3. To increase the safety margin, configure `iosSafetyTimeoutSecs` in your Tauri plugin config (default is `28.0`):

```json
{
  "plugins": {
    "background-service": {
      "iosSafetyTimeoutSecs": 25.0
    }
  }
}
```

Keep it below 30 to avoid iOS forcefully terminating the task.

**See also:** [iOS Platform Guide](./ios.md#foreground-vs-background-behavior)

---

### [All] `ServiceError::AlreadyRunning` when calling `startService()`

**Symptom:** Calling `startService()` returns an error:

```json
"AlreadyRunning"
```

Or in Rust:

```
Service is already running
```

**Root cause:** The actor rejects duplicate starts. Only one service instance can run at a time. This is checked in `manager.rs` before any side-effects occur.

**Solution:**

1. Check if the service is already running before calling start:

```typescript
import { isServiceRunning, startService } from 'tauri-plugin-background-service';

if (!await isServiceRunning()) {
  await startService();
}
```

2. Or stop the existing service first:

```typescript
import { stopService, startService } from 'tauri-plugin-background-service';

await stopService();
await startService();
```

3. If you encounter this unexpectedly, check that a previous `startService()` call succeeded. Listen for the `started` event to confirm:

```typescript
import { onPluginEvent } from 'tauri-plugin-background-service';

const unlisten = await onPluginEvent((event) => {
  if (event.type === 'started') {
    console.log('Service confirmed started');
  }
});
```

**See also:** [API Reference](./api-reference.md#serviceerror)

---

### [All] `ServiceError::NotRunning` when calling `stopService()`

**Symptom:** Calling `stopService()` returns an error:

```json
"NotRunning"
```

Or in Rust:

```
Service is not running
```

**Root cause:** No service is currently active. This happens if the service already completed, was never started, or was already stopped.

**Solution:**

1. Guard the stop call with `isServiceRunning()`:

```typescript
import { isServiceRunning, stopService } from 'tauri-plugin-background-service';

if (await isServiceRunning()) {
  await stopService();
}
```

2. If the service should be running but isn't, check for errors in the `onPluginEvent` listener. The service may have failed during `init()` and emitted an `error` event:

```typescript
import { onPluginEvent } from 'tauri-plugin-background-service';

await onPluginEvent((event) => {
  if (event.type === 'error') {
    console.error('Service error:', event.message);
  }
});
```

3. On Android, check SharedPreferences (`bg_service_label` / `bg_service_type`) — if the keys are absent, the OS killed and failed to restart the service (see the [Android restart troubleshooting](#android-service-does-not-restart-after-os-kills-the-app) entry).

**See also:** [API Reference](./api-reference.md#serviceerror)

---

### [All] Enable debug logging

**Symptom:** You need more visibility into what the plugin is doing internally.

**Solution:**

Set the `RUST_LOG` environment variable to control log output from the plugin:

```bash
# Debug-level logging for the plugin only
RUST_LOG=tauri_plugin_background_service=debug your-app

# Trace-level (very verbose) for the plugin
RUST_LOG=tauri_plugin_background_service=trace your-app

# Debug for the plugin + Tauri framework
RUST_LOG=tauri_plugin_background_service=debug,tauri=debug your-app
```

On Android, view the Kotlin-side lifecycle logs via `adb logcat`:

```bash
adb logcat -s LifecycleService:V
```

For Rust-side debug logging on Android, set `RUST_LOG` before launching the app (e.g., via your IDE's run configuration or `adb shell am start` with `--ez` extras if your app reads it).

Key log messages to look for:
- `handle_start` / `handle_stop` — actor command processing
- `start_keepalive` / `stop_keepalive` — mobile lifecycle calls
- `PluginEvent::Started` / `PluginEvent::Stopped` — lifecycle events
- `Unrecognized foreground service type` (Android only, `Log.w` from `LifecycleService.mapServiceType()`) — invalid service type fallback

**See also:** [API Reference](./api-reference.md#pluginevent)

---

### [iOS] BGProcessingTask never fires

**Symptom:** `BGAppRefreshTask` works but `BGProcessingTask` is never scheduled or executed.

**Root cause:** iOS is selective about when it runs `BGProcessingTask`. It prefers conditions like device charging, connected to Wi-Fi, and idle. Unlike `BGAppRefreshTask`, processing tasks require more favorable system conditions.

**Solution:**

1. Verify both identifiers are in `Info.plist`:

```xml
<key>BGTaskSchedulerPermittedIdentifiers</key>
<array>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER).bg-refresh</string>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER).bg-processing</string>
</array>
```

2. Verify `UIBackgroundModes` includes `processing`:

```xml
<key>UIBackgroundModes</key>
<array>
    <string>fetch</string>
    <string>processing</string>
</array>
```

3. Test using the Xcode simulate-launch command:

```bash
e -l objc -- (void)[[BGTaskScheduler sharedScheduler] _simulateLaunchForTaskWithIdentifier:@"YOUR_BUNDLE_ID.bg-processing"]
```

4. For real device testing, plug the device into power and leave it idle. Processing tasks are more likely to execute overnight.

**See also:** [iOS Platform Guide](./ios.md#bgprocessingtask-support)

---

### [Desktop] IPC connection failures in OS service mode

**Symptom:** `startService()` returns an IPC error when `desktopServiceMode` is `"osService"`:

```
Ipc: connect failed: No such file or directory
```

**Root cause:** The sidecar process (headless binary) is not running or the socket path is incorrect.

**Solution:**

1. Verify the sidecar binary is installed and running:

```bash
# Linux
systemctl --user status com.example.myapp.background

# macOS
launchctl list | grep com.example.myapp.background
```

2. Check the socket file exists:

```bash
ls -la /tmp/com.example.myapp.background.sock
```

3. Ensure the sidecar was started with the correct `--service-label` argument.

4. Check the `desktopServiceLabel` config matches the label used when installing the service.

**See also:** [Desktop Platform Guide](./desktop.md#os-service-mode)

---

### [Desktop] Service install permission errors

**Symptom:** `installService()` fails with a permission error.

**Root cause:** Installing OS-level services requires appropriate permissions. On Linux, the user must have access to systemd --user (typically available without sudo). On macOS, launchd user agents should not require elevated permissions.

**Solution:**

1. On Linux, verify the user's systemd session is active:

```bash
systemctl --user status
```

2. On macOS, check that the launchd agent plist is in the correct directory (`~/Library/LaunchAgents/`).

3. If using a system-level service (not user-level), you may need elevated permissions. Consider using a user-level service instead.

4. Check logs for the specific error message from the `service-manager` crate.

**See also:** [Desktop Platform Guide](./desktop.md#os-service-mode)
