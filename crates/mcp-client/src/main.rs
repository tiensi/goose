use anyhow::{anyhow, Result};
use clap::Parser;
use mcp_client::{
    session::Session,
    sse_transport::{SseTransport, SseTransportParams},
    stdio_transport::{StdioServerParams, StdioTransport},
    transport::Transport,
};
use serde_json::json;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Mode to run in: "git" or "echo"
    #[arg(short, long, default_value = "git")]
    mode: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("mcp_client=debug".parse().unwrap())
                .add_directive("reqwest_eventsource=debug".parse().unwrap()),
        )
        .init();

    let args = Args::parse();
    println!("Args - mode: {}", args.mode);

    // Create session based on mode
    let transport: Box<dyn Transport> = match args.mode.as_str() {
        "git" => Box::new(StdioTransport {
            params: StdioServerParams {
                command: "uvx".into(),
                args: vec!["mcp-server-git".into()],
                env: None,
            },
        }),
        "echo" => Box::new(SseTransport {
            params: SseTransportParams {
                url: "http://localhost:8000/sse".into(),
                headers: None,
            },
        }),
        _ => return Err(anyhow!("Invalid mode. Use 'git' or 'echo'")),
    };

    let (read_stream, write_stream) = transport.connect().await?;
    let mut session = Session::new(read_stream, write_stream).await?;

    // Initialize the connection
    let init_result = session.initialize().await?;
    println!("Initialized: {:?}", init_result);

    // List tools
    let tools = session.list_tools().await?;
    println!("Tools: {:?}", tools);

    if args.mode == "echo" {
        // Call a tool (replace with actual tool name and arguments)
        let call_result = session
            .call_tool("echo_tool", Some(json!({"message": "Hello, world!"})))
            .await?;
        println!("Call tool result: {:?}", call_result);

        // List available resources
        let resources = session.list_resources().await?;
        println!("Resources: {:?}", resources);

        // Read a resource (replace with actual URI)
        if let Some(resource) = resources.resources.first() {
            let read_result = session.read_resource(&resource.uri).await?;
            println!("Read resource result: {:?}", read_result);
        }
    } else {
        // Call a tool (replace with actual tool name and arguments)
        let call_result = session
            .call_tool("git_status", Some(json!({"repo_path": "."})))
            .await?;
        println!("Call tool result: {:?}", call_result);
    }

    session.shutdown().await?;
    println!("Done!");

    Ok(())
}
