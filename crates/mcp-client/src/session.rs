use crate::transport::{ReadStream, WriteStream};
use anyhow::{anyhow, Context, Result};
use mcp_core::types::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::debug;

struct OutgoingMessage {
    message: JsonRpcMessage,
    response_tx: mpsc::Sender<Result<Option<JsonRpcResponse>>>,
}

pub struct Session {
    request_tx: mpsc::Sender<OutgoingMessage>,
    id_counter: AtomicU64,
    shutdown_tx: mpsc::Sender<()>,
    background_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    is_closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Session {
    pub async fn new(read_stream: ReadStream, write_stream: WriteStream) -> Result<Self> {
        let (request_tx, mut request_rx) = mpsc::channel::<OutgoingMessage>(32);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let is_closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let is_closed_clone = is_closed.clone();

        // Spawn the background task
        let background_task = Arc::new(Mutex::new(Some(tokio::spawn({
            async move {
                let mut pending_requests: Vec<(
                    u64,
                    mpsc::Sender<Result<Option<JsonRpcResponse>>>,
                )> = Vec::new();
                let mut read_stream = read_stream;
                let write_stream = write_stream;

                loop {
                    tokio::select! {
                        // Handle shutdown signal
                        Some(()) = shutdown_rx.recv() => {
                            // Notify all pending requests of shutdown
                            for (_, tx) in pending_requests {
                                let _ = tx.send(Err(anyhow!("Session shutdown"))).await;
                            }
                            break;
                        }

                        // Handle outgoing messages
                        Some(outgoing) = request_rx.recv() => {
                            // If session is closed, reject new messages
                            if is_closed_clone.load(Ordering::SeqCst) {
                                let _ = outgoing.response_tx.send(Err(anyhow!("Session is closed"))).await;
                                continue;
                            }

                            // Send the message
                            if let Err(e) = write_stream.send(outgoing.message.clone()).await {
                                debug!("Write error occurred: {}", e);
                                // let _ = outgoing.response_tx.send(Err(e.into())).await;
                                // On write error, mark session as closed
                                is_closed_clone.store(true, Ordering::SeqCst);
                                break;
                            }

                            // For requests, store the response channel for later
                            if let JsonRpcMessage::Request(request) = outgoing.message {
                                if let Some(id) = request.id {
                                    pending_requests.push((id, outgoing.response_tx));
                                }
                            } else {
                                // For notifications, just confirm success
                                let _ = outgoing.response_tx.send(Ok(None)).await;
                            }
                        }

                        // Handle incoming messages
                        Some(message_result) = read_stream.recv() => {
                            match message_result {
                                Ok(JsonRpcMessage::Response(response)) => {
                                    if let Some(id) = response.id {
                                        if let Some(pos) = pending_requests.iter().position(|(req_id, _)| *req_id == id) {
                                            let (_, tx) = pending_requests.remove(pos);
                                            let _ = tx.send(Ok(Some(response))).await;
                                        }
                                    }
                                }
                                Ok(JsonRpcMessage::Notification(_)) => {
                                    // Handle incoming notifications if needed
                                }
                                Ok(_) => {
                                    eprintln!("Unexpected message type");
                                }
                                Err(e) => {
                                    // On transport error, notify all pending requests and shutdown
                                    eprintln!("Transport error: {}", e);
                                    for (_, tx) in pending_requests {
                                        let _ = tx.send(Err(anyhow!("{}", e))).await;
                                    }

                                    // Mark session as closed
                                    is_closed_clone.store(true, Ordering::SeqCst);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }))));

        Ok(Self {
            request_tx,
            id_counter: AtomicU64::new(1),
            shutdown_tx,
            background_task,
            is_closed,
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        // Mark session as closed
        self.is_closed.store(true, Ordering::SeqCst);

        // Send shutdown signal
        self.shutdown_tx
            .send(())
            .await
            .map_err(|e| anyhow!("Failed to shutdown session: {}", e))?;

        // Wait for background task to complete
        if let Some(task) = self.background_task.lock().await.take() {
            task.await
                .map_err(|e| anyhow!("Background task failed: {}", e))?;
        }

        Ok(())
    }

    async fn send_message(&self, message: JsonRpcMessage) -> Result<Option<JsonRpcResponse>> {
        // Check if session is closed
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(anyhow!("Session is closed"));
        }

        let (response_tx, mut response_rx) = mpsc::channel(1);

        self.request_tx
            .send(OutgoingMessage {
                message,
                response_tx,
            })
            .await
            .context("Failed to send message")?;

        response_rx
            .recv()
            .await
            .context("Failed to receive response")?
    }

    async fn rpc_call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<T> {
        // Check if session is closed
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(anyhow!("Session is closed"));
        }

        let id = self.id_counter.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: method.to_string(),
            params,
        };

        let response = self
            .send_message(JsonRpcMessage::Request(request))
            .await?
            .context("Expected response for request")?;

        match (response.error, response.result) {
            (Some(error), _) => Err(anyhow!("RPC Error {}: {}", error.code, error.message)),
            (_, Some(result)) => {
                serde_json::from_value(result).context("Failed to deserialize result")
            }
            (None, None) => Err(anyhow!("No result in response")),
        }
    }

    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        // Check if session is closed
        if self.is_closed.load(Ordering::SeqCst) {
            return Err(anyhow!("Session is closed"));
        }

        let notification = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };

        self.send_message(JsonRpcMessage::Notification(notification))
            .await?;

        Ok(())
    }

    pub async fn initialize(&mut self) -> Result<InitializeResult> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "sampling": null,
                "experimental": null,
                "roots": {
                    "listChanged": true
                }
            },
            "clientInfo": {
                "name": "RustMCPClient",
                "version": "0.1.0"
            }
        });

        let result: InitializeResult = self.rpc_call("initialize", Some(params)).await?;
        self.send_notification("notifications/initialized", None)
            .await?;
        Ok(result)
    }

    pub async fn list_resources(&self) -> Result<ListResourcesResult> {
        self.rpc_call("resources/list", Some(json!({}))).await
    }

    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult> {
        self.rpc_call("resources/read", Some(json!({ "uri": uri })))
            .await
    }

    pub async fn list_tools(&self) -> Result<ListToolsResult> {
        self.rpc_call("tools/list", Some(json!({}))).await
    }

    pub async fn call_tool(&self, name: &str, arguments: Option<Value>) -> Result<CallToolResult> {
        self.rpc_call(
            "tools/call",
            Some(json!({
                "name": name,
                "arguments": arguments.unwrap_or_else(|| json!({})),
            })),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{ReadStream, Transport, WriteStream};
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    // Mock transport that simulates errors
    struct MockTransport {
        error_mode: ErrorMode,
    }

    #[derive(Clone)]
    enum ErrorMode {
        ReadError,
        WriteError,
        ProcessTermination,
        Nil,
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn connect(&self) -> Result<(ReadStream, WriteStream)> {
            let (tx_read, rx_read) = mpsc::channel(100);
            let (tx_write, mut rx_write) = mpsc::channel(100);

            let error_mode = self.error_mode.clone();

            tokio::spawn(async move {
                // For WriteError, don't wait for any writes, just drop the receiver to force an immediate failure.
                // This ensures that the first attempt to send by the Session fails.
                match error_mode {
                    ErrorMode::ReadError => {
                        // Wait a bit for the request to be sent and then send the error
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        let _ = tx_read.send(Err(anyhow!("Simulated read error"))).await;
                    }
                    ErrorMode::WriteError => {
                        // Immediately drop the rx_write side
                        drop(rx_write);
                    }
                    ErrorMode::ProcessTermination => {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                        let _ = tx_read.send(Err(anyhow!("Child process terminated"))).await;
                    }
                    ErrorMode::Nil => {
                        // Test with initialize and then list_resources
                        while let Some(message) = rx_write.recv().await {
                            match message {
                                JsonRpcMessage::Request(req) => {
                                    // Send a successful response for initialization or other calls
                                    if req.method == "initialize" {
                                        let response = JsonRpcMessage::Response(JsonRpcResponse {
                                            jsonrpc: "2.0".to_string(),
                                            id: req.id,
                                            result: Some(json!({
                                                "protocolVersion": "2024-11-05",
                                                "capabilities": { "resources": { "listChanged": false } },
                                                "serverInfo": { "name": "MockServer", "version": "1.0.0" }
                                            })),
                                            error: None,
                                        });
                                        let _ = tx_read.send(Ok(response)).await;
                                    } else if req.method == "resources/list" {
                                        let response = JsonRpcMessage::Response(JsonRpcResponse {
                                            jsonrpc: "2.0".to_string(),
                                            id: req.id,
                                            result: Some(
                                                json!({ "resources": [{ "uri": "file://res1", "name": "res1" }, { "uri": "file://res2", "name": "res2" }] }),
                                            ),
                                            error: None,
                                        });
                                        let _ = tx_read.send(Ok(response)).await;
                                    } else {
                                        // Default success for other calls
                                        let response = JsonRpcMessage::Response(JsonRpcResponse {
                                            jsonrpc: "2.0".to_string(),
                                            id: req.id,
                                            result: Some(json!({ "ok": true })),
                                            error: None,
                                        });
                                        let _ = tx_read.send(Ok(response)).await;
                                    }
                                }
                                JsonRpcMessage::Notification(_notif) => {
                                    // For notifications, no response is required.
                                }
                                _ => {}
                            }
                        }
                    }
                }
            });

            Ok((rx_read, tx_write))
        }
    }

    #[tokio::test]
    async fn test_session_can_initialize_and_list_resources() -> Result<()> {
        let transport = MockTransport {
            error_mode: ErrorMode::Nil,
        };

        let (read_stream, write_stream) = transport.connect().await?;
        let mut session = Session::new(read_stream, write_stream).await?;

        // Initialize the session
        let init_result = session.initialize().await?;
        assert_eq!(init_result.protocol_version, "2024-11-05");
        assert_eq!(
            init_result.capabilities.resources.unwrap().list_changed,
            Some(false)
        );

        // Now list resources
        let list_result = session.list_resources().await?;
        assert_eq!(
            list_result
                .resources
                .iter()
                .map(|r| &r.name)
                .collect::<Vec<_>>(),
            vec!["res1", "res2"]
        );

        // Make another call - just to verify multiple calls work fine
        let _: serde_json::Value = session.rpc_call("someMethod", Some(json!({}))).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_read_error_terminates_session() {
        let transport = MockTransport {
            error_mode: ErrorMode::ReadError,
        };

        let (read_stream, write_stream) = transport.connect().await.unwrap();
        let session = Session::new(read_stream, write_stream).await.unwrap();

        // // Introduce a brief delay to ensure the request is fully sent and pending before the error occurs
        // tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        // Try to make an RPC call - should fail due to transport error
        let result = session.list_resources().await;
        assert!(result.is_err());

        // Print the actual error message for debugging
        let err = result.unwrap_err();
        println!("Actual error: {}", err);
        assert!(err.to_string().contains("Simulated read error"));

        // Verify session is marked as closed
        assert!(
            session.is_closed.load(Ordering::SeqCst),
            "Session did not close after error"
        );

        // Subsequent calls should fail immediately
        let result = session.list_tools().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Session is closed"));
    }

    #[tokio::test]
    async fn test_write_error_terminates_session() {
        let transport = MockTransport {
            error_mode: ErrorMode::WriteError,
        };

        let (read_stream, write_stream) = transport.connect().await.unwrap();
        let session = Session::new(read_stream, write_stream).await.unwrap();

        // Try to make an RPC call - should fail due to transport error
        let result = session.list_resources().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed to receive response"));

        // Verify session is marked as closed
        assert!(session.is_closed.load(Ordering::SeqCst));
        println!("First call made");

        // Subsequent calls should fail immediately
        let result = session.list_tools().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Session is closed"));
    }

    #[tokio::test]
    async fn test_process_termination_terminates_session() {
        let transport = MockTransport {
            error_mode: ErrorMode::ProcessTermination,
        };

        let (read_stream, write_stream) = transport.connect().await.unwrap();
        let session = Session::new(read_stream, write_stream).await.unwrap();

        // Try to make an RPC call - should fail due to process termination
        let result = session.list_resources().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Child process terminated"));

        // Verify session is marked as closed
        assert!(session.is_closed.load(Ordering::SeqCst));

        // Subsequent calls should fail immediately
        let result = session.list_tools().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Session is closed"));
    }

    #[tokio::test]
    async fn test_session_cleanup_on_drop() {
        let transport = MockTransport {
            error_mode: ErrorMode::ProcessTermination,
        };

        let (read_stream, write_stream) = transport.connect().await.unwrap();
        let session = Session::new(read_stream, write_stream).await.unwrap();

        // Get a clone of the background task handle
        let background_task = session.background_task.clone();

        // Drop the session
        drop(session);

        // Verify that the background task completes
        let timeout_result = timeout(Duration::from_secs(1), async {
            if let Some(task) = background_task.lock().await.take() {
                task.await.unwrap();
            }
        })
        .await;

        assert!(timeout_result.is_ok(), "Background task did not complete");
    }

    #[tokio::test]
    async fn test_explicit_shutdown() -> Result<()> {
        let transport = MockTransport {
            error_mode: ErrorMode::Nil,
        };

        let (read_stream, write_stream) = transport.connect().await?;
        let session = Session::new(read_stream, write_stream).await?;

        // Verify we can make calls before shutdown
        let _: serde_json::Value = session.rpc_call("someMethod", Some(json!({}))).await?;

        // Shutdown the session
        session.shutdown().await?;

        // Verify calls fail after shutdown
        let result = session.list_resources().await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Session is closed"));

        Ok(())
    }
}
