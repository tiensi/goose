use mcp_client::{
    client::{ClientCapabilities, ClientInfo, McpClient, McpClientImpl},
    service::TransportService,
    transport::{SseTransport, StdioTransport},
};
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("mcp_client=debug".parse().unwrap())
                .add_directive("reqwest_eventsource=debug".parse().unwrap()),
        )
        .init();

    // Create two separate clients with stdio transport
    let client1 = create_stdio_client("client1", "1.0.0")?;
    let client2 = create_stdio_client("client2", "1.0.0")?;
    let client3 = create_sse_client("client3", "1.0.0")?;

    // Initialize both clients
    let mut clients: Vec<Box<dyn McpClient>> = Vec::new();
    clients.push(client1);
    clients.push(client2);
    clients.push(client3);

    // Initialize all clients
    for (i, client) in clients.iter_mut().enumerate() {
        let info = ClientInfo {
            name: format!("example-client-{}", i + 1),
            version: "1.0.0".to_string(),
        };
        let capabilities = ClientCapabilities::default();

        println!("\nInitializing client {}", i + 1);
        let init_result = client.initialize(info, capabilities).await?;
        println!("Client {} initialized: {:?}", i + 1, init_result);
    }

    // List tools for each client
    for (i, client) in clients.iter_mut().enumerate() {
        let tools = client.list_tools().await?;
        println!("\nClient {} tools: {:?}", i + 1, tools);
    }

    Ok(())
}

fn create_stdio_client(
    _name: &str,
    _version: &str,
) -> Result<Box<dyn McpClient>, Box<dyn std::error::Error>> {
    let transport = StdioTransport::new("uvx", vec!["mcp-server-git".to_string()]);
    // TODO: Add timeout middleware
    let service = ServiceBuilder::new().service(TransportService::new(transport));
    Ok(Box::new(McpClientImpl::new(service)))
}

fn create_sse_client(
    _name: &str,
    _version: &str,
) -> Result<Box<dyn McpClient>, Box<dyn std::error::Error>> {
    let transport = SseTransport::new("http://localhost:8000/sse");
    // TODO: Add timeout middleware
    let service = ServiceBuilder::new().service(TransportService::new(transport));
    Ok(Box::new(McpClientImpl::new(service)))
}
