use anyhow::Result;
use mcp_core::{
    handler::ToolError,
    tool::Tool,
    protocol::ServerCapabilities,
};
use mcp_server::{Router, Server, ByteTransport};
use mcp_server::router::{RouterService, CapabilitiesBuilder};
use serde_json::Value;
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::{io::{stdin, stdout}, sync::Mutex};
use tracing_subscriber::{self, EnvFilter};
use tracing_appender::rolling::{RollingFileAppender, Rotation};

// A simple counter service that demonstrates the Router trait
#[derive(Clone)]
struct CounterRouter {
    counter: Arc<Mutex<i32>>,
}

impl CounterRouter {
    fn new() -> Self {
        Self {
            counter: Arc::new(Mutex::new(0)),
        }
    }

    async fn increment(&self) -> Result<i32, ToolError> {
        let mut counter = self.counter.lock().await;
        *counter += 1;
        Ok(*counter)
    }

    async fn decrement(&self) -> Result<i32, ToolError> {
        let mut counter = self.counter.lock().await;
        *counter -= 1;
        Ok(*counter)
    }

    async fn get_value(&self) -> Result<i32, ToolError> {
        let counter = self.counter.lock().await;
        Ok(*counter)
    }
}

impl Router for CounterRouter {
    fn capabilities(&self) -> ServerCapabilities {
        CapabilitiesBuilder::new()
            .with_tools(true)
            .build()
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool::new(
                "increment".to_string(),
                "Increment the counter by 1".to_string(),
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            ),
            Tool::new(
                "decrement".to_string(),
                "Decrement the counter by 1".to_string(),
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            ),
            Tool::new(
                "get_value".to_string(),
                "Get the current counter value".to_string(),
                serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            ),
        ]
    }

    fn call_tool(&self, tool_name: &str, _arguments: Value) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + 'static>> {
        let this = self.clone();
        let tool_name = tool_name.to_string();

        Box::pin(async move {
            match tool_name.as_str() {
                "increment" => {
                    let value = this.increment().await?;
                    Ok(Value::Number(value.into()))
                }
                "decrement" => {
                    let value = this.decrement().await?;
                    Ok(Value::Number(value.into()))
                }
                "get_value" => {
                    let value = this.get_value().await?;
                    Ok(Value::Number(value.into()))
                }
                _ => Err(ToolError::NotFound(format!("Tool {} not found", tool_name))),
            }
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up file appender for logging
    let file_appender = RollingFileAppender::new(
        Rotation::DAILY,
        "logs",
        "mcp-server.log",
    );

    // Initialize the tracing subscriber with file and stdout logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive(tracing::Level::INFO.into()))
        .with_writer(file_appender)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("Starting MCP server");

    // Create an instance of our counter router
    let router = RouterService(CounterRouter::new());
    
    // Create and run the server
    let server = Server::new(router);
    let transport = ByteTransport::new(stdin(), stdout());
    
    tracing::info!("Server initialized and ready to handle requests");
    Ok(server.run(transport).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;

    #[test]
    async fn test_counter_operations() {
        let router = CounterRouter::new();
        
        // Test increment
        let result = router.call_tool("increment", Value::Null).await.unwrap();
        assert_eq!(result, Value::Number(1.into()));
        
        // Test get_value
        let result = router.call_tool("get_value", Value::Null).await.unwrap();
        assert_eq!(result, Value::Number(1.into()));
        
        // Test decrement
        let result = router.call_tool("decrement", Value::Null).await.unwrap();
        assert_eq!(result, Value::Number(0.into()));
    }

    #[test]
    async fn test_invalid_tool() {
        let router = CounterRouter::new();
        let result = router.call_tool("invalid_tool", Value::Null).await;
        assert!(result.is_err());
    }
}
