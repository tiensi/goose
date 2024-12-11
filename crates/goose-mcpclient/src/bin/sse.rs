// Run it with `cargo run -p goose-mcpclient --bin sse`
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // SSE endpoint
    let sse_url = "http://localhost:8000/sse";

    // Create an HTTP client
    let client = Client::builder().build()?;

    // Channel to send the endpoint URL from the SSE handler to the main task
    let (endpoint_sender, mut endpoint_receiver) = mpsc::channel(1);

    // Spawn a task to handle SSE events
    let sse_client = client.clone();
    tokio::spawn(async move {
        if let Err(e) = handle_sse_events(sse_client, sse_url, endpoint_sender).await {
            eprintln!("Error in SSE handler: {:?}", e);
        }
    });

    // Wait for the endpoint URL
    let endpoint_url = endpoint_receiver
        .recv()
        .await
        .ok_or("Failed to receive endpoint URL")?;
    println!("Received endpoint URL: {}", endpoint_url);

    // Construct the full endpoint URL
    let base_url = reqwest::Url::parse(sse_url)?;
    let full_endpoint_url = base_url.join(&endpoint_url)?.to_string();
    println!("Full endpoint URL: {}", full_endpoint_url);

    // // Now we can send HTTP POST requests to the endpoint URL
    // // For example, list resources
    // let _session_id = extract_session_id(&endpoint_url)?;

    // Send the 'initialize' request first
    let initialize_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(1),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                // Your client's capabilities
            },
            "clientInfo": {
                "name": "RustMCPClient",
                "version": "0.1.0"
            }
        })),
    };
    let response = send_request(&client, &full_endpoint_url, &initialize_request).await?;
    println!("Initialize request was {}", response);

    // Send the 'initialized' notification
    let initialized_notification = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: "notifications/initialized".to_string(),
        params: None,
    };

    // Send notification and expect acceptance
    let response = send_request(&client, &full_endpoint_url, &initialized_notification).await?;
    println!("Initialized notification was {}", response);

    // Example of sending a `resources/list` request
    let list_resources_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(2),
        method: "resources/list".to_string(),
        params: Some(serde_json::json!({"cursor": "123"})),
    };

    let response = send_request(&client, &full_endpoint_url, &list_resources_request).await?;
    println!("List resources request was {}", response);

    // You can also send other requests, e.g., `resources/read`
    let read_resource_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(3),
        method: "resources/read".to_string(),
        params: Some(serde_json::json!({
            "uri": "echo://fixedresource"
        })),
    };
    let response = send_request(&client, &full_endpoint_url, &read_resource_request).await?;
    println!("\nRead resource request was {}", response);

    // Try a tool invocation with the correct format
    let echo_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(4),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "echo_tool",
            "arguments": {
                "message": "Hello from Rust!"
            }
        })),
    };
    // Send tool request and expect acceptance
    let response = send_request(&client, &full_endpoint_url, &echo_request).await?;
    println!("Echo tool request was {}", response);

    // Keep the SSE connection alive to see the tool response
    println!("Waiting for events...");
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    Ok(())
}

async fn handle_sse_events(
    client: Client,
    sse_url: &str,
    mut endpoint_sender: mpsc::Sender<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("handle_sse_events started");

    let response = client
        .get(sse_url)
        .header("Accept", "text/event-stream")
        .send()
        .await?;

    println!("SSE connection status: {}", response.status());

    if !response.status().is_success() {
        return Err(format!("Failed to connect to SSE endpoint: {}", response.status()).into());
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(chunk) => {
                let chunk_str = String::from_utf8_lossy(&chunk);
                buffer.push_str(&chunk_str);

                // Process all complete events in the buffer
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer = buffer[pos + 1..].to_string();

                    // Ignore empty lines
                    if line.trim().is_empty() {
                        continue;
                    }

                    // println!("Received line: {}", line);
                    // println!("Remaining buffer: {}", buffer);

                    // Parse the line
                    if line.starts_with("event:") {
                        let event_type = line[6..].trim().to_string();
                        // Next line should be data
                        if let Some(pos) = buffer.find('\n') {
                            let data_line = buffer[..pos].to_string();
                            buffer = buffer[pos + 1..].to_string();

                            if data_line.starts_with("data:") {
                                let data = data_line[5..].trim().to_string();

                                println!("Parsed event: {}, data: {}", event_type, data);

                                match event_type.as_str() {
                                    "endpoint" => {
                                        // Send the endpoint URL back to the main task
                                        endpoint_sender.send(data.clone()).await?;
                                        // println!("Received endpoint URL: {}", data);
                                    }
                                    "message" => {
                                        // Handle the server message
                                        println!("Received server message: {}", data);

                                        // Parse the JSON-RPC response
                                        match serde_json::from_str::<JsonRpcResponse>(&data) {
                                            Ok(response) => {
                                                println!(
                                                    "Received JSON-RPC response: {:?}",
                                                    response
                                                );
                                                // Handle the response as needed
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "Failed to parse JSON-RPC response: {:?}",
                                                    e
                                                );
                                            }
                                        }
                                    }
                                    _ => {
                                        println!(
                                            "Received event '{}' with data: {}",
                                            event_type, data
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving chunk: {:?}", e);
            }
        }
    }

    Ok(())
}

async fn send_request(
    client: &Client,
    endpoint_url: &str,
    request: &JsonRpcRequest,
) -> Result<String, Box<dyn std::error::Error>> {
    // Log the request being sent
    println!("\nSending request to {}: {:?}", endpoint_url, request);

    let response = client.post(endpoint_url).json(request).send().await?;

    // Small delay to ensure server processes initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let status = response.status();
    let text = response.text().await?;

    if status != reqwest::StatusCode::ACCEPTED {
        return Err(format!("Request failed: {} - {}", status, text).into());
    }

    Ok(text)
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<u64>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}
