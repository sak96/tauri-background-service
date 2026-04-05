# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-04-04

### Added

- `BackgroundService<R>` trait with `init()` and `run()` lifecycle methods
- `ServiceContext<R>` with notifier, app handle, and shutdown token
- Android Foreground Service with `START_STICKY` auto-restart
- iOS BGTaskScheduler integration with configurable safety timeout
- Desktop standard Tokio task execution
- TypeScript API: `startService()`, `stopService()`, `isServiceRunning()`, `onPluginEvent()`
- Permissions system with `allow-start`, `allow-stop`, `allow-is-running`
- `Notifier` helper for fire-and-forget local notifications
- `StartConfig` with configurable `serviceLabel` and `foregroundServiceType`

[Unreleased]: https://github.com/dardourimohamed/tauri-background-service/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/dardourimohamed/tauri-background-service/releases/tag/v0.1.0
