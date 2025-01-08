use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use anyhow::Result;
use mcp_core::{Tool, Resource, Content};
use mcp_core::handler::{ToolError, ResourceError};
use mcp_core::protocol::ServerCapabilities;
use mcp_server::Router;
use mcp_server::router::CapabilitiesBuilder;
use serde_json::Value;
use tracing::{info, warn, debug};
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

use crate::jetbrains::proxy::JetBrainsProxy;

#[derive(Clone)]
pub struct JetBrainsRouter {
    proxy: Arc<JetBrainsProxy>,
    tools_cache: Arc<parking_lot::RwLock<Vec<Tool>>>,
    initialized: Arc<AtomicBool>,
    init_notifier: Arc<Notify>,
}

impl JetBrainsRouter {
    pub fn new() -> Self {
        let router = Self {
            proxy: Arc::new(JetBrainsProxy::new()),
            tools_cache: Arc::new(parking_lot::RwLock::new(Vec::new())),
            initialized: Arc::new(AtomicBool::new(false)),
            init_notifier: Arc::new(Notify::new()),
        };

        // Spawn background initialization task
        let router_clone = router.clone();
        tokio::spawn(async move {
            if let Err(e) = router_clone.initialize().await {
                debug!("Background initialization failed: {}", e);
            }
        });

        router
    }

    async fn populate_tools_cache(&self) -> Result<()> {
        debug!("Attempting to populate tools cache...");
        
        // Try multiple times with delay
        for attempt in 1..=5 {
            debug!("Cache population attempt {} of 5", attempt);
            
            match self.proxy.list_tools().await {
                Ok(tools) => {
                    debug!("Successfully fetched {} tools from proxy", tools.len());
                    if tools.is_empty() {
                        debug!("Tools list is empty, will retry...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    let mut cache = self.tools_cache.write();
                    *cache = tools;
                    debug!("Tools cache updated successfully");
                    return Ok(());
                }
                Err(e) => {
                    debug!("Failed to fetch tools (attempt {}): {}", attempt, e);
                    if attempt < 5 {
                        debug!("Waiting before retry...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
        
        debug!("Failed to populate tools cache after all attempts");
        Err(anyhow::anyhow!("Failed to populate tools cache after 5 attempts"))
    }

    async fn initialize(&self) -> Result<()> {
        if self.initialized.load(Ordering::SeqCst) {
            debug!("Router already initialized");
            return Ok(());
        }

        debug!("Starting JetBrains Router initialization...");
        info!("Starting JetBrains Router...");
        
        // First start the proxy
        debug!("Starting proxy...");
        if let Err(e) = self.proxy.start().await {
            debug!("Failed to start proxy: {:?}", e);
            return Err(e);
        }
        debug!("Proxy started successfully");

        // Give the proxy a moment to initialize
        debug!("Waiting for proxy initialization...");
        tokio::time::sleep(Duration::from_secs(1)).await;
        
        // Then try to populate the tools cache
        if let Err(e) = self.populate_tools_cache().await {
            debug!("Warning: Initial tools cache population failed: {}", e);
            warn!("Initial tools cache population failed: {}", e);
        }

        self.initialized.store(true, Ordering::SeqCst);
        self.init_notifier.notify_waiters();
        debug!("Router initialization completed");
        
        Ok(())
    }
}

impl Router for JetBrainsRouter {
    fn name(&self) -> String {
        "jetbrains/proxy".to_string()
    }

    fn instructions(&self) -> String {
        "JetBrains IDE integration providing access to IDE features via MCP".to_string()
    }

    fn capabilities(&self) -> ServerCapabilities {
        CapabilitiesBuilder::new().with_tools(true).build()
    }

    fn list_tools(&self) -> Vec<Tool> {
        debug!("Accessing tools cache...");
        let tools = self.tools_cache.read().clone();
        
        if !tools.is_empty() {
            debug!("Returning {} tools from cache", tools.len());
            return tools;
        }

        debug!("Cache is empty, initialization may still be in progress");
        Vec::new()
    }

    fn call_tool(&self, tool_name: &str, arguments: Value) -> Pin<Box<dyn Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>> {
        let tool_name = tool_name.to_string();
        let proxy = self.proxy.clone();
        let init_notifier = self.init_notifier.clone();
        
        Box::pin(async move {
            // Wait for initialization before proceeding
            init_notifier.notified().await;
            
            debug!("Calling tool: {}", tool_name);
            match proxy.call_tool(&tool_name, arguments).await {
                Ok(result) => {
                    debug!("Tool {} completed successfully", tool_name);
                    Ok(result.content)
                }
                Err(e) => {
                    debug!("Tool {} failed: {}", tool_name, e);
                    Err(ToolError::ExecutionError(e.to_string()))
                }
            }
        })
    }

    fn list_resources(&self) -> Vec<Resource> {
        vec![] // No static resources
    }

    fn read_resource(&self, uri: &str) -> Pin<Box<dyn Future<Output = Result<String, ResourceError>> + Send + 'static>> {
        let uri = uri.to_string();
        Box::pin(async move {
            Err(ResourceError::NotFound(format!("Resource not found: {}", uri)))
        })
    }
}