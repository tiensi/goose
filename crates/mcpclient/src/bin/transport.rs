use mcpclient::{
    sse_transport::{SSEServerParams, SSETransport},
    stdio_transport::{StdioServerParams, StdioTransport},
    transport::Transport,
};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send>> {
    // For stdio
    let transport = StdioTransport {
        params: StdioServerParams {
            command: "uvx".into(),
            args: vec!["mcp-server-git".into()],
            env: None,
        },
    };

    // // Or for SSE
    // let transport = SSETransport {
    //     params: SSEServerParams {
    //         url: Url::parse("http://0.0.0.0:8000/sse")
    //             .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?,
    //         headers: None,
    //         timeout: std::time::Duration::from_secs(30),
    //         sse_read_timeout: std::time::Duration::from_secs(300),
    //     }
    // };

    let (mut read_stream, write_stream) = transport.connect().await?;

    // Use the streams
    while let Some(message) = read_stream.recv().await {
        match message {
            Ok(msg) => println!("Received: {:?}", msg),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    Ok(())
}
