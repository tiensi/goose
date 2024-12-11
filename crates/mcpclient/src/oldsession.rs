use crate::stdio_client::StdioClient;
use crate::types::*;
use crate::errors::McpError;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

pub struct Session {
    client: StdioClient,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
}

impl Session {
    pub async fn new(command: &str, args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let client = StdioClient::new(command, args).await?;
        let mut message_rx = client.message_receiver();
        let pending = Arc::new(Mutex::new(HashMap::new()));

        let session = Self {
            client,
            next_id: AtomicU64::new(1),
            pending: pending.clone(),
        };

        // Start the message handling task
        let pending_clone = pending.clone();
        tokio::spawn(async move {
            while let Ok(line) = message_rx.recv().await {
                println!("Received: {}", line);
                let message: Value = match serde_json::from_str(&line) {
                    Ok(msg) => msg,
                    Err(e) => {
                        println!("Failed to parse JSON: {}", e);
                        continue;
                    }
                };

                if let Some(id_value) = message.get("id") {
                    if id_value.is_null() {
                        // It's a notification
                        // Handle notifications if needed
                    } else if let Some(id) = id_value.as_u64() {
                        let response: JsonRpcResponse = serde_json::from_value(message).unwrap();
                        let mut pending = pending_clone.lock().await;
                        if let Some(sender) = pending.remove(&id) {
                            let _ = sender.send(response);
                        } else {
                            println!("Received response with unknown id: {}", id);
                        }
                    } else {
                        println!("Invalid id in message: {:?}", id_value);
                    }
                } else {
                    // Handle notifications if needed
                }
            }
        });

        Ok(session)
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse, Box<dyn std::error::Error>> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: method.to_string(),
            params,
        };

        let (sender, receiver) = oneshot::channel();

        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, sender);
        }

        self.client.send_request(&request).await?;

        let response = receiver.await?;
        Ok(response)
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let notification = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        };
        self.client.send_notification(&notification).await?;
        Ok(())
    }

    pub async fn initialize(&mut self) -> Result<InitializeResult, Box<dyn std::error::Error>> {
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
        let response = self.send_request("initialize", Some(params)).await?;
        if let Some(error) = response.error {
            Err(Box::new(McpError::from(error)))
        } else if let Some(result) = response.result {
            let init_result: InitializeResult = serde_json::from_value(result)?;
            // Send initialized notification
            self.send_notification("notifications/initialized", None)
                .await?;
            Ok(init_result)
        } else {
            Err(Box::new(McpError::new(ErrorData {
                code: -1,
                message: "No result in response".to_string(),
                data: None,
            })))
        }
    }

    pub async fn list_resources(
        &mut self,
    ) -> Result<ListResourcesResult, Box<dyn std::error::Error>> {
        let params = json!({});
        let response = self.send_request("resources/list", Some(params)).await?;
        if let Some(error) = response.error {
            Err(Box::new(McpError::from(error)))
        } else if let Some(result) = response.result {
            let resources_result: ListResourcesResult = serde_json::from_value(result)?;
            Ok(resources_result)
        } else {
            Err(Box::new(McpError::new(ErrorData {
                code: -1,
                message: "No result in response".to_string(),
                data: None,
            })))
        }
    }

    pub async fn read_resource(
        &mut self,
        uri: &str,
    ) -> Result<ReadResourceResult, Box<dyn std::error::Error>> {
        let params = json!({
            "uri": uri,
        });
        let response = self.send_request("resources/read", Some(params)).await?;
        if let Some(error) = response.error {
            Err(Box::new(McpError::from(error)))
        } else if let Some(result) = response.result {
            let read_result: ReadResourceResult = serde_json::from_value(result)?;
            Ok(read_result)
        } else {
            Err(Box::new(McpError::new(ErrorData {
                code: -1,
                message: "No result in response".to_string(),
                data: None,
            })))
        }
    }

    pub async fn list_tools(&mut self) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
        let params = json!({});
        let response = self.send_request("tools/list", Some(params)).await?;
        if let Some(error) = response.error {
            Err(Box::new(McpError::from(error)))
        } else if let Some(result) = response.result {
            let tools_result: ListToolsResult = serde_json::from_value(result)?;
            Ok(tools_result)
        } else {
            Err(Box::new(McpError::new(ErrorData {
                code: -1,
                message: "No result in response".to_string(),
                data: None,
            })))
        }
    }

    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult, Box<dyn std::error::Error>> {
        let params = json!({
            "name": name,
            "arguments": arguments.unwrap_or_else(|| json!({})),
        });
        let response = self.send_request("tools/call", Some(params)).await?;
        if let Some(error) = response.error {
            Err(Box::new(McpError::from(error)))
        } else if let Some(result) = response.result {
            let call_result: CallToolResult = serde_json::from_value(result)?;
            Ok(call_result)
        } else {
            Err(Box::new(McpError::new(ErrorData {
                code: -1,
                message: "No result in response".to_string(),
                data: None,
            })))
        }
    }

    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client.close().await
    }
}
