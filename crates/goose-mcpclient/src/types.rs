#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct InitializeResult {
    pub protocolVersion: String,
    pub capabilities: ServerCapabilities,
    pub serverInfo: Implementation,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub prompts: Option<PromptsCapability>,
    pub resources: Option<ResourcesCapability>,
    pub tools: Option<ToolsCapability>,
    // Add other capabilities as needed
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptsCapability {
    pub listChanged: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    pub listChanged: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolsCapability {
    pub listChanged: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResourcesResult {
    pub resources: Vec<Resource>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mimeType: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContents>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceContents {
    pub uri: String,
    pub mimeType: Option<String>,
    #[serde(flatten)]
    pub content: ResourceContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceContent {
    Text { text: String },
    Blob { blob: String }, // Base64-encoded
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<Tool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: Option<String>,
    pub inputSchema: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<Content>,
    pub isError: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String, // Base64-encoded image data
        mimeType: String,
    },
    #[serde(rename = "resource")]
    EmbeddedResource { resource: ResourceContents },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmptyResult {}

// Add other types as needed...
