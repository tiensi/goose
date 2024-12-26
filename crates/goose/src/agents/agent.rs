use anyhow::Result;
use async_stream;
use async_trait::async_trait;
use futures::stream::BoxStream;
use rust_decimal_macros::dec;
use serde_json::json;
use std::collections::HashMap;
use tokio::sync::Mutex;

use crate::errors::{AgentError, AgentResult};
use crate::message::{Message, ToolRequest};
use crate::prompt_template::load_prompt_file;
use crate::providers::base::{Provider, ProviderUsage};
use crate::systems::System;
use crate::token_counter::TokenCounter;
use mcp_core::{Content, Resource, Tool, ToolCall};
use serde::Serialize;

// used to sort resources by priority within error margin
const PRIORITY_EPSILON: f32 = 0.001;

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

#[derive(Clone, Debug, Serialize)]
struct SystemStatus {
    name: String,
    status: String,
}

impl SystemStatus {
    fn new(name: &str, status: String) -> Self {
        Self {
            name: name.to_string(),
            status,
        }
    }
}

/// Core trait defining the behavior of an Agent
#[async_trait]
pub trait Agent: Send + Sync {
    /// Get all tools from all systems with proper system prefixing
    fn get_prefixed_tools(&self) -> Vec<Tool> {
        let mut tools = Vec::new();
        for system in self.get_systems() {
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

    // add a system to the agent
    fn add_system(&mut self, system: Box<dyn System>);

    /// Get the systems this agent has access to
    fn get_systems(&self) -> &Vec<Box<dyn System>>;
    
    /// Get the provider for this agent
    fn get_provider(&self) -> &Box<dyn Provider>;
    
    /// Get the provider usage statistics
    fn get_provider_usage(&self) -> &Mutex<Vec<ProviderUsage>>;

    /// Setup the next inference by budgeting the context window
    async fn prepare_inference(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        messages: &[Message],
        pending: &[Message],
        target_limit: usize,
    ) -> AgentResult<Vec<Message>> {
        // Default implementation for prepare_inference
        let token_counter = TokenCounter::new();
        let resource_content = self.get_systems_resources().await?;

        // Flatten all resource content into a vector of strings
        let mut resources = Vec::new();
        for system_resources in resource_content.values() {
            for (_, content) in system_resources.values() {
                resources.push(content.clone());
            }
        }

        let approx_count = token_counter.count_everything(
            system_prompt,
            messages,
            tools,
            &resources,
            Some(&self.get_provider().get_model_config().model_name),
        );

        let mut status_content: Vec<String> = Vec::new();

        if approx_count > target_limit {
            println!("[WARNING] Token budget exceeded. Current count: {} \n Difference: {} tokens over buget. Removing context", approx_count, approx_count - target_limit);

            // Get token counts for each resource
            let mut system_token_counts = HashMap::new();

            // Iterate through each system and its resources
            for (system_name, resources) in &resource_content {
                let mut resource_counts = HashMap::new();
                for (uri, (_resource, content)) in resources {
                    let token_count = token_counter
                        .count_tokens(&content, Some(&self.get_provider().get_model_config().model_name))
                        as u32;
                    resource_counts.insert(uri.clone(), token_count);
                }
                system_token_counts.insert(system_name.clone(), resource_counts);
            }
            // Sort resources by priority and timestamp and trim to fit context limit
            let mut all_resources: Vec<(String, String, Resource, u32)> = Vec::new();
            for (system_name, resources) in &resource_content {
                for (uri, (resource, _)) in resources {
                    if let Some(token_count) = system_token_counts
                        .get(system_name)
                        .and_then(|counts| counts.get(uri))
                    {
                        all_resources.push((
                            system_name.clone(),
                            uri.clone(),
                            resource.clone(),
                            *token_count,
                        ));
                    }
                }
            }

            // Sort by priority (high to low) and timestamp (newest to oldest)
            all_resources.sort_by(|a, b| {
                let a_priority = a.2.priority().unwrap_or(0.0);
                let b_priority = b.2.priority().unwrap_or(0.0);
                if (b_priority - a_priority).abs() < PRIORITY_EPSILON {
                    b.2.timestamp().cmp(&a.2.timestamp())
                } else {
                    b.2.priority()
                        .partial_cmp(&a.2.priority())
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            });

            // Remove resources until we're under target limit
            let mut current_tokens = approx_count;

            while current_tokens > target_limit && !all_resources.is_empty() {
                if let Some((system_name, uri, _, token_count)) = all_resources.pop() {
                    if let Some(system_counts) = system_token_counts.get_mut(&system_name) {
                        system_counts.remove(&uri);
                        current_tokens -= token_count as usize;
                    }
                }
            }
            // Create status messages only from resources that remain after token trimming
            for (system_name, uri, _, _) in &all_resources {
                if let Some(system_resources) = resource_content.get(system_name) {
                    if let Some((resource, content)) = system_resources.get(uri) {
                        status_content.push(format!("{}\n```\n{}\n```\n", resource.name, content));
                    }
                }
            }
        } else {
            // Create status messages from all resources when no trimming needed
            for resources in resource_content.values() {
                for (resource, content) in resources.values() {
                    status_content.push(format!("{}\n```\n{}\n```\n", resource.name, content));
                }
            }
        }

        // Join remaining status content and create status message
        let status_str = status_content.join("\n");
        let mut context = HashMap::new();
        let systems_status = vec![SystemStatus::new("system", status_str)];
        context.insert("systems", &systems_status);

        // Load and format the status template with only remaining resources
        let status = load_prompt_file("status.md", &context)
            .map_err(|e| AgentError::Internal(e.to_string()))?;

        // Create a new messages vector with our changes
        let mut new_messages = messages.to_vec();

        // Add pending messages
        for msg in pending {
            new_messages.push(msg.clone());
        }

        // Finally add the status messages
        let message_use =
            Message::assistant().with_tool_request("000", Ok(ToolCall::new("status", json!({}))));

        let message_result =
            Message::user().with_tool_response("000", Ok(vec![Content::text(status)]));

        new_messages.push(message_use);
        new_messages.push(message_result);

        Ok(new_messages)
    }

    /// Create a stream that yields each message as it's generated
    async fn reply(&self, messages: &[Message]) -> Result<BoxStream<'_, Result<Message>>> {
        let mut messages = messages.to_vec();
        let tools = self.get_prefixed_tools();
        let system_prompt = self.get_system_prompt()?;
        let estimated_limit = self.get_provider().get_model_config().get_estimated_limit();

        // Update conversation history for the start of the reply
        messages = self
            .prepare_inference(
                &system_prompt,
                &tools,
                &messages,
                &Vec::new(),
                estimated_limit,
            )
            .await?;

        Ok(Box::pin(async_stream::try_stream! {
            loop {
                // Get completion from provider
                let (response, usage) = self.get_provider().complete(
                    &system_prompt,
                    &messages,
                    &tools,
                ).await?;
                self.get_provider_usage().lock().await.push(usage);

                // Yield the assistant's response
                yield response.clone();

                tokio::task::yield_now().await;

                // First collect any tool requests
                let tool_requests: Vec<&ToolRequest> = response.content
                    .iter()
                    .filter_map(|content| content.as_tool_request())
                    .collect();

                if tool_requests.is_empty() {
                    break;
                }

                // Then dispatch each in parallel
                let futures: Vec<_> = tool_requests
                    .iter()
                    .map(|request| self.dispatch_tool_call(request.tool_call.clone()))
                    .collect();

                // Process all the futures in parallel but wait until all are finished
                let outputs = futures::future::join_all(futures).await;

                // Create a message with the responses
                let mut message_tool_response = Message::user();
                // Now combine these into MessageContent::ToolResponse using the original ID
                for (request, output) in tool_requests.iter().zip(outputs.into_iter()) {
                    message_tool_response = message_tool_response.with_tool_response(
                        request.id.clone(),
                        output,
                    );
                }

                yield message_tool_response.clone();

                // Now we have to remove the previous status tooluse and toolresponse
                // before we add pending messages, then the status msgs back again
                messages.pop();
                messages.pop();

                let pending = vec![response, message_tool_response];
                messages = self.prepare_inference(&system_prompt, &tools, &messages, &pending, estimated_limit).await?;
            }
        }))
    }

    /// Get usage statistics
    async fn usage(&self) -> Result<Vec<ProviderUsage>> {
        let provider_usage = self.get_provider_usage().lock().await.clone();
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

    /// Get system resources and their contents
    async fn get_systems_resources(
        &self,
    ) -> AgentResult<HashMap<String, HashMap<String, (Resource, String)>>> {
        let mut system_resource_content = HashMap::new();
        for system in self.get_systems() {
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
    fn get_system_prompt(&self) -> AgentResult<String> {
        let mut context = HashMap::new();
        let systems_info: Vec<SystemInfo> = self
            .get_systems()
            .iter()
            .map(|system| {
                SystemInfo::new(system.name(), system.description(), system.instructions())
            })
            .collect();

        context.insert("systems", systems_info);
        load_prompt_file("system.md", &context)
            .map_err(|e| AgentError::Internal(e.to_string()))
    }

    /// Find the appropriate system for a tool call based on the prefixed name
    fn get_system_for_tool(&self, prefixed_name: &str) -> Option<&dyn System> {
        let parts: Vec<&str> = prefixed_name.split("__").collect();
        if parts.len() != 2 {
            return None;
        }
        let system_name = parts[0];
        self.get_systems()
            .iter()
            .find(|sys| sys.name() == system_name)
            .map(|v| &**v)
    }

    /// Dispatch a single tool call to the appropriate system
    async fn dispatch_tool_call(
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