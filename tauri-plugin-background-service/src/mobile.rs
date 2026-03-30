use serde::Serialize;
use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

/// Rust-side bridge to native mobile keepalive code.
///
/// Only compiled on mobile targets (`#[cfg(mobile)]` in lib.rs).
/// Calls through to Kotlin (Android) and Swift (iOS) via `run_mobile_plugin`.
pub struct MobileLifecycle<R: Runtime> {
    pub handle: PluginHandle<R>,
}

/// Arguments sent to the native `startKeepalive` handler.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartKeepaliveArgs<'a> {
    label: &'a str,
}

impl<R: Runtime> MobileLifecycle<R> {
    /// Start the OS-specific keepalive mechanism.
    ///
    /// - Android: starts a Foreground Service with `label` as notification text.
    /// - iOS: schedules a `BGAppRefreshTask`.
    pub fn start_keepalive(&self, label: &str) -> Result<(), tauri::Error> {
        self.handle
            .run_mobile_plugin("startKeepalive", StartKeepaliveArgs { label })
    }

    /// Stop the OS-specific keepalive mechanism.
    ///
    /// - Android: stops the Foreground Service.
    /// - iOS: cancels the scheduled background task.
    pub fn stop_keepalive(&self) -> Result<(), tauri::Error> {
        self.handle.run_mobile_plugin("stopKeepalive", ())
    }
}

/// Canonical Tauri v2 mobile init function.
///
/// Registers the plugin with the appropriate native layer:
/// - Android: `app.tauri.backgroundservice.BackgroundServicePlugin`
/// - iOS: uses the `init_plugin_background_service` binding macro
pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<MobileLifecycle<R>, tauri::Error> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("app.tauri.backgroundservice", "BackgroundServicePlugin")?;
    #[cfg(target_os = "ios")]
    let handle = api.register_ios_plugin(init_plugin_background_service)?;
    Ok(MobileLifecycle { handle })
}
