//! Desktop IPC client for the GUI process.
//!
//! [`IpcClient`] connects to the headless sidecar's Unix domain socket and
//! provides methods to start/stop the background service and receive events
//! over the IPC protocol.
//!
//! Only available when the `desktop-service` Cargo feature is enabled.

use std::path::PathBuf;

use tauri::{Emitter, Runtime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::desktop::ipc::{
    decode_frame, encode_frame, IpcEvent, IpcRequest, IpcResponse, MAX_FRAME_SIZE,
};
use crate::error::ServiceError;
use crate::models::{PluginEvent, StartConfig};

/// IPC client for communicating with the headless sidecar service.
///
/// Connects to the sidecar's Unix domain socket and translates method calls
/// into [`IpcRequest`] messages. Responses are decoded from [`IpcResponse`]
/// frames.
///
/// Events from the sidecar (started/stopped/error) are read as [`IpcEvent`]
/// frames and converted to [`PluginEvent`] for emission via the Tauri event
/// system.
pub struct IpcClient {
    stream: UnixStream,
}

impl IpcClient {
    /// Connect to the sidecar's IPC socket at the given path.
    pub async fn connect(path: PathBuf) -> Result<Self, ServiceError> {
        let stream = UnixStream::connect(&path)
            .await
            .map_err(|e| ServiceError::Ipc(format!("connect failed: {e}")))?;
        Ok(Self { stream })
    }

    /// Send a Start command to the sidecar.
    pub async fn start(&mut self, config: StartConfig) -> Result<(), ServiceError> {
        let request = IpcRequest::Start { config };
        let response = self.send_and_read(&request).await?;
        if response.ok {
            Ok(())
        } else {
            Err(ServiceError::Ipc(
                response.error.unwrap_or_else(|| "unknown error".into()),
            ))
        }
    }

    /// Send a Stop command to the sidecar.
    pub async fn stop(&mut self) -> Result<(), ServiceError> {
        let response = self.send_and_read(&IpcRequest::Stop).await?;
        if response.ok {
            Ok(())
        } else {
            Err(ServiceError::Ipc(
                response.error.unwrap_or_else(|| "unknown error".into()),
            ))
        }
    }

    /// Send an IsRunning query to the sidecar.
    pub async fn is_running(&mut self) -> Result<bool, ServiceError> {
        let response = self.send_and_read(&IpcRequest::IsRunning).await?;
        if response.ok {
            Ok(response
                .data
                .and_then(|d| d.get("running").and_then(|v| v.as_bool()))
                .unwrap_or(false))
        } else {
            Err(ServiceError::Ipc(
                response.error.unwrap_or_else(|| "unknown error".into()),
            ))
        }
    }

    /// Read the next [`IpcEvent`] from the socket.
    ///
    /// Returns `None` if the connection was closed.
    pub async fn read_event(&mut self) -> Result<Option<IpcEvent>, ServiceError> {
        let frame = match self.read_frame().await? {
            Some(f) => f,
            None => return Ok(None),
        };
        decode_frame::<IpcEvent>(&frame)
            .map(Some)
            .map_err(|e| ServiceError::Ipc(format!("decode event: {e}")))
    }

    /// Spawn a background task that reads [`IpcEvent`] frames and emits
    /// [`PluginEvent`] via the given `AppHandle`.
    ///
    /// The task runs until the socket is closed or an error occurs.
    pub fn listen_events<R: Runtime>(mut self, app: tauri::AppHandle<R>) {
        tokio::spawn(async move {
            loop {
                match self.read_event().await {
                    Ok(Some(event)) => {
                        let plugin_event = ipc_event_to_plugin_event(event);
                        let _ = app.emit("background-service://event", plugin_event);
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        });
    }

    // -- Private helpers -------------------------------------------------------

    async fn send_and_read(
        &mut self,
        request: &IpcRequest,
    ) -> Result<IpcResponse, ServiceError> {
        self.send_request(request).await?;
        // The server interleaves IpcResponse and broadcast IpcEvent frames on
        // the same socket.  Read frames in a loop until we get one that
        // deserialises as an IpcResponse — any IpcEvent frames encountered
        // along the way are silently discarded (they'll also be delivered via
        // listen_events if the caller uses that path).
        loop {
            let frame = self
                .read_frame()
                .await?
                .ok_or_else(|| ServiceError::Ipc("connection closed".into()))?;
            if let Ok(resp) = decode_frame::<IpcResponse>(&frame) {
                return Ok(resp);
            }
            // Not a response frame — it's an event or unknown frame; skip it.
        }
    }

    async fn send_request(&mut self, request: &IpcRequest) -> Result<(), ServiceError> {
        let frame = encode_frame(request);
        self.stream
            .write_all(&frame)
            .await
            .map_err(|e| ServiceError::Ipc(format!("send request: {e}")))?;
        Ok(())
    }

    /// Read a single length-prefixed frame from the socket.
    ///
    /// Returns `None` if the connection was closed cleanly.
    async fn read_frame(&mut self) -> Result<Option<Vec<u8>>, ServiceError> {
        let mut len_buf = [0u8; 4];
        match self.stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(ServiceError::Ipc(format!("read frame: {e}"))),
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        if len > MAX_FRAME_SIZE {
            return Err(ServiceError::Ipc(format!("frame too large: {len}")));
        }
        if len == 0 {
            return Ok(None);
        }
        let mut payload = vec![0u8; len];
        self.stream
            .read_exact(&mut payload)
            .await
            .map_err(|e| ServiceError::Ipc(format!("read payload: {e}")))?;
        let mut frame = Vec::with_capacity(4 + len);
        frame.extend_from_slice(&len_buf);
        frame.extend_from_slice(&payload);
        Ok(Some(frame))
    }
}

/// Convert an [`IpcEvent`] to a [`PluginEvent`].
pub fn ipc_event_to_plugin_event(event: IpcEvent) -> PluginEvent {
    match event {
        IpcEvent::Started => PluginEvent::Started,
        IpcEvent::Stopped { reason } => PluginEvent::Stopped { reason },
        IpcEvent::Error { message } => PluginEvent::Error { message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::ipc_server::IpcServer;
    use crate::manager::{manager_loop, ServiceFactory};
    use crate::models::ServiceContext;
    use crate::service_trait::BackgroundService;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tauri::Listener;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_socket_path() -> PathBuf {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ipc-client-test-{}-{id}.sock",
            std::process::id()
        ))
    }

    /// Service that blocks in run() until cancelled.
    struct BlockingService;

    #[async_trait]
    impl BackgroundService<tauri::test::MockRuntime> for BlockingService {
        async fn init(
            &mut self,
            _ctx: &ServiceContext<tauri::test::MockRuntime>,
        ) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn run(
            &mut self,
            ctx: &ServiceContext<tauri::test::MockRuntime>,
        ) -> Result<(), ServiceError> {
            ctx.shutdown.cancelled().await;
            Ok(())
        }
    }

    /// Service that completes immediately.
    struct ImmediateSuccessService;

    #[async_trait]
    impl BackgroundService<tauri::test::MockRuntime> for ImmediateSuccessService {
        async fn init(
            &mut self,
            _ctx: &ServiceContext<tauri::test::MockRuntime>,
        ) -> Result<(), ServiceError> {
            Ok(())
        }

        async fn run(
            &mut self,
            _ctx: &ServiceContext<tauri::test::MockRuntime>,
        ) -> Result<(), ServiceError> {
            Ok(())
        }
    }

    fn setup_server_with_factory(
        factory: ServiceFactory<tauri::test::MockRuntime>,
    ) -> (PathBuf, tokio_util::sync::CancellationToken) {
        let path = unique_socket_path();
        let app = tauri::test::mock_app();
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(manager_loop(cmd_rx, factory, 28.0, 0.0));
        let server =
            IpcServer::bind(path.clone(), cmd_tx, app.handle().clone()).unwrap();
        let shutdown = tokio_util::sync::CancellationToken::new();
        let s = shutdown.clone();
        tokio::spawn(async move { server.run(s).await });
        (path, shutdown)
    }

    fn setup_server() -> (PathBuf, tokio_util::sync::CancellationToken) {
        setup_server_with_factory(Box::new(|| Box::new(BlockingService)))
    }

    // -- AC1: Client connects ---------------------------------------------------

    #[tokio::test]
    async fn ipc_client_connect() {
        let (path, shutdown) = setup_server();
        let result = IpcClient::connect(path).await;
        assert!(result.is_ok(), "client should connect: {:?}", result.err());
        shutdown.cancel();
    }

    // -- AC2: Start command works -----------------------------------------------

    #[tokio::test]
    async fn ipc_client_send_start() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();
        let result = client.start(StartConfig::default()).await;
        assert!(
            result.is_ok(),
            "start should succeed: {:?}",
            result.err()
        );
        shutdown.cancel();
    }

    // -- AC3: Stop command works ------------------------------------------------

    #[tokio::test]
    async fn ipc_client_send_stop() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();
        client.start(StartConfig::default()).await.unwrap();
        let result = client.stop().await;
        assert!(
            result.is_ok(),
            "stop should succeed: {:?}",
            result.err()
        );
        shutdown.cancel();
    }

    // -- AC4: IsRunning returns status ------------------------------------------

    #[tokio::test]
    async fn ipc_client_is_running() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();

        let running = client.is_running().await.unwrap();
        assert!(!running, "should not be running initially");

        client.start(StartConfig::default()).await.unwrap();
        let running = client.is_running().await.unwrap();
        assert!(running, "should be running after start");

        shutdown.cancel();
    }

    // -- AC5: Events are received -----------------------------------------------

    #[tokio::test]
    async fn ipc_client_receive_events() {
        let (path, shutdown) =
            setup_server_with_factory(Box::new(|| Box::new(ImmediateSuccessService)));
        let mut client = IpcClient::connect(path).await.unwrap();
        client.start(StartConfig::default()).await.unwrap();

        let event = tokio::time::timeout(Duration::from_millis(500), client.read_event())
            .await
            .expect("timed out waiting for event")
            .expect("read_event failed");

        assert!(event.is_some(), "should receive an event");
        let event = event.unwrap();
        assert!(
            matches!(event, IpcEvent::Started),
            "Expected Started event, got {:?}",
            event
        );

        shutdown.cancel();
    }

    // -- Additional: Stop when not running returns error -------------------------

    #[tokio::test]
    async fn ipc_client_stop_when_not_running() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();
        let result = client.stop().await;
        assert!(result.is_err(), "stop when not running should fail");
        shutdown.cancel();
    }

    // -- Additional: Connect to nonexistent socket fails -------------------------

    #[tokio::test]
    async fn ipc_client_connect_to_nonexistent() {
        let path = std::env::temp_dir().join("nonexistent-test-socket.sock");
        let result = IpcClient::connect(path).await;
        assert!(
            result.is_err(),
            "should fail to connect to nonexistent socket"
        );
    }

    // -- Additional: ipc_event_to_plugin_event conversion -----------------------

    #[test]
    fn ipc_event_to_plugin_event_started() {
        let event = IpcEvent::Started;
        let plugin = ipc_event_to_plugin_event(event);
        assert!(matches!(plugin, PluginEvent::Started));
    }

    #[test]
    fn ipc_event_to_plugin_event_stopped() {
        let event = IpcEvent::Stopped {
            reason: "cancelled".into(),
        };
        let plugin = ipc_event_to_plugin_event(event);
        match plugin {
            PluginEvent::Stopped { reason } => assert_eq!(reason, "cancelled"),
            other => panic!("Expected Stopped, got {other:?}"),
        }
    }

    #[test]
    fn ipc_event_to_plugin_event_error() {
        let event = IpcEvent::Error {
            message: "init failed".into(),
        };
        let plugin = ipc_event_to_plugin_event(event);
        match plugin {
            PluginEvent::Error { message } => assert_eq!(message, "init failed"),
            other => panic!("Expected Error, got {other:?}"),
        }
    }

    // -- Additional: Full lifecycle ---------------------------------------------

    #[tokio::test]
    async fn ipc_client_full_lifecycle() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();

        assert!(!client.is_running().await.unwrap());
        client.start(StartConfig::default()).await.unwrap();
        assert!(client.is_running().await.unwrap());
        client.stop().await.unwrap();
        assert!(!client.is_running().await.unwrap());

        shutdown.cancel();
    }

    // -- Additional: listen_events spawns and converts events -------------------

    #[tokio::test]
    async fn ipc_client_listen_events() {
        let (path, shutdown) =
            setup_server_with_factory(Box::new(|| Box::new(ImmediateSuccessService)));
        let app = tauri::test::mock_app();

        let received = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let received_clone = received.clone();
        app.listen("background-service://event", move |_event| {
            received_clone.store(true, Ordering::SeqCst);
        });

        let mut client = IpcClient::connect(path).await.unwrap();
        client.start(StartConfig::default()).await.unwrap();
        client.listen_events(app.handle().clone());

        tokio::time::timeout(Duration::from_millis(500), async {
            while !received.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("timed out waiting for event via listen_events");

        assert!(received.load(Ordering::SeqCst), "should have received event");
        shutdown.cancel();
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  IPC LOOPBACK TESTS (Step 20 — AC2, AC3, AC4)
    // ═══════════════════════════════════════════════════════════════════════

    // -- AC2: IPC loopback full lifecycle with event verification ---------------

    /// Comprehensive IPC loopback: IpcServer + IpcClient in the same process.
    /// Exercises start → Started event → running → stop → Stopped event → stopped.
    ///
    /// Note: IpcEvent frames must be read BEFORE other requests because
    /// `send_and_read` skips event frames looking for IpcResponse.
    #[tokio::test]
    async fn ipc_loopback_full_lifecycle_with_events() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();

        // Initially not running
        assert!(
            !client.is_running().await.unwrap(),
            "should not be running initially"
        );

        // Start the service
        client
            .start(StartConfig::default())
            .await
            .expect("start should succeed");

        // Read the Started event BEFORE any other request
        // (send_and_read on subsequent calls would skip buffered events)
        let started = tokio::time::timeout(Duration::from_millis(500), client.read_event())
            .await
            .expect("timed out waiting for Started event")
            .expect("read_event failed")
            .expect("should receive event");
        assert!(
            matches!(started, IpcEvent::Started),
            "Expected Started event, got {started:?}"
        );

        // Verify running (after consuming the event)
        assert!(
            client.is_running().await.unwrap(),
            "should be running after start"
        );

        // Stop the service
        client.stop().await.expect("stop should succeed");

        // Read the Stopped event BEFORE any other request
        let stopped = tokio::time::timeout(Duration::from_millis(500), client.read_event())
            .await
            .expect("timed out waiting for Stopped event")
            .expect("read_event failed")
            .expect("should receive event");
        assert!(
            matches!(stopped, IpcEvent::Stopped { .. }),
            "Expected Stopped event, got {stopped:?}"
        );

        // Verify not running
        assert!(
            !client.is_running().await.unwrap(),
            "should not be running after stop"
        );

        shutdown.cancel();
    }

    // -- AC3: Event streaming converts IpcEvent to PluginEvent -------------------

    /// Verify events streamed through IPC are correctly converted to PluginEvent.
    #[tokio::test]
    async fn ipc_loopback_event_streaming_plugin_event_conversion() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();

        // Start — expect Started event → PluginEvent::Started
        client.start(StartConfig::default()).await.unwrap();
        let started_ipc = tokio::time::timeout(Duration::from_millis(500), client.read_event())
            .await
            .expect("timed out")
            .expect("read_event failed")
            .expect("should receive event");
        let started_plugin = ipc_event_to_plugin_event(started_ipc);
        assert!(
            matches!(started_plugin, PluginEvent::Started),
            "Expected PluginEvent::Started, got {started_plugin:?}"
        );

        // Stop — expect Stopped event → PluginEvent::Stopped
        client.stop().await.unwrap();
        let stopped_ipc = tokio::time::timeout(Duration::from_millis(500), client.read_event())
            .await
            .expect("timed out")
            .expect("read_event failed")
            .expect("should receive event");
        let stopped_plugin = ipc_event_to_plugin_event(stopped_ipc);
        match stopped_plugin {
            PluginEvent::Stopped { reason } => {
                assert_eq!(reason, "cancelled", "Expected 'cancelled' reason");
            }
            other => panic!("Expected PluginEvent::Stopped, got {other:?}"),
        }

        shutdown.cancel();
    }

    // -- AC4: Error handling — connection drop detected by client ---------------

    /// Verify client detects a dropped connection gracefully (no panic).
    /// Simulates the server side closing the socket mid-connection.
    #[tokio::test]
    async fn ipc_loopback_connection_drop_returns_error() {
        let path = unique_socket_path();

        // Create a minimal "server" that accepts one connection then drops it.
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let path_clone = path.clone();

        let client_handle = tokio::spawn(async move {
            IpcClient::connect(path_clone).await.unwrap()
        });

        // Accept the connection and immediately drop the server-side stream.
        let (server_stream, _) = listener.accept().await.unwrap();
        drop(server_stream);
        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut client = client_handle.await.unwrap();

        // Client should detect the closed connection on next operation.
        let result = client.is_running().await;
        assert!(
            result.is_err(),
            "should get error after server drops connection"
        );

        let _ = std::fs::remove_file(&path);
    }

    // -- AC4: Error handling — double start returns error through IPC ------------

    /// Verify second start (when already running) returns an IPC error.
    #[tokio::test]
    async fn ipc_loopback_double_start_returns_error() {
        let (path, shutdown) = setup_server();
        let mut client = IpcClient::connect(path).await.unwrap();

        client.start(StartConfig::default()).await.unwrap();

        let result = client.start(StartConfig::default()).await;
        assert!(result.is_err(), "double start should return error");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.to_lowercase().contains("already"),
            "Error should mention 'already': {err_msg}"
        );

        shutdown.cancel();
    }
}
