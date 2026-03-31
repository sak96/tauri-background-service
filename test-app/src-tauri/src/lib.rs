use async_trait::async_trait;
use std::time::Duration;
use tauri::{Emitter, Runtime};
use tauri_plugin_background_service::{BackgroundService, ServiceContext, ServiceError};

struct TickService {
    tick_count: u64,
}

#[async_trait]
impl<R: Runtime> BackgroundService<R> for TickService {
    async fn init(&mut self, _ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        Ok(())
    }

    async fn run(&mut self, ctx: &ServiceContext<R>) -> Result<(), ServiceError> {
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                _ = ctx.shutdown.cancelled() => {
                    let _ = ctx.app.emit("service-stopped", self.tick_count);
                    break;
                }
                _ = interval.tick() => {
                    self.tick_count += 1;

                    if self.tick_count == 1 {
                        ctx.notifier.show("Background Service", "Service is running");
                    }

                    let _ = ctx.app.emit("service-tick", self.tick_count);
                }
            }
        }

        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_background_service::init_with_service(|| TickService { tick_count: 0 }))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
