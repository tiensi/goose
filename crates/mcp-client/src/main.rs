use anyhow::Result;
use mcp_client::client::{ClientCapabilities, ClientInfo, Error as ClientError, McpClient};
use mcp_client::{service::TransportService, transport::StdioTransport};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), ClientError> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("mcp_client=debug".parse().unwrap())
                .add_directive("reqwest_eventsource=debug".parse().unwrap()),
        )
        .init();

    // Create the base transport as Arc<Mutex<StdioTransport>>
    let transport = Arc::new(Mutex::new(StdioTransport::new("uvx", ["mcp-server-git"])));

    // Build service with middleware
    let service = ServiceBuilder::new().service(TransportService::new(Arc::clone(&transport)));

    // Create client
    let mut client = McpClient::new(service, Arc::clone(&transport));

    // Initialize
    let server_info = client
        .initialize(
            ClientInfo {
                name: "test-client".into(),
                version: "1.0.0".into(),
            },
            ClientCapabilities::default(),
        )
        .await?;
    println!("Connected to server: {server_info:?}\n");

    // List tools
    let tools = client.list_tools().await?;
    println!("Available tools: {tools:?}");

    Ok(())
}
