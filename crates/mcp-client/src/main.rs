use anyhow::Result;
use mcp_client::client::{ClientCapabilities, ClientInfo, Error as ClientError, McpClient};
use mcp_client::{service::TransportService, transport::StdioTransport};
use tower::ServiceBuilder;
use tracing_subscriber::EnvFilter;

// use mcp_client::{
//     service::{ServiceError},
//     transport::{Error as TransportError},
// };
// use std::time::Duration;
// use tower::timeout::error::Elapsed;

// fn convert_box_error(err: Box<dyn std::error::Error + Send + Sync>) -> ServiceError {
//     if let Some(elapsed) = err.downcast_ref::<Elapsed>() {
//         ServiceError::Transport(TransportError::Io(
//             std::io::Error::new(
//                 std::io::ErrorKind::TimedOut,
//                 format!("Timeout elapsed: {}", elapsed),
//             ),
//         ))
//     } else {
//         ServiceError::Other(err.to_string())
//     }
// }

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

    // Create the base transport
    let transport = StdioTransport::new("uvx", ["mcp-server-git"]);

    // Build service with middleware
    let service = ServiceBuilder::new().service(TransportService::new(transport));

    // Create client
    let mut client = McpClient::new(service);

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
    println!("Connected to server: {server_info:?}");

    // List resources
    let resources = client.list_resources().await?;
    println!("Available resources: {resources:?}");

    // Read a resource
    let content = client.read_resource("file:///example.txt".into()).await?;
    println!("Content: {content:?}");

    Ok(())
}
