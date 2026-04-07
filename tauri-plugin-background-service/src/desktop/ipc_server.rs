//! Desktop IPC server for the headless sidecar process.
//!
//! The IpcServer binds to a Unix domain socket and translates incoming
//! [`IpcRequest`] messages into [`ManagerCommand`] messages for the local actor
//! loop. Command outcomes produce [`IpcResponse`] and [`IpcEvent`] messages
//! sent back to connected clients.
//!
//! Events are broadcast to **all** connected clients via a [`broadcast`] channel,
//! not just the one that triggered the state change.

use std::path::PathBuf;

use tauri::{AppHandle, Runtime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use crate::desktop::ipc::{encode_frame, IpcEvent, IpcRequest, IpcResponse, MAX_FRAME_SIZE};
use crate::error::ServiceError;
use crate::manager::ManagerCommand;

/// Error type for reading IPC frames from a stream.
#[allow(dead_code)]
enum ReadError {
    /// An I/O error (connection lost, etc.).
    Io(std::io::Error),
    /// The JSON payload could not be deserialized as a valid [`IpcRequest`].
    Json(String),
    /// The frame payload exceeded [`MAX_FRAME_SIZE`].
    TooLarge(#[allow(dead_code)] usize),
}

/// Incoming message from the reader task.
enum Incoming {
    /// A valid IPC request.
    Request(IpcRequest),
    /// A recoverable error (malformed frame). Reader keeps running.
    Error(String),
    /// The connection was lost or a fatal error occurred.
    Done,
}

/// IPC server for the headless sidecar process.
///
/// Binds to a Unix domain socket, accepts client connections, and translates
/// incoming [`IpcRequest`] messages into [`ManagerCommand`] dispatches to the
/// local service manager actor. Responses and events are written back to the
/// client as [`IpcResponse`] and [`IpcEvent`] frames.
///
/// Events are broadcast to **all** connected clients, not just the one that
/// triggered the state change.
pub(crate) struct IpcServer<R: Runtime> {
    listener: UnixListener,
    cmd_tx: mpsc::Sender<ManagerCommand<R>>,
    app: AppHandle<R>,
    event_tx: broadcast::Sender<IpcEvent>,
}

#[allow(dead_code)]
impl<R: Runtime> IpcServer<R> {
    /// Bind to the given socket path and return a new [`IpcServer`].
    ///
    /// Removes any stale socket file at the given path before binding.
    pub fn bind(
        path: PathBuf,
        cmd_tx: mpsc::Sender<ManagerCommand<R>>,
        app: AppHandle<R>,
    ) -> Result<Self, ServiceError> {
        // Remove stale socket file from a previous run.
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        let listener = UnixListener::bind(&path)
            .map_err(|e| ServiceError::Ipc(format!("bind failed: {e}")))?;
        let (event_tx, _) = broadcast::channel(32);
        Ok(Self {
            listener,
            cmd_tx,
            app,
            event_tx,
        })
    }

    /// Run the accept loop, spawning a task per client connection.
    ///
    /// This method consumes `self` and runs until either:
    /// - The `shutdown` token is cancelled (graceful shutdown)
    /// - The listener encounters a fatal error
    pub async fn run(self, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let cmd_tx = self.cmd_tx.clone();
                            let app = self.app.clone();
                            let event_tx = self.event_tx.clone();
                            tokio::spawn(handle_connection(stream, cmd_tx, app, event_tx));
                        }
                        Err(e) => {
                            log::warn!("IPC accept error: {e}");
                            break;
                        }
                    }
                }
                _ = shutdown.cancelled() => {
                    log::info!("IPC server shutting down");
                    break;
                }
            }
        }
    }
}

/// Background task that reads [`IpcRequest`] frames from a stream and sends
/// them through an [`mpsc`] channel. This isolates the non-cancel-safe read
/// operations from the select loop in [`handle_connection`].
async fn request_reader(
    mut stream: tokio::net::unix::OwnedReadHalf,
    tx: mpsc::Sender<Incoming>,
) {
    loop {
        match read_request(&mut stream).await {
            Ok(req) => {
                if tx.send(Incoming::Request(req)).await.is_err() {
                    break;
                }
            }
            Err(ReadError::Json(msg)) => {
                if tx.send(Incoming::Error(msg)).await.is_err() {
                    break;
                }
            }
            Err(_) => {
                let _ = tx.send(Incoming::Done).await;
                break;
            }
        }
    }
}

/// Handle a single client connection.
///
/// Splits the stream into read and write halves. A reader task forwards
/// [`IpcRequest`] frames through an mpsc channel. The main loop uses
/// `tokio::select!` to handle both incoming requests and broadcast events,
/// relaying events to the connected client.
#[allow(dead_code)]
async fn handle_connection<R: Runtime>(
    stream: tokio::net::UnixStream,
    cmd_tx: mpsc::Sender<ManagerCommand<R>>,
    app: AppHandle<R>,
    event_tx: broadcast::Sender<IpcEvent>,
) {
    let mut event_rx = event_tx.subscribe();
    let (stream_read, mut stream_write) = stream.into_split();
    let (incoming_tx, mut incoming_rx) = mpsc::channel::<Incoming>(16);

    let reader_handle = tokio::spawn(request_reader(stream_read, incoming_tx));

    loop {
        tokio::select! {
            incoming = incoming_rx.recv() => {
                match incoming {
                    Some(Incoming::Request(request)) => {
                        let (response, maybe_event) = handle_request_with_event(
                            request, &cmd_tx, &app,
                        )
                        .await;
                        let resp_frame = encode_frame(&response);
                        if stream_write.write_all(&resp_frame).await.is_err() {
                            break;
                        }
                        if let Some(event) = maybe_event {
                            let _ = event_tx.send(event);
                        }
                    }
                    Some(Incoming::Error(msg)) => {
                        let resp = IpcResponse {
                            ok: false,
                            data: None,
                            error: Some(msg),
                        };
                        let frame = encode_frame(&resp);
                        if stream_write.write_all(&frame).await.is_err() {
                            break;
                        }
                    }
                    Some(Incoming::Done) | None => break,
                }
            }
            event_result = event_rx.recv() => {
                match event_result {
                    Ok(event) => {
                        let frame = encode_frame(&event);
                        if stream_write.write_all(&frame).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("IPC client lagged {n} events");
                    }
                    Err(_) => break,
                }
            }
        }
    }

    reader_handle.abort();
}

/// Read a length-prefixed [`IpcRequest`] from the stream.
#[allow(dead_code)]
async fn read_request<R: tokio::io::AsyncRead + Unpin>(
    stream: &mut R,
) -> Result<IpcRequest, ReadError> {
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(ReadError::Io)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(ReadError::TooLarge(len));
    }
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .await
        .map_err(ReadError::Io)?;
    serde_json::from_slice(&payload).map_err(|e| ReadError::Json(e.to_string()))
}

/// Forward an [`IpcRequest`] to the actor and return the response + optional event.
#[allow(dead_code)]
async fn handle_request_with_event<R: Runtime>(
    request: IpcRequest,
    cmd_tx: &mpsc::Sender<ManagerCommand<R>>,
    app: &AppHandle<R>,
) -> (IpcResponse, Option<IpcEvent>) {
    match request {
        IpcRequest::Start { config } => {
            let (reply, rx) = tokio::sync::oneshot::channel();
            if cmd_tx
                .send(ManagerCommand::Start {
                    config,
                    reply,
                    app: app.clone(),
                })
                .await
                .is_err()
            {
                return error_response("manager shut down");
            }
            match rx.await {
                Ok(Ok(())) => (
                    IpcResponse {
                        ok: true,
                        data: None,
                        error: None,
                    },
                    Some(IpcEvent::Started),
                ),
                Ok(Err(e)) => (
                    IpcResponse {
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    },
                    None,
                ),
                Err(_) => error_response("manager dropped reply"),
            }
        }
        IpcRequest::Stop => {
            let (reply, rx) = tokio::sync::oneshot::channel();
            if cmd_tx
                .send(ManagerCommand::Stop { reply })
                .await
                .is_err()
            {
                return error_response("manager shut down");
            }
            match rx.await {
                Ok(Ok(())) => (
                    IpcResponse {
                        ok: true,
                        data: None,
                        error: None,
                    },
                    Some(IpcEvent::Stopped {
                        reason: "cancelled".into(),
                    }),
                ),
                Ok(Err(e)) => (
                    IpcResponse {
                        ok: false,
                        data: None,
                        error: Some(e.to_string()),
                    },
                    None,
                ),
                Err(_) => error_response("manager dropped reply"),
            }
        }
        IpcRequest::IsRunning => {
            let (reply, rx) = tokio::sync::oneshot::channel();
            if cmd_tx
                .send(ManagerCommand::IsRunning { reply })
                .await
                .is_err()
            {
                return error_response("manager shut down");
            }
            match rx.await {
                Ok(running) => (
                    IpcResponse {
                        ok: true,
                        data: Some(serde_json::json!({ "running": running })),
                        error: None,
                    },
                    None,
                ),
                Err(_) => error_response("manager dropped reply"),
            }
        }
    }
}

/// Helper to build an error-only response tuple.
#[allow(dead_code)]
fn error_response(msg: &str) -> (IpcResponse, Option<IpcEvent>) {
    (
        IpcResponse {
            ok: false,
            data: None,
            error: Some(msg.to_string()),
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::{manager_loop, ServiceFactory};
    use crate::models::ServiceContext;
    use crate::service_trait::BackgroundService;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use tokio::net::UnixStream;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn unique_socket_path() -> PathBuf {
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ipc-server-test-{}-{id}.sock",
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
    ) -> (IpcServer<tauri::test::MockRuntime>, PathBuf, CancellationToken) {
        let path = unique_socket_path();
        let app = tauri::test::mock_app();
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        tokio::spawn(manager_loop(cmd_rx, factory, 28.0, 0.0));
        let server =
            IpcServer::bind(path.clone(), cmd_tx, app.handle().clone()).unwrap();
        let shutdown = CancellationToken::new();
        (server, path, shutdown)
    }

    fn setup_server() -> (IpcServer<tauri::test::MockRuntime>, PathBuf, CancellationToken) {
        setup_server_with_factory(Box::new(|| Box::new(BlockingService)))
    }

    async fn connect(path: &PathBuf) -> UnixStream {
        UnixStream::connect(path).await.unwrap()
    }

    async fn send_request(stream: &mut UnixStream, request: &IpcRequest) {
        let frame = encode_frame(request);
        stream.write_all(&frame).await.unwrap();
    }

    async fn read_response(stream: &mut UnixStream) -> IpcResponse {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();
        serde_json::from_slice(&payload).unwrap()
    }

    async fn read_event(stream: &mut UnixStream) -> IpcEvent {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.unwrap();
        serde_json::from_slice(&payload).unwrap()
    }

    // ── AC1: Server accepts connections ────────────────────────────────

    #[tokio::test]
    async fn ipc_server_accepts_connection() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let result = UnixStream::connect(&path).await;
        assert!(result.is_ok(), "client should connect");

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── AC2: Start command works ───────────────────────────────────────

    #[tokio::test]
    async fn ipc_server_start_command() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;
        send_request(
            &mut stream,
            &IpcRequest::Start {
                config: crate::models::StartConfig::default(),
            },
        )
        .await;

        let response = read_response(&mut stream).await;
        assert!(response.ok, "Start should succeed: {:?}", response.error);

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── AC3: Stop command works ────────────────────────────────────────

    #[tokio::test]
    async fn ipc_server_stop_command() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;

        // Start first
        send_request(
            &mut stream,
            &IpcRequest::Start {
                config: crate::models::StartConfig::default(),
            },
        )
        .await;
        let resp = read_response(&mut stream).await;
        assert!(resp.ok);
        // Consume the Started event broadcast
        let _ = read_event(&mut stream).await;

        // Stop
        send_request(&mut stream, &IpcRequest::Stop).await;
        let resp = read_response(&mut stream).await;
        assert!(resp.ok, "Stop should succeed: {:?}", resp.error);

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── AC4: Events are streamed ───────────────────────────────────────

    #[tokio::test]
    async fn ipc_server_streams_started_event() {
        let (server, path, shutdown) =
            setup_server_with_factory(Box::new(|| Box::new(ImmediateSuccessService)));
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;
        send_request(
            &mut stream,
            &IpcRequest::Start {
                config: crate::models::StartConfig::default(),
            },
        )
        .await;

        // Read response first
        let resp = read_response(&mut stream).await;
        assert!(resp.ok);

        // Read event — should be Started (broadcast)
        let event = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream),
        )
        .await
        .expect("timed out waiting for Started event");
        assert!(
            matches!(event, IpcEvent::Started),
            "Expected Started event, got {:?}",
            event
        );

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── AC5: Malformed frames handled gracefully ───────────────────────

    #[tokio::test]
    async fn ipc_server_rejects_malformed_frame() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;

        // Send a valid length prefix + invalid JSON
        let payload = b"not valid json!!!";
        let mut frame = Vec::with_capacity(4 + payload.len());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(payload);
        stream.write_all(&frame).await.unwrap();

        // Read error response
        let resp = read_response(&mut stream).await;
        assert!(!resp.ok, "should be error response");
        assert!(resp.error.is_some(), "should have error message");

        // Connection should still be open — send a valid request
        send_request(&mut stream, &IpcRequest::IsRunning).await;
        let resp2 = read_response(&mut stream).await;
        assert!(
            resp2.ok,
            "connection should still work after malformed frame"
        );

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── AC6: Client disconnect handled ─────────────────────────────────

    #[tokio::test]
    async fn ipc_server_handles_client_disconnect() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        // Connect and immediately drop
        {
            let _stream = connect(&path).await;
        }

        // Give the server a moment to process the disconnect
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Server should still accept new connections
        let result = UnixStream::connect(&path).await;
        assert!(
            result.is_ok(),
            "server should still accept connections after client disconnect"
        );

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── Additional: IsRunning returns correct state ────────────────────

    #[tokio::test]
    async fn ipc_server_is_running_returns_false_initially() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;
        send_request(&mut stream, &IpcRequest::IsRunning).await;
        let resp = read_response(&mut stream).await;
        assert!(resp.ok);
        assert_eq!(resp.data.unwrap()["running"], false);

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── Additional: Stop when not running returns error ────────────────

    #[tokio::test]
    async fn ipc_server_stop_when_not_running() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;
        send_request(&mut stream, &IpcRequest::Stop).await;
        let resp = read_response(&mut stream).await;
        assert!(!resp.ok, "stop when not running should fail");
        assert!(resp.error.unwrap().contains("not running"));

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── Additional: Stopped event on stop ──────────────────────────────

    #[tokio::test]
    async fn ipc_server_stopped_event_on_stop() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream = connect(&path).await;

        // Start
        send_request(
            &mut stream,
            &IpcRequest::Start {
                config: crate::models::StartConfig::default(),
            },
        )
        .await;
        let resp = read_response(&mut stream).await;
        assert!(resp.ok);
        // Consume the Started event (broadcast)
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream),
        )
        .await;

        // Stop
        send_request(&mut stream, &IpcRequest::Stop).await;
        let resp = read_response(&mut stream).await;
        assert!(resp.ok);
        let event = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream),
        )
        .await
        .expect("timed out waiting for Stopped event");
        assert!(
            matches!(event, IpcEvent::Stopped { .. }),
            "Expected Stopped event, got {:?}",
            event
        );

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── Additional: Multiple clients can connect ───────────────────────

    #[tokio::test]
    async fn ipc_server_multiple_clients() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        let mut stream1 = connect(&path).await;
        let mut stream2 = connect(&path).await;

        // Start via client 1
        send_request(
            &mut stream1,
            &IpcRequest::Start {
                config: crate::models::StartConfig::default(),
            },
        )
        .await;
        let resp1 = read_response(&mut stream1).await;
        assert!(resp1.ok);

        // Consume broadcast Started event on client 1
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream1),
        )
        .await;

        // Client 2 also receives the broadcast Started event
        let _ = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream2),
        )
        .await;

        // Client 2 can query is_running
        send_request(&mut stream2, &IpcRequest::IsRunning).await;
        let resp2 = read_response(&mut stream2).await;
        assert!(resp2.ok);
        assert_eq!(resp2.data.unwrap()["running"], true);

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── TR4: Graceful shutdown via CancellationToken ───────────────────

    #[tokio::test]
    async fn ipc_server_graceful_shutdown() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        // Server is running — client can connect
        let result = UnixStream::connect(&path).await;
        assert!(result.is_ok(), "should connect before shutdown");

        // Trigger graceful shutdown
        shutdown.cancel();

        // run() should return cleanly
        let _ = handle.await;

        // New connections should fail (socket is closed)
        tokio::time::sleep(Duration::from_millis(50)).await;
        let result = UnixStream::connect(&path).await;
        assert!(
            result.is_err(),
            "should not connect after graceful shutdown"
        );
    }

    // ── TR6: Events broadcast to all connected clients ─────────────────

    #[tokio::test]
    async fn ipc_server_broadcasts_events_to_all_clients() {
        let (server, path, shutdown) = setup_server();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move { server.run(s).await });

        // Connect two clients
        let mut stream1 = connect(&path).await;
        let mut stream2 = connect(&path).await;

        // Start via client 1
        send_request(
            &mut stream1,
            &IpcRequest::Start {
                config: crate::models::StartConfig::default(),
            },
        )
        .await;
        let resp1 = read_response(&mut stream1).await;
        assert!(resp1.ok);

        // Client 1 should get Started event (broadcast)
        let event1 = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream1),
        )
        .await
        .expect("client 1 timed out waiting for Started event");
        assert!(
            matches!(event1, IpcEvent::Started),
            "Client 1: expected Started, got {:?}",
            event1
        );

        // Client 2 should ALSO get Started event (broadcast)
        let event2 = tokio::time::timeout(
            Duration::from_millis(500),
            read_event(&mut stream2),
        )
        .await
        .expect("client 2 timed out waiting for broadcast Started event");
        assert!(
            matches!(event2, IpcEvent::Started),
            "Client 2: expected broadcast Started, got {:?}",
            event2
        );

        shutdown.cancel();
        let _ = handle.await;
    }

    // ── Additional: Bind removes stale socket ──────────────────────────

    #[tokio::test]
    async fn ipc_server_bind_removes_stale_socket() {
        let path = unique_socket_path();
        let app = tauri::test::mock_app();
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);

        // Create a stale file at the socket path
        std::fs::write(&path, b"stale").unwrap();
        assert!(path.exists());

        // Bind should succeed by removing the stale file
        let result = IpcServer::bind(path.clone(), cmd_tx, app.handle().clone());
        assert!(result.is_ok(), "bind should remove stale socket");

        // Clean up
        let _ = std::fs::remove_file(&path);
    }
}
