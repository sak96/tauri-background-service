use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Runtime};
use tokio_util::sync::CancellationToken;

use crate::error::ServiceError;
use crate::models::{PluginEvent, ServiceContext, StartConfig};
use crate::notifier::Notifier;
use crate::service_trait::BackgroundService;

/// Manages service lifecycle: spawns Tokio tasks and tracks the CancellationToken.
///
/// A generation counter prevents a race condition where a rapid stop→start
/// could have the old task's cleanup clear the new service's token.
pub struct ServiceRunner {
    token: Arc<Mutex<Option<CancellationToken>>>,
    generation: Arc<AtomicU64>,
}

impl ServiceRunner {
    /// Creates a new runner with no active service.
    pub fn new() -> Self {
        Self {
            token: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns `true` if a service is currently running.
    pub fn is_running(&self) -> bool {
        self.token.lock().unwrap().is_some()
    }

    /// Start a background service (generic version for concrete types).
    ///
    /// Boxes the service and delegates to [`start_boxed`](Self::start_boxed).
    pub fn start<R, S>(
        &self,
        app: AppHandle<R>,
        service: S,
        config: StartConfig,
    ) -> Result<(), ServiceError>
    where
        R: Runtime,
        S: BackgroundService<R> + 'static,
    {
        self.start_boxed(app, Box::new(service), config)
    }

    /// Start a background service from a type-erased boxed trait object.
    ///
    /// This is the core start method used by the factory pattern.
    /// `#[async_trait]` transforms the trait methods to return
    /// `Pin<Box<dyn Future>>`, so `service.init(&ctx).await` and
    /// `service.run(&ctx).await` work through vtable dispatch.
    pub fn start_boxed<R: Runtime>(
        &self,
        app: AppHandle<R>,
        mut service: Box<dyn BackgroundService<R>>,
        config: StartConfig,
    ) -> Result<(), ServiceError> {
        // Lock discipline: hold briefly, create token, drop, then spawn.
        let mut guard = self.token.lock().unwrap();

        if guard.is_some() {
            return Err(ServiceError::AlreadyRunning);
        }

        let token = CancellationToken::new();
        let shutdown = token.clone();
        *guard = Some(token);

        let my_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;

        drop(guard);

        // Suppress unused-config warning — config is used by the command handler
        // for mobile keepalive labels, not by the runner itself.
        let _config = config;

        let token_ref = self.token.clone();
        let gen_ref = self.generation.clone();

        let ctx = ServiceContext {
            notifier: Notifier { app: app.clone() },
            app: app.clone(),
            shutdown,
        };

        tokio::spawn(async move {
            // Phase 1: init
            if let Err(e) = service.init(&ctx).await {
                let _ = app.emit(
                    "background-service://event",
                    PluginEvent::Error {
                        message: e.to_string(),
                    },
                );
                // Clear token only if generation hasn't advanced
                if gen_ref.load(Ordering::SeqCst) == my_gen {
                    token_ref.lock().unwrap().take();
                }
                return;
            }

            // Emit Started
            let _ = app.emit("background-service://event", PluginEvent::Started);

            // Phase 2: run
            let result = service.run(&ctx).await;

            // Clear token only if generation hasn't advanced
            if gen_ref.load(Ordering::SeqCst) == my_gen {
                token_ref.lock().unwrap().take();
            }

            // Emit terminal event
            match result {
                Ok(()) => {
                    let _ = app.emit(
                        "background-service://event",
                        PluginEvent::Stopped {
                            reason: "completed".into(),
                        },
                    );
                }
                Err(e) => {
                    let _ = app.emit(
                        "background-service://event",
                        PluginEvent::Error {
                            message: e.to_string(),
                        },
                    );
                }
            }
        });

        Ok(())
    }

    /// Stop the currently running service by cancelling the token.
    ///
    /// Returns `NotRunning` if no service is active.
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

impl Default for ServiceRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_not_running() {
        let runner = ServiceRunner::new();
        assert!(!runner.is_running());
    }

    #[test]
    fn default_is_not_running() {
        let runner = ServiceRunner::default();
        assert!(!runner.is_running());
    }

    #[test]
    fn stop_when_not_running_returns_not_running() {
        let runner = ServiceRunner::new();
        let result = runner.stop();
        assert!(matches!(result, Err(ServiceError::NotRunning)));
    }

    #[test]
    fn double_stop_both_return_not_running() {
        let runner = ServiceRunner::new();
        assert!(matches!(runner.stop(), Err(ServiceError::NotRunning)));
        assert!(matches!(runner.stop(), Err(ServiceError::NotRunning)));
    }

    #[test]
    fn stop_clears_running_state() {
        let runner = ServiceRunner::new();
        // Simulate a running service by setting the token directly
        *runner.token.lock().unwrap() = Some(CancellationToken::new());
        assert!(runner.is_running());

        let result = runner.stop();
        assert!(result.is_ok());
        assert!(!runner.is_running());
    }

    #[test]
    fn stop_cancels_the_token() {
        let runner = ServiceRunner::new();
        let token = CancellationToken::new();
        let token_clone = token.clone();
        *runner.token.lock().unwrap() = Some(token);

        assert!(!token_clone.is_cancelled());
        runner.stop().unwrap();
        assert!(token_clone.is_cancelled());
    }

    #[test]
    fn generation_starts_at_zero() {
        let runner = ServiceRunner::new();
        assert_eq!(runner.generation.load(Ordering::SeqCst), 0);
    }

    /// Compile-time test: start() signature compiles with a concrete service type.
    #[allow(dead_code)]
    fn start_signature_compiles<R: Runtime, S: BackgroundService<R> + 'static>(
        runner: &ServiceRunner,
        app: AppHandle<R>,
        service: S,
        config: StartConfig,
    ) {
        let _ = runner.start(app, service, config);
    }

    /// Compile-time test: start_boxed() signature compiles with a boxed dyn trait.
    #[allow(dead_code)]
    fn start_boxed_signature_compiles<R: Runtime>(
        runner: &ServiceRunner,
        app: AppHandle<R>,
        service: Box<dyn BackgroundService<R>>,
        config: StartConfig,
    ) {
        let _ = runner.start_boxed(app, service, config);
    }
}
