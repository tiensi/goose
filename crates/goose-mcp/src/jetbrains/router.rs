use std::sync::Arc;
use anyhow::Result;
use mcp_core::{Tool, Resource, Content};
use mcp_core::handler::{ToolError, ResourceError};
use mcp_core::protocol::{ServerCapabilities, ToolsCapability, ResourcesCapability};
use mcp_server::Router;
use serde_json::Value;
use tracing::info;
use std::future::Future;
use std::pin::Pin;

use crate::jetbrains::proxy::JetBrainsProxy;

#[derive(Clone)]
pub struct JetBrainsRouter {
    proxy: Arc<JetBrainsProxy>,
    tools_cache: Arc<parking_lot::RwLock<Vec<Tool>>>,
}

impl JetBrainsRouter {
    pub fn new() -> Self {
        Self {
            proxy: Arc::new(JetBrainsProxy::new()),
            tools_cache: Arc::new(parking_lot::RwLock::new(Vec::new())),
        }
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting JetBrains Router...");
        // Initialize the tools cache
        if let Ok(tools) = self.proxy.list_tools().await {
            let mut cache = self.tools_cache.write();
            *cache = tools;
        }
        self.proxy.start().await
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
        ServerCapabilities {
            tools: Some(ToolsCapability {
                list_changed: Some(true),
            }),
            resources: Some(ResourcesCapability {
                list_changed: Some(false),
                subscribe: Some(false),
            }),
            prompts: None,
        }
    }

    fn list_tools(&self) -> Vec<Tool> {
        self.tools_cache.read().clone()
    }

    fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>> {
        let proxy = Arc::clone(&self.proxy);
        let name = tool_name.to_string();
        Box::pin(async move {
            match proxy.call_tool(&name, arguments).await {
                Ok(result) => Ok(result.content),
                Err(e) => Err(ToolError::ExecutionError(e.to_string())),
            }
        })
    }

    fn list_resources(&self) -> Vec<Resource> {
        vec![] // No static resources
    }

    fn read_resource(
        &self,
        uri: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ResourceError>> + Send + 'static>> {
        let uri = uri.to_string();
        Box::pin(async move {
            Err(ResourceError::NotFound(format!("Resource not found: {}", uri)))
        })
    }
}