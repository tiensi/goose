use anyhow::Result;
use mcp_server::{Router, Server, ByteTransport};
use tokio::io::{stdin, stdout};
use tokio::time::Duration;
use tower::timeout::Timeout;

#[tokio::main]
async fn main() -> Result<()> {
    let router = Router::new();
    // Add a 30 second timeout to all requests
    let router = Timeout::new(router, Duration::from_secs(30));
    let server = Server::new(router);
    
    let transport = ByteTransport::new(stdin(), stdout());
    Ok(server.run(transport).await?)
}
