use clap::Parser;
use mcpclient::{
    session::Session,
    sse_transport::{SSEServerParams, SSETransport},
    stdio_transport::{StdioServerParams, StdioTransport},
    transport::Transport,
};
use serde_json::json;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Mode to run in: "git" or "echo"
    #[arg(short, long, default_value = "git")]
    mode: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        "echo" => Box::new(SSETransport::new(SSEServerParams {
            url: reqwest::Url::parse("http://0.0.0.0:8000").unwrap(),
            headers: None,
            timeout: std::time::Duration::from_secs(30),
            sse_read_timeout: std::time::Duration::from_secs(300),
        })),
        _ => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid mode. Use 'git' or 'echo'",
            )) as Box<dyn std::error::Error>)
        }
    };

    let (read_stream, write_stream) = transport.connect().await.map_err(|e| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    let mut session = Session::new(read_stream, write_stream).await.map_err(|e| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;

    // Initialize the connection
    let init_result = session.initialize().await.map_err(|e| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    println!("Initialized: {:?}", init_result);
    // List tools
    let tools = session.list_tools().await.map_err(|e| {
        Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ))
    })?;
    println!("Tools: {:?}", tools);

    if args.mode == "echo" {
        // Call a tool (replace with actual tool name and arguments)
        let call_result = session
            .call_tool("echo_tool", Some(json!({"message": "Hello, world!"})))
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
        println!("Call tool result: {:?}", call_result);

        // List available resources
        let resources = session.list_resources().await.map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
        println!("Resources: {:?}", resources);

        // Read a resource (replace with actual URI)
        if let Some(resource) = resources.resources.first() {
            let read_result = session.read_resource(&resource.uri).await.map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
            println!("Read resource result: {:?}", read_result);
        }
    } else {
        // Call a tool (replace with actual tool name and arguments)
        let call_result = session
            .call_tool("git_status", Some(json!({"repo_path": "."})))
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            })?;
        println!("Call tool result: {:?}", call_result);
    }

    println!("Done!");

    Ok(())
}
