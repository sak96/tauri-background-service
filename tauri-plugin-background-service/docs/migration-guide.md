# Migration Guide

This guide covers breaking changes and migration steps between major versions of `tauri-plugin-background-service`.

## 0.1 → 0.2 Migration

Version 0.2 adds **iOS BGProcessingTask support** and a **desktop OS service mode**. There are **no breaking changes** to the existing API — all 0.1 code continues to work unchanged.

### What's New

| Feature | Platform | Description |
|---------|----------|-------------|
| `BGProcessingTask` | iOS | Longer background execution windows (minutes/hours instead of ~30 seconds) |
| `iosProcessingSafetyTimeoutSecs` config | iOS | Configurable safety timeout for processing tasks (default: 0.0, no cap) |
| `desktop-service` feature | Desktop | Cargo feature enabling OS-level daemon mode (systemd / launchd) |
| `desktopServiceMode` config | Desktop | `"inProcess"` (default) or `"osService"` for OS daemon mode |
| `desktopServiceLabel` config | Desktop | Custom label for the OS service |
| `installService()` | Desktop | TypeScript API to install OS service |
| `uninstallService()` | Desktop | TypeScript API to uninstall OS service |
| `serviceStatus()` | Desktop | TypeScript API to query OS service status |

### Required iOS Changes

Update your `Info.plist` to support `BGProcessingTask`:

**Before (0.1):**

```xml
<key>BGTaskSchedulerPermittedIdentifiers</key>
<array>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER).bg-refresh</string>
</array>
<key>UIBackgroundModes</key>
<array>
    <string>fetch</string>
</array>
```

**After (0.2):**

```xml
<key>BGTaskSchedulerPermittedIdentifiers</key>
<array>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER).bg-refresh</string>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER).bg-processing</string>
</array>
<key>UIBackgroundModes</key>
<array>
    <string>fetch</string>
    <string>processing</string>
</array>
```

### Optional: Desktop OS Service Mode

To use the desktop OS service mode:

1. Enable the feature in `Cargo.toml`:

```toml
[dependencies]
tauri-plugin-background-service = { version = "0.2", features = ["desktop-service"] }
```

2. Configure in `tauri.conf.json`:

```json
{
    "plugins": {
        "background-service": {
            "desktopServiceMode": "osService"
        }
    }
}
```

3. Add desktop service permissions to your capabilities.

### No Action Required For

- Existing `startService()` / `stopService()` / `isServiceRunning()` calls
- Existing `BackgroundService<R>` trait implementations
- Existing `PluginConfig` fields (`iosSafetyTimeoutSecs`, `iosCancelListenerTimeoutSecs`)
- Android foreground service behavior

## Change Type Classification

| Type | Meaning | Migration Required |
|------|---------|--------------------|
| **API Changed** | Function signature, parameter, or return type changed | Yes — update call sites |
| **Behavior Changed** | Runtime behavior changed without signature change | Possibly — verify assumptions |
| **Default Changed** | Default value for a configuration option changed | Possibly — check if relying on old default |
| **Deprecated** | Feature still works but will be removed in a future version | Recommended — plan migration |
| **Removed** | Feature no longer exists | Yes — replace with alternative |

## Migration Template

When a breaking change is documented, it follows this format:

```markdown
### [VERSION] Change Title (Change Type)

**Affected:** Who is affected (e.g., "All users", "Android only")

**Before:**

```rust
// Old API or configuration
```

**After:**

```rust
// New API or configuration
```

**Steps:**
1. Concrete action to migrate
2. Another concrete action
```

## Version History

_No versions with breaking changes yet._

## Planned Breaking Changes

_No planned breaking changes at this time._

When planning a breaking change, document it here before release so users can prepare. Include the target version, the planned change, and the recommended migration path.
