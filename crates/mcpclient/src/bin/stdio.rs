// Run it with `cargo run -p mcpclient --bin stdio`
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<u64>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

struct StdioClient {
    process: Child,
    writer: BufWriter<tokio::process::ChildStdin>,
    reader: BufReader<tokio::process::ChildStdout>,
}

impl StdioClient {
    async fn new(command: &str, args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut process = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = process.stdin.take().expect("Failed to get stdin");
        let stdout = process.stdout.take().expect("Failed to get stdout");

        Ok(Self {
            process,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout),
        })
    }

    async fn send_request(&mut self, request: &JsonRpcRequest) -> Result<(), std::io::Error> {
        let json = serde_json::to_string(&request)?;
        println!("\nSending: {}", json);
        self.writer.write_all(json.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn read_response(&mut self) -> Result<JsonRpcResponse, Box<dyn std::error::Error>> {
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;
        println!("\nReceived: {}", line);
        let response: JsonRpcResponse = serde_json::from_str(&line)?;
        Ok(response)
    }

    // close the process
    async fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.process.kill().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = StdioClient::new("uvx", &["mcp-server-git"]).await?;
    // let mut client = StdioClient::new("uv", &["run", "--with", "fastmcp", "fastmcp", "run", "/Users/smohammed/Development/mcp/echo.py"]).await?;

    // Send initialize request
    let init_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(1),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "sampling": null,
                "experimental": null,
                "roots": {
                    "listChanged": true
                }
            },
            "clientInfo": {
                "name": "RustMCPClient",
                "version": "0.1.0"
            }
        })),
    };

    client.send_request(&init_request).await?;
    let response = client.read_response().await?;
    println!("Initialize response: {:?}", response);

    // Send initialized notification
    let init_notification = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: "notifications/initialized".to_string(),
        params: None,
    };
    client.send_request(&init_notification).await?;

    // // List resources request
    // let list_resources_request = JsonRpcRequest {
    //     jsonrpc: "2.0".to_string(),
    //     id: Some(2),
    //     method: "resources/list".to_string(),
    //     params: Some(serde_json::json!({})),
    // };
    // client.send_request(&list_resources_request).await?;
    // let response = client.read_response().await?;
    // println!("List resources response: {:?}", response);

    // List tools request
    let list_tools_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(3),
        method: "tools/list".to_string(),
        params: None,
    };
    client.send_request(&list_tools_request).await?;
    let response = client.read_response().await?;
    println!("List tools response: {:?}", response);

    // Git status request
    let git_status_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(4),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "git_status",
            "arguments": {
                "repo_path": "."
            }
        })),
    };

    client.send_request(&git_status_request).await?;
    let response = client.read_response().await?;
    println!("Git status response: {:?}", response);

    // Git log request
    let git_log_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(5),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "git_log",
            "arguments": {
                "repo_path": ".",
                "max_count": 5
            }
        })),
    };

    client.send_request(&git_log_request).await?;
    let response = client.read_response().await?;
    println!("Git log response: {:?}", response);

    // sleep for 1 second and then close the process
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    client.close().await?;

    Ok(())
}
