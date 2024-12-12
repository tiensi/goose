use crate::transport::{ReadStream, WriteStream};
use crate::types::*;
use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

struct OutgoingMessage {
    message: JsonRpcMessage,
    response_tx: mpsc::Sender<Result<Option<JsonRpcResponse>>>,
}

pub struct Session {
    request_tx: mpsc::Sender<OutgoingMessage>,
    id_counter: AtomicU64,
}

impl Session {
    pub async fn new(read_stream: ReadStream, write_stream: WriteStream) -> Result<Self> {
        let (request_tx, mut request_rx) = mpsc::channel::<OutgoingMessage>(32);

        tokio::spawn(async move {
            let mut pending_requests = Vec::new();
            let mut read_stream = read_stream;
            let write_stream = write_stream;

            loop {
                tokio::select! {
                    Some(outgoing) = request_rx.recv() => {
                        // Send the message
                        if let Err(e) = write_stream.send(outgoing.message.clone()).await {
                            let _ = outgoing.response_tx.send(Err(e.into())).await;
                            continue;
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
                                println!("Unexpected message type");
                            }
                            Err(e) => {
                                eprintln!("Error receiving message: {}", e);
                            }
                        }
                    }
                }
            }
        });

        Ok(Self {
            request_tx,
            id_counter: AtomicU64::new(1),
        })
    }

    async fn send_message(&self, message: JsonRpcMessage) -> Result<Option<JsonRpcResponse>> {
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
