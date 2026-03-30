# Building a Tauri v2 Background Service Plugin
## Generic Rust Background Tasks · Stateful Service Trait · Local Notifications · All Platforms

> **Target:** Tauri v2 · Android (API 26+) · iOS 16+ · Windows · macOS · Linux
> **Design principle:** The plugin is a lifecycle container. You provide a plain Rust struct that initialises itself and runs an async loop. The plugin starts it, keeps the OS from killing it, and stops it. Nothing else.

---

## Table of Contents

1. [Core Design Philosophy](#1-core-design-philosophy)
2. [Architecture](#2-architecture)
3. [Plugin File Structure](#3-plugin-file-structure)
4. [Project Scaffold & Dependencies](#4-project-scaffold--dependencies)
5. [Rust Plugin Core](#5-rust-plugin-core)
   - 5.1 `error.rs`
   - 5.2 `models.rs`
   - 5.3 `service_trait.rs` — the contract your service implements
   - 5.4 `runner.rs` — spawns and owns the service task
   - 5.5 `notifier.rs` — notification helper
   - 5.6 `mobile.rs` — thin native bridge
   - 5.7 `lib.rs` — plugin registration
6. [Android — Thin Kotlin OS Adapter](#6-android--thin-kotlin-os-adapter)
7. [iOS — Thin Swift OS Adapter](#7-ios--thin-swift-os-adapter)
8. [JavaScript / TypeScript API](#8-javascript--typescript-api)
9. [Integrating the Plugin into Your App](#9-integrating-the-plugin-into-your-app)
10. [Writing Your Service — The Only File You Edit](#10-writing-your-service--the-only-file-you-edit)
    - 10.1 Minimal example
    - 10.2 Stateful struct pattern
    - 10.3 Notification decisions
    - 10.4 Emitting events to the UI
11. [Reliability Patterns](#11-reliability-patterns)
12. [Notification Behavior by Platform](#12-notification-behavior-by-platform)
13. [iOS Background Execution — Honest Assessment](#13-ios-background-execution--honest-assessment)
14. [Platform Matrix](#14-platform-matrix)
15. [Full File Reference](#15-full-file-reference)

---

## 1. Core Design Philosophy

The plugin does three things and three things only:

1. **Lifecycle management** — `start`, `stop`, `is_running`
2. **OS keepalive on mobile** — Foreground Service on Android, `BGAppRefreshTask` on iOS
3. **Provide helpers** — a `Notifier` the service can call, and an `AppHandle` to emit events to the UI

Everything else — what your service does internally, what state it keeps, what constitutes a "notification-worthy" event — is your code, in your struct, in your crate. The plugin never touches it.

The contract is a single trait:

```rust
pub trait BackgroundService: Send + 'static {
    /// Called once before `run`. Set up whatever your service needs:
    /// load config from disk, open a handle, seed initial state, etc.
    async fn init(&mut self, ctx: &ServiceContext) -> Result<(), ServiceError>;

    /// Called after `init`. This is your async loop. It must run until
    /// `ctx.shutdown` is cancelled — that is the only contract the plugin
    /// enforces. What runs inside is entirely up to you.
    async fn run(&mut self, ctx: &ServiceContext) -> Result<(), ServiceError>;
}
```

You implement this trait on your own struct. The plugin calls `init` then `run` in a Tokio task and manages the rest.

---

## 2. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Your App UI  (TypeScript / Svelte / React / Vue)            │
│                                                              │
│    startService()    stopService()    onServiceEvent(fn)     │
└───────────────────────────┬──────────────────────────────────┘
                            │  Tauri IPC
┌───────────────────────────▼──────────────────────────────────┐
│  tauri-plugin-background-service  (Rust)                     │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  ServiceRunner                                         │  │
│  │                                                        │  │
│  │   your_service.init(ctx).await  ──────────────────┐   │  │
│  │   your_service.run(ctx).await   ──────────────┐   │   │  │
│  │                                               │   │   │  │
│  │   on drop / shutdown token cancel:            │   │   │  │
│  │       task is cancelled cleanly               │   │   │  │
│  └───────────────────────────────────────────────┼───┼───┘  │
│                                                  │   │       │
│  ServiceContext (passed into init and run):      │   │       │
│    .notifier   → show local notification  ◄──────┘   │       │
│    .app        → emit event to JS UI      ◄──────────┘       │
│    .shutdown   → CancellationToken                            │
└───────────────┬─────────────────────────────┬────────────────┘
                │ JNI (mobile only)            │ ObjC (mobile only)
                ▼                             ▼
      ┌──────────────────┐         ┌──────────────────┐
      │  Kotlin          │         │  Swift           │
      │  LifecycleService│         │  BGAppRefresh    │
      │  (keepalive only)│         │  Task (wakeup)   │
      └──────────────────┘         └──────────────────┘
```

**Your service struct** lives entirely outside the plugin. It receives a `ServiceContext` which gives it a `Notifier`, an `AppHandle` for event emission, and a shutdown `CancellationToken`. It never imports anything from the plugin except those types.

---

## 3. Plugin File Structure

```
tauri-plugin-background-service/    ← the reusable plugin
│
├── src/
│   ├── lib.rs              ← Plugin registration & commands
│   ├── error.rs            ← ServiceError type
│   ├── models.rs           ← ServiceContext, ServiceEvent, config types
│   ├── service_trait.rs    ← BackgroundService trait definition
│   ├── runner.rs           ← Spawns the task, owns the handle
│   ├── notifier.rs         ← Notifier helper (calls tauri-plugin-notification)
│   └── mobile.rs           ← Thin bridge to Kotlin/Swift (mobile only)
│
├── android/src/main/kotlin/app/tauri/backgroundservice/
│   ├── BackgroundServicePlugin.kt   ← @TauriPlugin: startKeepalive/stopKeepalive
│   └── LifecycleService.kt          ← Foreground Service, zero logic
│
└── ios/Sources/TauriPluginBackgroundService/
    └── BackgroundServicePlugin.swift  ← BGAppRefreshTask scheduler only


src-tauri/src/                      ← YOUR app crate
│
└── my_service.rs           ← Your struct that implements BackgroundService
```

---

## 4. Project Scaffold & Dependencies

### 4.1 Generate the Plugin

```bash
cargo tauri plugin new background-service --android --ios
cd tauri-plugin-background-service
```

### 4.2 `Cargo.toml`

The plugin has no opinion on what network library you use. It only depends on Tauri and the notification plugin.

```toml
[package]
name = "tauri-plugin-background-service"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib", "cdylib", "rlib"]

[dependencies]
tauri                    = { version = "2" }
tauri-plugin-notification = "2"
serde                    = { version = "1", features = ["derive"] }
serde_json               = "1"
tokio                    = { version = "1", features = ["full"] }
tokio-util               = { version = "0.7", features = ["rt"] }  # CancellationToken
thiserror                = "1"
log                      = "0.4"

[build-dependencies]
tauri-build = "2"
```

Your **app** crate (`src-tauri/Cargo.toml`) is where you add your networking dependencies — `iroh`, `tokio-tungstenite`, a gRPC client, or whatever your service uses.

### 4.3 `build.rs`

```rust
fn main() {
    tauri_build::build()
}
```

---

## 5. Rust Plugin Core

### 5.1 `src/error.rs`

```rust
#[derive(Debug, thiserror::Error)]
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

impl From<ServiceError> for tauri::ipc::InvokeError {
    fn from(e: ServiceError) -> Self {
        tauri::ipc::InvokeError::from(e.to_string())
    }
}
```

---

### 5.2 `src/models.rs`

```rust
use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tokio_util::sync::CancellationToken;
use crate::notifier::Notifier;

/// Passed into both `init` and `run`.
/// Gives your service everything it needs to interact with the outside world.
pub struct ServiceContext<R: Runtime> {
    /// Fire a local notification. Works on all platforms.
    pub notifier: Notifier<R>,

    /// Emit an event to the JS UI layer.
    pub app: tauri::AppHandle<R>,

    /// Cancelled when `stopService()` is called.
    /// Check this inside your run loop to exit cleanly.
    pub shutdown: CancellationToken,
}

/// Optional startup configuration forwarded from JS through the plugin.
/// Extend this or replace it with your own type — it just needs to be
/// Deserialize so it can cross the IPC boundary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StartConfig {
    /// Text shown in the Android persistent foreground notification.
    #[serde(default = "default_label")]
    pub service_label: String,
}

fn default_label() -> String { "Service running".into() }

/// Built-in event types emitted by the runner itself.
/// Your service can emit whatever additional events it likes via ctx.app.emit().
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PluginEvent {
    /// init() completed successfully
    Started,
    /// run() returned or was cancelled
    Stopped { reason: String },
    /// init() or run() returned an error
    Error { message: String },
}
```

---

### 5.3 `src/service_trait.rs`

The contract your service implements.

```rust
use crate::error::ServiceError;
use crate::models::ServiceContext;
use tauri::Runtime;

/// Implement this trait on your own struct in your app crate.
///
/// The plugin will:
///   1. Call `init()` once — seed state, open handles, load config, etc.
///   2. Call `run()` — your async loop, runs until ctx.shutdown is cancelled.
///   3. Drop your struct when the task exits.
///
/// Both methods receive a `ServiceContext`, which gives access to the
/// notifier, the app handle for UI events, and the shutdown token.
/// The plugin has no opinion on what work happens inside these methods.
pub trait BackgroundService<R: Runtime>: Send + 'static {
    /// One-time setup. Called before `run`.
    /// Return Err to abort startup — the plugin will emit a PluginEvent::Error.
    async fn init(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError>;

    /// The main async loop. Must honour `ctx.shutdown`:
    ///
    /// ```rust
    /// loop {
    ///     tokio::select! {
    ///         _ = ctx.shutdown.cancelled() => break,
    ///         _ = self.do_work()           => { /* handle result */ }
    ///     }
    /// }
    /// ```
    ///
    /// Returning Ok(()) is a clean stop.
    /// Returning Err emits a PluginEvent::Error to the JS layer.
    async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError>;
}
```

---

### 5.4 `src/runner.rs`

Owns the spawned task and exposes `start` / `stop` / `is_running` to the plugin commands.

```rust
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};
use tokio_util::sync::CancellationToken;

use crate::error::ServiceError;
use crate::models::{PluginEvent, ServiceContext, StartConfig};
use crate::notifier::Notifier;
use crate::service_trait::BackgroundService;

pub struct ServiceRunner {
    token: Arc<Mutex<Option<CancellationToken>>>,
}

impl ServiceRunner {
    pub fn new() -> Self {
        Self { token: Arc::new(Mutex::new(None)) }
    }

    pub fn is_running(&self) -> bool {
        self.token.lock().unwrap().is_some()
    }

    /// Spawn the service. Generic over R (Tauri runtime) and S (your service type).
    pub fn start<R, S>(
        &self,
        app: AppHandle<R>,
        mut service: S,
        _config: StartConfig,
    ) -> Result<(), ServiceError>
    where
        R: Runtime,
        S: BackgroundService<R>,
    {
        let mut guard = self.token.lock().unwrap();
        if guard.is_some() {
            return Err(ServiceError::AlreadyRunning);
        }

        let shutdown = CancellationToken::new();
        *guard = Some(shutdown.clone());
        drop(guard); // release lock before spawning

        let token_ref = Arc::clone(&self.token);
        let app_clone = app.clone();

        tauri::async_runtime::spawn(async move {
            let ctx = ServiceContext {
                notifier: Notifier::new(app_clone.clone()),
                app: app_clone.clone(),
                shutdown: shutdown.clone(),
            };

            // ── init ──────────────────────────────────────────────────────
            if let Err(e) = service.init(&ctx).await {
                let _ = app_clone.emit(
                    "background-service://event",
                    PluginEvent::Error { message: e.to_string() },
                );
                token_ref.lock().unwrap().take();
                return;
            }

            let _ = app_clone.emit("background-service://event", PluginEvent::Started);

            // ── run ───────────────────────────────────────────────────────
            let result = service.run(&ctx).await;

            // Clean up the token so is_running() returns false
            token_ref.lock().unwrap().take();

            match result {
                Ok(()) => {
                    let _ = app_clone.emit(
                        "background-service://event",
                        PluginEvent::Stopped { reason: "completed".into() },
                    );
                }
                Err(e) => {
                    let _ = app_clone.emit(
                        "background-service://event",
                        PluginEvent::Error { message: e.to_string() },
                    );
                }
            }
        });

        Ok(())
    }

    pub fn stop(&self) -> Result<(), ServiceError> {
        let mut guard = self.token.lock().unwrap();
        match guard.take() {
            Some(token) => {
                token.cancel();
                Ok(())
            }
            None => Err(ServiceError::NotRunning),
        }
    }
}
```

---

### 5.5 `src/notifier.rs`

```rust
use tauri::Runtime;
use tauri_plugin_notification::NotificationExt;

/// Thin wrapper around tauri-plugin-notification.
/// Callable from your service on every platform — no platform-specific code.
#[derive(Clone)]
pub struct Notifier<R: Runtime> {
    app: tauri::AppHandle<R>,
}

impl<R: Runtime> Notifier<R> {
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        Self { app }
    }

    pub fn show(&self, title: &str, body: &str) {
        if let Err(e) = self.app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            log::error!("[notifier] {e}");
        }
    }
}
```

---

### 5.6 `src/mobile.rs`

Signals the native layer to start/stop OS lifecycle bookkeeping. Contains no logic.

```rust
use tauri::{plugin::PluginHandle, Runtime};

pub struct MobileLifecycle<R: Runtime>(pub PluginHandle<R>);

impl<R: Runtime> MobileLifecycle<R> {
    pub fn start_keepalive(&self, label: &str) -> Result<(), tauri::Error> {
        #[derive(serde::Serialize)]
        struct Args<'a> { label: &'a str }
        self.0.run_mobile_plugin("startKeepalive", Args { label })
    }

    pub fn stop_keepalive(&self) -> Result<(), tauri::Error> {
        self.0.run_mobile_plugin("stopKeepalive", ())
    }
}
```

---

### 5.7 `src/lib.rs`

The plugin exposes a generic `init_with_service` function. You pass in your service struct at startup.

```rust
mod error;
mod models;
mod notifier;
mod runner;
mod service_trait;

#[cfg(mobile)]
mod mobile;

pub use error::ServiceError;
pub use models::{PluginEvent, ServiceContext, StartConfig};
pub use notifier::Notifier;
pub use service_trait::BackgroundService;

use std::sync::Arc;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

#[cfg(mobile)]
use mobile::MobileLifecycle;
use runner::ServiceRunner;

// ─── Tauri Commands ───────────────────────────────────────────────────────────

#[tauri::command]
async fn start<R: Runtime>(app: AppHandle<R>, config: StartConfig) -> Result<(), String> {
    #[cfg(mobile)]
    app.state::<MobileLifecycle<R>>()
        .start_keepalive(&config.service_label)
        .map_err(|e| e.to_string())?;

    // The runner is pre-loaded with the service factory by init_with_service().
    // Calling start here triggers the factory to build + spawn the service.
    app.state::<Arc<ServiceRunnerHolder<R>>>()
        .start(app.clone(), config)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    app.state::<Arc<ServiceRunnerHolder<R>>>()
        .runner
        .stop()
        .map_err(|e| e.to_string())?;

    #[cfg(mobile)]
    app.state::<MobileLifecycle<R>>()
        .stop_keepalive()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn is_running<R: Runtime>(app: AppHandle<R>) -> bool {
    app.state::<Arc<ServiceRunnerHolder<R>>>()
        .runner
        .is_running()
}

// ─── Service Factory Holder ───────────────────────────────────────────────────
// Stores a boxed factory closure so the plugin can build the service on demand
// without knowing its concrete type at compile time.

type ServiceFactory<R> = Box<dyn Fn() -> Box<dyn BackgroundService<R>> + Send + Sync>;

struct ServiceRunnerHolder<R: Runtime> {
    runner:  ServiceRunner,
    factory: ServiceFactory<R>,
}

impl<R: Runtime> ServiceRunnerHolder<R> {
    fn start(&self, app: AppHandle<R>, config: StartConfig) -> Result<(), ServiceError> {
        let service = (self.factory)();
        self.runner.start_boxed(app, service, config)
    }
}

// ─── Plugin Builder ───────────────────────────────────────────────────────────

/// Register the plugin with your concrete service type.
///
/// Call this in `main.rs`:
/// ```rust
/// .plugin(tauri_plugin_background_service::init_with_service(|| MyService::new()))
/// ```
pub fn init_with_service<R, S, F>(factory: F) -> TauriPlugin<R>
where
    R: Runtime,
    S: BackgroundService<R>,
    F: Fn() -> S + Send + Sync + 'static,
{
    let boxed_factory: ServiceFactory<R> = Box::new(move || Box::new(factory()));

    Builder::new("background-service")
        .invoke_handler(tauri::generate_handler![start, stop, is_running])
        .setup(move |app, api| {
            app.manage(Arc::new(ServiceRunnerHolder {
                runner:  ServiceRunner::new(),
                factory: boxed_factory,
            }));

            #[cfg(target_os = "android")]
            {
                let handle = api.register_android_plugin(
                    "app.tauri.backgroundservice",
                    "BackgroundServicePlugin",
                )?;
                app.manage(MobileLifecycle(handle));
            }

            #[cfg(target_os = "ios")]
            {
                let handle = api.register_ios_plugin("BackgroundServicePlugin")?;
                app.manage(MobileLifecycle(handle));
            }

            Ok(())
        })
        .build()
}
```

> **Note on `start_boxed`:** `ServiceRunner::start` needs a small companion method that accepts a `Box<dyn BackgroundService<R>>` to erase the concrete type. Add this to `runner.rs` alongside the generic `start`:
>
> ```rust
> pub fn start_boxed<R: Runtime>(
>     &self,
>     app: AppHandle<R>,
>     service: Box<dyn BackgroundService<R>>,
>     config: StartConfig,
> ) -> Result<(), ServiceError> {
>     // same body as start<R, S> but service is already boxed
>     // — identical implementation, just accepting Box<dyn ...>
> }
> ```

---

## 6. Android — Thin Kotlin OS Adapter

### 6.1 `AndroidManifest.xml`

```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_DATA_SYNC" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-permission android:name="android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS" />

    <application>
        <service
            android:name=".LifecycleService"
            android:foregroundServiceType="dataSync"
            android:exported="false"
            android:stopWithTask="false" />
    </application>

</manifest>
```

`stopWithTask="false"` is the critical line. It ensures that when the user swipes the app away, the Foreground Service — and the process running the Rust Tokio runtime — stays alive.

---

### 6.2 `LifecycleService.kt`

Posts the mandatory silent foreground notification. Contains zero business logic.

```kotlin
package app.tauri.backgroundservice

import android.app.*
import android.content.Intent
import android.os.IBinder
import androidx.core.app.NotificationCompat

class LifecycleService : Service() {

    companion object {
        const val CHANNEL_ID   = "bg_keepalive"
        const val NOTIF_ID     = 9001
        const val EXTRA_LABEL  = "label"
        const val ACTION_START = "START"
        const val ACTION_STOP  = "STOP"

        @Volatile var isRunning = false
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) { stopSelf(); return START_NOT_STICKY }

        val label = intent?.getStringExtra(EXTRA_LABEL) ?: "Service running"
        createChannel()
        startForeground(NOTIF_ID, buildNotification(label))
        isRunning = true

        // If the OS kills this process under memory pressure, it will
        // restart it automatically with a null intent.
        return START_STICKY
    }

    override fun onDestroy()          { isRunning = false; super.onDestroy() }
    override fun onBind(i: Intent?)   = null

    private fun buildNotification(label: String): Notification {
        val pi = packageManager.getLaunchIntentForPackage(packageName)
            ?.let { PendingIntent.getActivity(this, 0, it,
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(applicationInfo.loadLabel(packageManager).toString())
            .setContentText(label)
            .setSmallIcon(android.R.drawable.stat_notify_sync)
            .setOngoing(true)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .apply { pi?.let { setContentIntent(it) } }
            .build()
    }

    private fun createChannel() {
        getSystemService(NotificationManager::class.java)
            .createNotificationChannel(
                NotificationChannel(CHANNEL_ID, "Service Status",
                    NotificationManager.IMPORTANCE_LOW)
                    .apply { setShowBadge(false) }
            )
    }
}
```

---

### 6.3 `BackgroundServicePlugin.kt`

Receives `startKeepalive` / `stopKeepalive` from Rust and delegates to `LifecycleService`. Nothing else.

```kotlin
package app.tauri.backgroundservice

import android.app.Activity
import android.content.Intent
import android.os.Build
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

@InvokeArg class StartKeepaliveArgs { var label: String = "Service running" }

@TauriPlugin
class BackgroundServicePlugin(private val activity: Activity) : Plugin(activity) {

    override fun load(webView: android.webkit.WebView?) {
        super.load(webView)
        // Request POST_NOTIFICATIONS once so Rust's Notifier can fire freely
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            activity.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
            != android.content.pm.PackageManager.PERMISSION_GRANTED
        ) {
            activity.requestPermissions(
                arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1001)
        }
    }

    @Command
    fun startKeepalive(invoke: Invoke) {
        val args  = invoke.parseArgs(StartKeepaliveArgs::class.java)
        val intent = Intent(activity, LifecycleService::class.java).apply {
            action = LifecycleService.ACTION_START
            putExtra(LifecycleService.EXTRA_LABEL, args.label)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O)
            activity.startForegroundService(intent)
        else
            activity.startService(intent)
        invoke.resolve()
    }

    @Command
    fun stopKeepalive(invoke: Invoke) {
        activity.startService(Intent(activity, LifecycleService::class.java)
            .apply { action = LifecycleService.ACTION_STOP })
        invoke.resolve()
    }
}
```

---

## 7. iOS — Thin Swift OS Adapter

### 7.1 `Info.plist` additions

```xml
<key>BGTaskSchedulerPermittedIdentifiers</key>
<array>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER).bg-refresh</string>
</array>
<key>UIBackgroundModes</key>
<array>
    <string>fetch</string>
    <string>processing</string>
</array>
```

---

### 7.2 `BackgroundServicePlugin.swift`

Requests notification permission and schedules `BGAppRefreshTask`. Nothing else.

```swift
import UIKit
import BackgroundTasks
import UserNotifications
import WebKit

@objc public class BackgroundServicePlugin: Plugin {

    private var taskId: String {
        "\(Bundle.main.bundleIdentifier ?? "app").bg-refresh"
    }

    public override func load(webView: WKWebView) {
        super.load(webView)

        // Request notification permission once.
        // After this, Rust's Notifier can post notifications freely.
        UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }

        // Register background task handler before the app finishes launching.
        BGTaskScheduler.shared.register(forTaskWithIdentifier: taskId, using: .main) {
            [weak self] task in
            // The OS has granted a background execution window.
            // The Rust Tokio runtime is already in this process and will
            // use this window to run its async work naturally.
            task.setTaskCompleted(success: true)
            self?.scheduleNext()
        }
    }

    @objc public func startKeepalive(_ invoke: Invoke) {
        scheduleNext()
        invoke.resolve()
    }

    @objc public func stopKeepalive(_ invoke: Invoke) {
        BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: taskId)
        invoke.resolve()
    }

    private func scheduleNext() {
        let req = BGAppRefreshTaskRequest(identifier: taskId)
        req.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try? BGTaskScheduler.shared.submit(req)
    }
}
```

---

## 8. JavaScript / TypeScript API

```typescript
// guest-js/index.ts
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export interface StartConfig {
  /** Text shown in the Android persistent foreground notification */
  serviceLabel?: string;
}

/** Built-in plugin lifecycle events */
export type PluginEvent =
  | { type: 'started' }
  | { type: 'stopped';  reason: string }
  | { type: 'error';    message: string };

/**
 * Start the background service.
 * The service struct is already registered in main.rs — this just starts it.
 */
export async function startService(config: StartConfig = {}): Promise<void> {
  await invoke('plugin:background-service|start', config);
}

/** Stop the service and cancel the shutdown token. */
export async function stopService(): Promise<void> {
  await invoke('plugin:background-service|stop');
}

/** Returns true if the Rust service is currently running. */
export async function isServiceRunning(): Promise<boolean> {
  return invoke<boolean>('plugin:background-service|is_running');
}

/**
 * Listen to built-in plugin lifecycle events.
 * Your service can emit its own custom events via ctx.app.emit() —
 * subscribe to those separately with tauri's `listen()`.
 */
export async function onPluginEvent(
  handler: (event: PluginEvent) => void
): Promise<UnlistenFn> {
  return listen<PluginEvent>('background-service://event', e => handler(e.payload));
}
```

---

## 9. Integrating the Plugin into Your App

### 9.1 `src-tauri/Cargo.toml`

```toml
[dependencies]
tauri                           = { version = "2" }
tauri-plugin-notification       = "2"
tauri-plugin-background-service = { path = "../tauri-plugin-background-service" }

# Add your own dependencies here — the plugin has no opinion on what they are.
# Examples: iroh, tokio-tungstenite, reqwest, sqlx, prost, etc.
```

### 9.2 `src-tauri/src/main.rs`

```rust
mod my_service;

use my_service::MyService;

fn main() {
    tauri::Builder::default()
        // notification plugin must come before background-service
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_background_service::init_with_service(|| MyService::new())
        )
        .run(tauri::generate_context!())
        .expect("error while running application");
}
```

### 9.3 Capabilities

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "background-service:allow-start",
    "background-service:allow-stop",
    "background-service:allow-is-running",
    "notification:allow-notify",
    "notification:allow-request-permission",
    "core:event:allow-listen"
  ]
}
```

---

## 10. Writing Your Service — The Only File You Edit

### 10.1 Minimal Example

`src-tauri/src/my_service.rs`:

```rust
use tauri_plugin_background_service::{BackgroundService, ServiceContext, ServiceError};
use tauri::Runtime;

pub struct MyService;

impl MyService {
    pub fn new() -> Self { Self }
}

impl<R: Runtime> BackgroundService<R> for MyService {

    async fn init(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        // Runs once before `run`. Seed your state, open handles, etc.
        // Returning Err here aborts the service before it starts.
        log::info!("Service initialising");
        ctx.app.emit("my-service://status", "ready").ok();
        Ok(())
    }

    async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        // Your async loop. Replace the sleep branch with your real work.
        // The only rule: always select on ctx.shutdown.cancelled().
        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
                    // do work, emit events, show notifications ...
                    ctx.notifier.show("Tick", "Service is alive");
                }
            }
        }
        Ok(())
    }
}
```

---

### 10.2 Stateful Struct Pattern

Your struct is a plain Rust struct. It can hold anything: handles to async resources, counters, cached data, timers, queues. Fields are set in `new`, seeded or opened in `init`, and used throughout `run`. The struct lives for the entire lifetime of the service task — the plugin never re-creates it.

```rust
use std::time::Instant;
use tauri::Runtime;
use tauri_plugin_background_service::{BackgroundService, ServiceContext, ServiceError};

// ── Your struct — holds whatever state your service needs ─────────────────────

pub struct MyService {
    // Injected at construction time
    config: MyConfig,

    // Seeded in init(), mutated in run()
    state:          MyState,
    event_count:    u64,
    last_active:    Option<Instant>,
    last_notified:  Option<Instant>,
}

impl MyService {
    pub fn new(config: MyConfig) -> Self {
        Self {
            config,
            state:         MyState::default(),
            event_count:   0,
            last_active:   None,
            last_notified: None,
        }
    }
}

// ── Trait implementation ──────────────────────────────────────────────────────

impl<R: Runtime> BackgroundService<R> for MyService {

    async fn init(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        // Runs once. Do whatever setup your service requires.
        // Heavy async work (opening handles, loading state, etc.) belongs here
        // rather than in new() so it runs on the Tokio runtime.
        self.state = MyState::load(&self.config)
            .await
            .map_err(|e| ServiceError::Init(e.to_string()))?;

        ctx.app.emit("my-service://ready", &self.state).ok();
        Ok(())
    }

    async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        // The main loop. Structure it however your service demands.
        // The only plugin contract: select on ctx.shutdown.cancelled() so the
        // loop exits promptly when stopService() is called from JS.
        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => break,

                event = self.state.next_event() => {
                    match event {
                        Ok(ev)  => self.handle(ev, ctx),
                        Err(e)  => {
                            ctx.app.emit("my-service://error", e.to_string()).ok();
                            // recover, retry, or break — your choice
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// ── Your logic ────────────────────────────────────────────────────────────────

impl MyService {
    fn handle<R: Runtime>(&mut self, event: MyEvent, ctx: &ServiceContext<R>) {
        self.event_count += 1;
        self.last_active = Some(Instant::now());

        // Emit to the UI
        ctx.app.emit("my-service://event", &event).ok();

        // Decide whether to show a notification — fully your logic
        if event.is_important() {
            ctx.notifier.show(&event.title(), &event.description());
            self.last_notified = Some(Instant::now());
        }
    }
}
```

The types `MyConfig`, `MyState`, `MyEvent` are yours — define them however your service requires. The plugin never sees them.

---

### 10.3 Notification Decisions

The rule is simple: `ctx.notifier.show()` is just a function call. Call it wherever in your service logic it makes sense. There is no separate notification layer, no callback, no event bus — just a direct call.

```rust
// Show immediately when a condition is met
if event.is_critical() {
    ctx.notifier.show(&event.title(), &event.description());
}

// Conditional on a threshold
if self.state.value > self.config.alert_threshold {
    ctx.notifier.show(
        "Threshold exceeded",
        &format!("Value is {:.2}, limit is {}", self.state.value, self.config.alert_threshold),
    );
}

// Batch: accumulate and notify once
self.pending.push(event);
if self.pending.len() >= 5 {
    ctx.notifier.show(
        &format!("{} new events", self.pending.len()),
        &format!("Latest: {}", self.pending.last().unwrap().summary()),
    );
    self.pending.clear();
}

// Rate-limit: at most one notification per minute
let should_notify = self.last_notified
    .map(|t| t.elapsed() > Duration::from_secs(60))
    .unwrap_or(true);

if should_notify {
    ctx.notifier.show("Update", &event.description());
    self.last_notified = Some(Instant::now());
}
```

---

### 10.4 Emitting Events to the UI

`ctx.app.emit()` lets your service push any serializable data to the JS layer in real time. Use it for status updates, incoming data, errors — anything you want the UI to react to.

```rust
// Status string
ctx.app.emit("my-service://status", "ready").ok();

// Any serializable value
#[derive(serde::Serialize)]
struct Update { count: u64, payload: String, ts: u64 }

ctx.app.emit("my-service://update", Update {
    count:   self.event_count,
    payload: event.to_string(),
    ts:      unix_ts(),
}).ok();

// Error
ctx.app.emit("my-service://error", e.to_string()).ok();
```

Subscribe in TypeScript:

```typescript
import { listen } from '@tauri-apps/api/event';

await listen<{ count: number; payload: string; ts: number }>(
  'my-service://update',
  event => console.log('Update:', event.payload)
);

await listen<string>('my-service://status', event => {
  setStatus(event.payload);
});
```

---

## 11. Reliability Patterns

### 11.1 Retry / Recovery Inside `run`

`run` returning `Ok(())` means a clean stop. `run` returning `Err` emits a `PluginEvent::Error` to JS. Neither restarts the service automatically — that is intentional: the plugin has no opinion on whether recovery makes sense for your use case.

If your service should recover from failures internally, implement the retry loop yourself inside `run`. Because `self` persists across iterations of the loop, you can accumulate retry state:

```rust
async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
    let mut retry_delay = Duration::from_secs(2);

    loop {
        tokio::select! {
            _ = ctx.shutdown.cancelled() => break,

            result = self.do_work(ctx) => {
                match result {
                    Ok(())  => { retry_delay = Duration::from_secs(2); }   // reset on success
                    Err(e)  => {
                        ctx.app.emit("my-service://error", e.to_string()).ok();
                        // Exponential backoff, cap at 5 min
                        retry_delay = (retry_delay * 2).min(Duration::from_secs(300));
                        tokio::time::sleep(retry_delay).await;
                    }
                }
            }
        }
    }
    Ok(())
}
```

`do_work` is any private async method on your struct. It can open a handle, consume events, do computation — whatever your service does. When it fails, the loop backs off and retries. When shutdown is requested, the loop exits cleanly regardless of what `do_work` is doing.

---

### 11.2 Honouring the Shutdown Token

Always use `tokio::select!` with `ctx.shutdown.cancelled()` in every blocking `.await`. This ensures `stopService()` from JS exits the loop promptly without waiting for the next event or timeout.

If you have nested async methods, pass the token down or clone it:

```rust
async fn do_work(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
    loop {
        tokio::select! {
            _ = ctx.shutdown.cancelled() => return Ok(()),
            result = self.step()         => { self.process(result?, ctx); }
        }
    }
}
```

### 11.3 Persisting State Across Android Restarts

When Android kills and restarts `LifecycleService` under memory pressure (`START_STICKY`), the Rust process is new and your struct is re-created via the factory closure. If your service needs to resume from a known state, persist it in `run` and restore it in `init`:

```rust
async fn init(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
    // Restore persisted state if available
    if let Some(saved) = Self::load_state(&ctx.app) {
        self.event_count = saved.event_count;
        self.some_key    = saved.some_key;
    }
    Ok(())
}

fn save_state(&self, app: &tauri::AppHandle<impl Runtime>) {
    if let Ok(dir) = app.path().app_data_dir() {
        #[derive(serde::Serialize)]
        struct Snapshot { event_count: u64, some_key: String }
        if let Ok(data) = serde_json::to_vec(&Snapshot {
            event_count: self.event_count,
            some_key:    self.some_key.clone(),
        }) {
            let _ = std::fs::write(dir.join("service-state.json"), data);
        }
    }
}

fn load_state(app: &tauri::AppHandle<impl Runtime>) -> Option<serde_json::Value> {
    let dir  = app.path().app_data_dir().ok()?;
    let data = std::fs::read(dir.join("service-state.json")).ok()?;
    serde_json::from_slice(&data).ok()
}
```

---

## 12. Notification Behavior by Platform

All notifications come from `ctx.notifier.show()` in your Rust struct, via `tauri-plugin-notification`. No server is involved at any point.

| Platform | Underlying API | Notes |
|---|---|---|
| **Android** | `NotificationManager` | `POST_NOTIFICATIONS` permission requested once at plugin load |
| **iOS** | `UNUserNotificationCenter` | Authorization requested once at plugin load |
| **Windows** | WinRT `ToastNotification` | Works while app is running or in tray |
| **macOS** | `UNUserNotificationCenter` | Works while app is running |
| **Linux** | `libnotify` via D-Bus | Works while app is running |

The same call fires on every platform:

```rust
ctx.notifier.show("Title", "Body");
```

---

## 13. iOS Background Execution — Honest Assessment

iOS suspends app processes aggressively. There is no workaround that does not require special Apple entitlements.

| Mechanism | When it runs | Time available | Reliability |
|---|---|---|---|
| App foregrounded | Always | Unlimited | ✅ |
| `beginBackgroundTask` | Immediately after backgrounding | ~30 seconds | ✅ short window |
| `BGAppRefreshTask` | OS-scheduled, ~15–30 min | ~30 seconds | ⚠️ best-effort |

**Practical consequence for your service:**

- While the app is open: your `run` loop executes continuously, all messages arrive in real time
- When the app is backgrounded: you get ~30 seconds, then Tokio is frozen
- Periodically: iOS wakes the process for ~30 seconds via `BGAppRefreshTask`, your loop runs briefly, any queued async work gets processed

**Design accordingly:** make every message your service receives meaningful on its own. Avoid protocols that require maintaining a continuous stream of incremental updates — prefer self-contained messages that are still useful if they arrive minutes late.

The only mechanism that gives reliable real-time iOS background execution is the VoIP push entitlement, which Apple restricts to genuine VoIP applications and reviews strictly. For a general-purpose service, accept the limitation and design your UX around it.

---

## 14. Platform Matrix

| Capability | Android | iOS | Windows | macOS | Linux |
|---|---|---|---|---|---|
| Service runs in background | ✅ Foreground Service | ⚠️ Best-effort BGTask | ✅ | ✅ | ✅ |
| Service survives app close | ✅ `START_STICKY` | ❌ | ❌ | ❌ | ❌ |
| Local notifications from Rust | ✅ | ✅ | ✅ | ✅ | ✅ |
| Server / FCM / APNs required | ❌ Never | ❌ Never | ❌ Never | ❌ Never | ❌ Never |
| All service logic in one file | ✅ | ✅ | ✅ | ✅ | ✅ |
| Native code has service logic | ❌ | ❌ | N/A | N/A | N/A |

---

## 15. Full File Reference

```
tauri-plugin-background-service/   ← plugin crate (write once, never touch again)
│
├── src/
│   ├── lib.rs              Plugin registration, init_with_service()    § 5.7
│   ├── error.rs            ServiceError                                § 5.1
│   ├── models.rs           ServiceContext, StartConfig, PluginEvent    § 5.2
│   ├── service_trait.rs    BackgroundService trait                     § 5.3
│   ├── runner.rs           ServiceRunner, task spawn, lifecycle        § 5.4
│   ├── notifier.rs         Notifier (wraps tauri-plugin-notification)  § 5.5
│   └── mobile.rs           MobileLifecycle bridge                     § 5.6
│
├── android/src/main/kotlin/app/tauri/backgroundservice/
│   ├── LifecycleService.kt          OS keepalive, no logic             § 6.2
│   └── BackgroundServicePlugin.kt  start/stop commands                § 6.3
│
└── ios/Sources/TauriPluginBackgroundService/
    └── BackgroundServicePlugin.swift  BGAppRefreshTask only            § 7.2


src-tauri/                         ← your app (the only file you edit)
│
├── src/
│   ├── main.rs             registers the plugin with your service type § 9.2
│   └── my_service.rs       implements BackgroundService                § 10
└── Cargo.toml              your networking deps go here                § 9.1
```

**Summary of where work goes:**

- `my_service.rs` — all of it. Your struct, your state, your connection logic, your notification decisions
- `main.rs` — one line: `init_with_service(|| MyService::new())`
- Everything else — written once, never changed

---

*The plugin's public contract is four items: the `BackgroundService` trait you implement, the `ServiceContext` your methods receive, `startService()` / `stopService()` in TypeScript, and `onPluginEvent()` for lifecycle events. Your service is free to use any Rust async library for its networking and to emit any events to the UI — the plugin places no constraints on either.*
