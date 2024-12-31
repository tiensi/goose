use std::collections::HashMap;
use tokio::sync::Mutex;
use rust_decimal_macros::dec;

use crate::errors::{AgentError, AgentResult};
use crate::prompt_template::load_prompt_file;
use crate::systems::System;
use crate::providers::base::{Provider, ProviderUsage};
use mcp_core::{Content, Resource, Tool, ToolCall};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
struct SystemInfo {
    name: String,
    description: String,
    instructions: String,
}

impl SystemInfo {
    fn new(name: &str, description: &str, instructions: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            instructions: instructions.to_string(),
        }
    }
}

/// Manages MCP systems and their interactions
pub struct MCPManager {
    systems: Vec<Box<dyn System>>,
    provider: Box<dyn Provider>,
    provider_usage: Mutex<Vec<ProviderUsage>>,
}

impl MCPManager {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self { 
            systems: Vec::new(),
            provider,
            provider_usage: Mutex::new(Vec::new()),
        }
    }

    /// Get a reference to the provider
    pub fn provider(&self) -> &Box<dyn Provider> {
        &self.provider
    }

    /// Record provider usage
    pub async fn record_usage(&self, usage: ProviderUsage) {
        self.provider_usage.lock().await.push(usage);
    }

    /// Get aggregated usage statistics
    pub async fn get_usage(&self) -> anyhow::Result<Vec<ProviderUsage>> {
        let provider_usage = self.provider_usage.lock().await.clone();
        let mut usage_map: HashMap<String, ProviderUsage> = HashMap::new();

        provider_usage.iter().for_each(|usage| {
            usage_map
                .entry(usage.model.clone())
                .and_modify(|e| {
                    e.usage.input_tokens = Some(
                        e.usage.input_tokens.unwrap_or(0) + usage.usage.input_tokens.unwrap_or(0),
                    );
                    e.usage.output_tokens = Some(
                        e.usage.output_tokens.unwrap_or(0) + usage.usage.output_tokens.unwrap_or(0),
                    );
                    e.usage.total_tokens = Some(
                        e.usage.total_tokens.unwrap_or(0) + usage.usage.total_tokens.unwrap_or(0),
                    );
                    if e.cost.is_none() || usage.cost.is_none() {
                        e.cost = None; // Pricing is not available for all models
                    } else {
                        e.cost = Some(e.cost.unwrap_or(dec!(0)) + usage.cost.unwrap_or(dec!(0)));
                    }
                })
                .or_insert_with(|| usage.clone());
        });
        Ok(usage_map.into_values().collect())
    }

    /// Add a system to the manager
    pub fn add_system(&mut self, system: Box<dyn System>) {
        self.systems.push(system);
    }

    /// Remove a system by name
    pub fn remove_system(&mut self, name: &str) -> AgentResult<()> {
        if let Some(pos) = self.systems.iter().position(|sys| sys.name() == name) {
            self.systems.remove(pos);
            Ok(())
        } else {
            Err(AgentError::SystemNotFound(name.to_string()))
        }
    }

    /// List all systems and their status
    pub async fn list_systems(&self) -> AgentResult<Vec<(String, String)>> {
        let mut statuses = Vec::new();
        for system in &self.systems {
            let status = system
                .status()
                .await
                .map_err(|e| AgentError::Internal(e.to_string()))?;
            statuses.push((system.name().to_string(), format!("{:?}", status)));
        }
        Ok(statuses)
    }

    /// Get all tools from all systems with proper system prefixing
    pub fn get_prefixed_tools(&self) -> Vec<Tool> {
        let mut tools = Vec::new();
        for system in &self.systems {
            for tool in system.tools() {
                tools.push(Tool::new(
                    format!("{}__{}", system.name(), tool.name),
                    &tool.description,
                    tool.input_schema.clone(),
                ));
            }
        }
        tools
    }

    /// Get system resources and their contents
    pub async fn get_systems_resources(
        &self,
    ) -> AgentResult<HashMap<String, HashMap<String, (Resource, String)>>> {
        let mut system_resource_content = HashMap::new();
        for system in &self.systems {
            let system_status = system
                .status()
                .await
                .map_err(|e| AgentError::Internal(e.to_string()))?;

            let mut resource_content = HashMap::new();
            for resource in system_status {
                if let Ok(content) = system.read_resource(&resource.uri).await {
                    resource_content.insert(resource.uri.to_string(), (resource, content));
                }
            }
            system_resource_content.insert(system.name().to_string(), resource_content);
        }
        Ok(system_resource_content)
    }

    /// Get the system prompt
    pub fn get_system_prompt(&self) -> AgentResult<String> {
        let mut context = HashMap::new();
        let systems_info: Vec<SystemInfo> = self
            .systems
            .iter()
            .map(|system| {
                SystemInfo::new(system.name(), system.description(), system.instructions())
            })
            .collect();

        context.insert("systems", systems_info);
        load_prompt_file("system.md", &context).map_err(|e| AgentError::Internal(e.to_string()))
    }

    /// Find the appropriate system for a tool call based on the prefixed name
    pub fn get_system_for_tool(&self, prefixed_name: &str) -> Option<&dyn System> {
        let parts: Vec<&str> = prefixed_name.split("__").collect();
        if parts.len() != 2 {
            return None;
        }
        let system_name = parts[0];
        self.systems
            .iter()
            .find(|sys| sys.name() == system_name)
            .map(|v| &**v)
    }

    /// Dispatch a single tool call to the appropriate system
    pub async fn dispatch_tool_call(
        &self,
        tool_call: AgentResult<ToolCall>,
    ) -> AgentResult<Vec<Content>> {
        let call = tool_call?;
        let system = self
            .get_system_for_tool(&call.name)
            .ok_or_else(|| AgentError::ToolNotFound(call.name.clone()))?;

        let tool_name = call
            .name
            .split("__")
            .nth(1)
            .ok_or_else(|| AgentError::InvalidToolName(call.name.clone()))?;
        let system_tool_call = ToolCall::new(tool_name, call.arguments);

        system.call(system_tool_call).await
    }
}