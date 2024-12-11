use clap::Parser;
use mcpclient::session::ClientSession;
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

    // Create session based on mode
    let mut session = match args.mode.as_str() {
        "git" => ClientSession::new("uvx", &["mcp-server-git"]).await?,
        "echo" => {
            ClientSession::new(
                "uv",
                &[
                    "run",
                    "--with",
                    "fastmcp",
                    "fastmcp",
                    "run",
                    "/Users/smohammed/Development/mcp/echo.py",
                ],
            )
            .await?
        }
        _ => return Err("Invalid mode. Use 'git' or 'echo'".into()),
    };

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

    // Close the session
    session.close().await?;

    Ok(())
}
