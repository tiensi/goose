use std::pin::Pin;
use std::future::Future;

use goose_mcp::{DeveloperRouter, JetBrainsRouter};
use mcp_core::{
    protocol::{JsonRpcRequest, JsonRpcResponse, ServerCapabilities},
    Content, Resource, Tool, ToolError,
    handler::ResourceError,
};
use mcp_server::Router;

#[derive(Clone)]
pub enum RouterEnum {
    Developer(DeveloperRouter),
    JetBrains(JetBrainsRouter),
}

impl Router for RouterEnum {
    fn name(&self) -> String {
        match self {
            RouterEnum::Developer(router) => router.name(),
            RouterEnum::JetBrains(router) => router.name(),
        }
    }

    fn instructions(&self) -> String {
        match self {
            RouterEnum::Developer(router) => router.instructions(),
            RouterEnum::JetBrains(router) => router.instructions(),
        }
    }

    fn capabilities(&self) -> ServerCapabilities {
        match self {
            RouterEnum::Developer(router) => router.capabilities(),
            RouterEnum::JetBrains(router) => router.capabilities(),
        }
    }

    fn list_tools(&self) -> Vec<Tool> {
        match self {
            RouterEnum::Developer(router) => router.list_tools(),
            RouterEnum::JetBrains(router) => router.list_tools(),
        }
    }

    fn call_tool(&self, name: &str, params: serde_json::Value) -> Pin<Box<dyn Future<Output = Result<Vec<Content>, ToolError>> + Send + 'static>> {
        match self {
            RouterEnum::Developer(router) => router.call_tool(name, params),
            RouterEnum::JetBrains(router) => router.call_tool(name, params),
        }
    }

    fn list_resources(&self) -> Vec<Resource> {
        match self {
            RouterEnum::Developer(router) => router.list_resources(),
            RouterEnum::JetBrains(router) => router.list_resources(),
        }
    }

    fn read_resource(&self, path: &str) -> Pin<Box<dyn Future<Output = Result<String, ResourceError>> + Send + 'static>> {
        match self {
            RouterEnum::Developer(router) => router.read_resource(path),
            RouterEnum::JetBrains(router) => router.read_resource(path),
        }
    }
}