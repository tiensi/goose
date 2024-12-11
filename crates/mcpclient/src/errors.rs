use crate::types::ErrorData;

#[derive(Debug)]
pub struct McpError {
    pub error: ErrorData,
}

impl McpError {
    pub fn new(error: ErrorData) -> Self {
        Self { error }
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "McpError {}: {}", self.error.code, self.error.message)
    }
}

impl std::error::Error for McpError {}

impl From<ErrorData> for McpError {
    fn from(error: ErrorData) -> Self {
        McpError { error }
    }
}
