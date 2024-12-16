use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::sleep;

use mcp_core::protocol::{
    JsonRpcRequest, JsonRpcResponse, ServerCapabilities, InitializeResult, Implementation,
};
use tower_service::Service;

use crate::{RouterError, BoxError};

/// Router service that implements the JSON-RPC protocol
pub struct Router {
    capabilities: ServerCapabilities,
}

impl Router {
    pub fn new() -> Self {
        Self {
            capabilities: ServerCapabilities {
                prompts: None,
                resources: None,
                tools: None,
            },
        }
    }
}

impl Service<JsonRpcRequest> for Router {
    type Response = JsonRpcResponse;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: JsonRpcRequest) -> Self::Future {
        let capabilities = self.capabilities.clone();
        let fut = async move {
            match req.method.as_str() {
                "slow" => {
                    // Sleep for 60 seconds - this should trigger the timeout
                    sleep(Duration::from_secs(60)).await;
                    Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: Some(serde_json::Value::String("This should never be seen".to_string())),
                        error: None,
                    })
                },
                "initialize" => {
                    let result = InitializeResult {
                        protocol_version: "0.1.0".to_string(),
                        capabilities,
                        server_info: Implementation {
                            name: "mcp-server".to_string(),
                            version: env!("CARGO_PKG_VERSION").to_string(),
                        },
                    };

                    let result_value = serde_json::to_value(result)
                        .map_err(|e| RouterError::Internal(format!("JSON serialization error: {}", e)))?;
                    
                    Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: Some(result_value),
                        error: None,
                    })
                },
                _ => {
                    let error = RouterError::MethodNotFound(req.method);
                    Ok(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: req.id,
                        result: None,
                        error: Some(error.into()),
                    })
                },
            }
        };

        Box::pin(fut)
    }
}
