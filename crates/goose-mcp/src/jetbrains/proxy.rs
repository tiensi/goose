use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use reqwest::Client;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{info, error, debug};
use mcp_core::{Content, Tool};

const PORT_RANGE_START: u16 = 63342;
const PORT_RANGE_END: u16 = 63352;
const ENDPOINT_CHECK_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Serialize, Deserialize)]
struct IDEResponseOk {
    status: String,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IDEResponseErr {
    status: Option<String>,
    error: String,
}

#[derive(Debug, Serialize)]
pub struct CallToolResult {
    pub content: Vec<Content>,
    pub is_error: bool,
}

#[derive(Debug)]
pub struct JetBrainsProxy {
    cached_endpoint: Arc<RwLock<Option<String>>>,
    previous_response: Arc<RwLock<Option<String>>>,
    client: Client,
}

impl JetBrainsProxy {
    pub fn new() -> Self {
        Self {
            cached_endpoint: Arc::new(RwLock::new(None)),
            previous_response: Arc::new(RwLock::new(None)),
            client: Client::new(),
        }
    }

    async fn test_list_tools(&self, endpoint: &str) -> Result<bool> {
        debug!("Sending test request to {}/mcp/list_tools", endpoint);
        
        let response = match self.client.get(&format!("{}/mcp/list_tools", endpoint)).send().await {
            Ok(resp) => resp,
            Err(e) => {
                debug!("Error testing endpoint {}: {}", endpoint, e);
                return Ok(false);
            }
        };

        if !response.status().is_success() {
            debug!("Test request failed with status {}", response.status());
            return Ok(false);
        }

        let current_response = response.text().await?;
        debug!("Received response: {:.100}...", current_response);

        let mut prev_response = self.previous_response.write().await;
        if let Some(prev) = prev_response.as_ref() {
            if prev != &current_response {
                debug!("Response changed since last check");
                self.send_tools_changed().await;
            }
        }
        *prev_response = Some(current_response);

        Ok(true)
    }

    async fn find_working_ide_endpoint(&self) -> Result<String> {
        debug!("Attempting to find working IDE endpoint...");

        // Check IDE_PORT environment variable first
        if let Ok(port) = env::var("IDE_PORT") {
            let test_endpoint = format!("http://127.0.0.1:{}/api", port);
            if self.test_list_tools(&test_endpoint).await? {
                debug!("IDE_PORT {} is working", port);
                return Ok(test_endpoint);
            }
            return Err(anyhow!("Specified IDE_PORT={} is not responding correctly", port));
        }

        // Scan port range
        for port in PORT_RANGE_START..=PORT_RANGE_END {
            let candidate_endpoint = format!("http://127.0.0.1:{}/api", port);
            debug!("Testing port {}...", port);
            
            if self.test_list_tools(&candidate_endpoint).await? {
                debug!("Found working IDE endpoint at {}", candidate_endpoint);
                return Ok(candidate_endpoint);
            }
        }

        Err(anyhow!("No working IDE endpoint found in range {}-{}", 
            PORT_RANGE_START, PORT_RANGE_END))
    }

    async fn update_ide_endpoint(&self) {
        match self.find_working_ide_endpoint().await {
            Ok(endpoint) => {
                let mut cached = self.cached_endpoint.write().await;
                *cached = Some(endpoint);
                debug!("Updated cached endpoint: {:?}", *cached);
            }
            Err(e) => {
                error!("Failed to update IDE endpoint: {}", e);
            }
        }
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let endpoint = self.cached_endpoint.read().await
            .clone()
            .ok_or_else(|| anyhow!("No working IDE endpoint available"))?;

        let response = self.client
            .get(&format!("{}/mcp/list_tools", endpoint))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch tools with status {}", response.status()));
        }

        let tools_response: Value = response.json().await?;
        let tools = tools_response
            .as_array()
            .ok_or_else(|| anyhow!("Invalid tools response format"))?
            .iter()
            .filter_map(|t| {
                if let (Some(name), Some(description), Some(input_schema)) = (t["name"].as_str(), t["description"].as_str(), t["input_schema"].as_str()) {
                    Some(Tool {
                        name: name.to_string(),
                        description: description.to_string(),
                        input_schema: serde_json::json!(input_schema),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(tools)
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult> {
        let endpoint = self.cached_endpoint.read().await
            .clone()
            .ok_or_else(|| anyhow!("No working IDE endpoint available"))?;

        debug!("ENDPOINT: {} | Tool name: {} | args: {}", endpoint, name, args);

        let response = self.client
            .post(&format!("{}/mcp/{}", endpoint, name))
            .json(&args)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Response failed: {}", response.status()));
        }

        let ide_response: Value = response.json().await?;
        let (is_error, text) = match ide_response {
            Value::Object(map) => {
                let status = map.get("status").and_then(|v| v.as_str());
                let error = map.get("error").and_then(|v| v.as_str());
                
                match (status, error) {
                    (Some(s), None) => (false, s.to_string()),
                    (None, Some(e)) => (true, e.to_string()),
                    _ => return Err(anyhow!("Invalid response format from IDE")),
                }
            }
            _ => return Err(anyhow!("Unexpected response type from IDE")),
        };

        Ok(CallToolResult {
            content: vec![Content::text(text)],
            is_error,
        })
    }

    async fn send_tools_changed(&self) {
        debug!("Sending tools changed notification");
        // TODO: Implement notification mechanism when needed
    }

    pub async fn start(&self) -> Result<()> {
        info!("Initializing JetBrains Proxy...");
        
        // Initial endpoint check
        self.update_ide_endpoint().await;

        // Schedule periodic endpoint checks
        let proxy = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(ENDPOINT_CHECK_INTERVAL).await;
                proxy.update_ide_endpoint().await;
            }
        });

        info!("JetBrains Proxy running");
        Ok(())
    }
}

impl Clone for JetBrainsProxy {
    fn clone(&self) -> Self {
        Self {
            cached_endpoint: Arc::clone(&self.cached_endpoint),
            previous_response: Arc::clone(&self.previous_response),
            client: Client::new(),
        }
    }
}
