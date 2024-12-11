use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::ErrorData;

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub error: ErrorData,
}

impl From<JsonRpcError> for McpError {
    fn from(error: JsonRpcError) -> Self {
        McpError::new(error.error)
    }
}

#[derive(Debug)]
pub struct McpError {
    pub error: ErrorData,
}

impl McpError {
    pub fn new(error: ErrorData) -> Self {
        Self {
            error,
        }
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "McpError {}: {}", self.error.code, self.error.message)
    }
}

impl std::error::Error for McpError {}
