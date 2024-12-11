use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl From<JsonRpcError> for McpError {
    fn from(error: JsonRpcError) -> Self {
        McpError::new(error.code, &error.message)
    }
}

#[derive(Debug)]
pub struct McpError {
    pub code: i64,
    pub message: String,
}

impl McpError {
    pub fn new(code: i64, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "McpError {}: {}", self.code, self.message)
    }
}

impl std::error::Error for McpError {}
