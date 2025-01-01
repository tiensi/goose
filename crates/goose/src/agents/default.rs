use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;
use std::collections::HashMap;

use super::{Agent, MCPManager};
use crate::errors::{AgentError, AgentResult};
use crate::message::{Message, ToolRequest};
use crate::providers::base::Provider;
use crate::register_agent;
use crate::systems::System;
use crate::token_counter::TokenCounter;
use mcp_core::{Content, Resource, Tool, ToolCall};
use crate::prompt_template::load_prompt_file;
use crate::providers::base::ProviderUsage;
use serde_json::Value;
// used to sort resources by priority within error margin
const PRIORITY_EPSILON: f32 = 0.001;

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

/// Default implementation of an Agent
pub struct DefaultAgent {
    mcp_manager: Mutex<MCPManager>,
}

impl DefaultAgent {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            mcp_manager: Mutex::new(MCPManager::new(provider)),
        }
    }

    /// Setup the next inference by budgeting the context window
    async fn prepare_inference(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        messages: &[Message],
        pending: &[Message],
        target_limit: usize,
        model_name: &str,
        resource_content: &HashMap<String, HashMap<String, (Resource, String)>>,
    ) -> AgentResult<Vec<Message>> {
        let token_counter = TokenCounter::new();

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
            Some(model_name),
        );

        let mut status_content: Vec<String> = Vec::new();

        if approx_count > target_limit {
            println!("[WARNING] Token budget exceeded. Current count: {} \n Difference: {} tokens over buget. Removing context", approx_count, approx_count - target_limit);

            // Get token counts for each resource
            let mut system_token_counts = HashMap::new();

            // Iterate through each system and its resources
            for (system_name, resources) in resource_content {
                let mut resource_counts = HashMap::new();
                for (uri, (_resource, content)) in resources {
                    let token_count = token_counter.count_tokens(&content, Some(model_name)) as u32;
                    resource_counts.insert(uri.clone(), token_count);
                }
                system_token_counts.insert(system_name.clone(), resource_counts);
            }

            // Sort resources by priority and timestamp and trim to fit context limit
            let mut all_resources: Vec<(String, String, Resource, u32)> = Vec::new();
            for (system_name, resources) in resource_content {
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
}

#[async_trait]
impl Agent for DefaultAgent {
    async fn add_system(&mut self, system: Box<dyn System>) -> AgentResult<()> {
        let mut manager = self.mcp_manager.lock().await;
        manager.add_system(system);
        Ok(())
    }

    async fn remove_system(&mut self, name: &str) -> AgentResult<()> {
        let mut manager = self.mcp_manager.lock().await;
        manager.remove_system(name)
    }

    async fn list_systems(&self) -> AgentResult<Vec<(String, String)>> {
        let manager = self.mcp_manager.lock().await;
        manager.list_systems().await
    }

    async fn passthrough(&self, _system: &str, _request: Value) -> AgentResult<Value> {
        Ok(Value::Null)
    }

    async fn reply(&self, messages: &[Message]) -> anyhow::Result<BoxStream<'_, anyhow::Result<Message>>> {
        let manager = self.mcp_manager.lock().await;
        let tools = manager.get_prefixed_tools();
        let system_prompt = manager.get_system_prompt()?;
        let estimated_limit = manager.provider().get_model_config().get_estimated_limit();

        // Update conversation history for the start of the reply
        let mut messages = self.prepare_inference(
            &system_prompt,
            &tools,
            messages,
            &Vec::new(),
            estimated_limit,
            &manager.provider().get_model_config().model_name,
            &manager.get_systems_resources().await?,
        ).await?;

        Ok(Box::pin(async_stream::try_stream! {
            loop {
                // Get completion from provider
                let (response, usage) = manager.provider().complete(
                    &system_prompt,
                    &messages,
                    &tools,
                ).await?;
                manager.record_usage(usage).await;

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
                    .map(|request| manager.dispatch_tool_call(request.tool_call.clone()))
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
                messages = self.prepare_inference(&system_prompt, &tools, &messages, &pending, estimated_limit, &manager.provider().get_model_config().model_name, &manager.get_systems_resources().await?).await?;
            }
        }))
    }

    async fn usage(&self) -> AgentResult<Vec<ProviderUsage>> {
        let manager = self.mcp_manager.lock().await;
        manager.get_usage().await.map_err(|e| AgentError::Internal(e.to_string()))
    }
}

register_agent!("default", DefaultAgent);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, MessageContent};
    use crate::providers::configs::ModelConfig;
    use crate::providers::mock::MockProvider;
    use async_trait::async_trait;
    use chrono::Utc;
    use futures::TryStreamExt;
    use mcp_core::resource::Resource;
    use mcp_core::{Annotations, Content, Tool, ToolCall};
    use serde_json::json;
    use std::collections::HashMap;

    // Mock system for testing
    struct MockSystem {
        name: String,
        tools: Vec<Tool>,
        resources: Vec<Resource>,
        resource_content: HashMap<String, String>,
    }

    impl MockSystem {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                tools: vec![Tool::new(
                    "echo",
                    "Echoes back the input",
                    json!({"type": "object", "properties": {"message": {"type": "string"}}, "required": ["message"]}),
                )],
                resources: Vec::new(),
                resource_content: HashMap::new(),
            }
        }

        fn add_resource(&mut self, name: &str, content: &str, priority: f32) {
            let uri = format!("file://{}", name);
            let resource = Resource {
                name: name.to_string(),
                uri: uri.clone(),
                annotations: Some(Annotations::for_resource(priority, Utc::now())),
                description: Some("A mock resource".to_string()),
                mime_type: "text/plain".to_string(),
            };
            self.resources.push(resource);
            self.resource_content.insert(uri, content.to_string());
        }
    }

    #[async_trait]
    impl System for MockSystem {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "A mock system for testing"
        }

        fn instructions(&self) -> &str {
            "Mock system instructions"
        }

        fn tools(&self) -> &[Tool] {
            &self.tools
        }

        async fn status(&self) -> anyhow::Result<Vec<Resource>> {
            Ok(self.resources.clone())
        }

        async fn call(&self, tool_call: ToolCall) -> AgentResult<Vec<Content>> {
            match tool_call.name.as_str() {
                "echo" => Ok(vec![Content::text(
                    tool_call.arguments["message"].as_str().unwrap_or(""),
                )]),
                _ => Err(AgentError::ToolNotFound(tool_call.name)),
            }
        }

        async fn read_resource(&self, uri: &str) -> AgentResult<String> {
            self.resource_content.get(uri).cloned().ok_or_else(|| {
                AgentError::InvalidParameters(format!("Resource {} could not be found", uri))
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_simple_response() -> anyhow::Result<()> {
        let response = Message::assistant().with_text("Hello!");
        let provider = MockProvider::new(vec![response.clone()]);
        let mut agent = DefaultAgent::new(Box::new(provider));

        // Add a system to test system management
        agent.add_system(Box::new(MockSystem::new("test"))).await?;

        let initial_message = Message::user().with_text("Hi");
        let initial_messages = vec![initial_message];

        let mut stream = agent.reply(&initial_messages).await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], response);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_system_management() -> anyhow::Result<()> {
        let provider = MockProvider::new(vec![]);
        let mut agent = DefaultAgent::new(Box::new(provider));

        // Add a system
        agent.add_system(Box::new(MockSystem::new("test1"))).await?;
        agent.add_system(Box::new(MockSystem::new("test2"))).await?;

        // List systems
        let systems = agent.list_systems().await?;
        assert_eq!(systems.len(), 2);
        assert!(systems.iter().any(|(name, _)| name == "test1"));
        assert!(systems.iter().any(|(name, _)| name == "test2"));

        // Remove a system
        agent.remove_system("test1").await?;
        let systems = agent.list_systems().await?;
        assert_eq!(systems.len(), 1);
        assert_eq!(systems[0].0, "test2");

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call() -> anyhow::Result<()> {
        let mut agent = DefaultAgent::new(Box::new(MockProvider::new(vec![
            Message::assistant().with_tool_request(
                "1",
                Ok(ToolCall::new("test_echo", json!({"message": "test"}))),
            ),
            Message::assistant().with_text("Done!"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test"))).await?;

        let initial_message = Message::user().with_text("Echo test");
        let initial_messages = vec![initial_message];

        let mut stream = agent.reply(&initial_messages).await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        // Should have three messages: tool request, response, and model text
        assert_eq!(messages.len(), 3);
        assert!(messages[0]
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
        assert_eq!(messages[2].content[0], MessageContent::text("Done!"));
        Ok(())
    }

    #[tokio::test]
    async fn test_prepare_inference_trims_resources() -> anyhow::Result<()> {
        let provider = MockProvider::with_config(
            vec![],
            ModelConfig::new("test_model".to_string()).with_context_limit(Some(20)),
        );
        let mut agent = DefaultAgent::new(Box::new(provider));

        // Create a mock system with resources
        let mut system = MockSystem::new("test");
        let hello_1_tokens = "hello ".repeat(1); // 1 tokens
        let goodbye_10_tokens = "goodbye ".repeat(10); // 10 tokens
        system.add_resource("test_resource_removed", &goodbye_10_tokens, 0.1);
        system.add_resource("test_resource_expected", &hello_1_tokens, 0.5);

        agent.add_system(Box::new(system)).await?;

        // Set up test parameters
        let manager = agent.mcp_manager.lock().await;

        let system_prompt = "This is a system prompt";
        let messages = vec![Message::user().with_text("Hi there")];
        let pending = vec![];
        let tools = vec![];
        let target_limit = manager.provider().get_model_config().context_limit();

        assert_eq!(target_limit, 20, "Context limit should be 20");
        // Test prepare_inference
        let result = agent
            .prepare_inference(&system_prompt, &tools, &messages, &pending, target_limit, &manager.provider().get_model_config().model_name, &manager.get_systems_resources().await?)
            .await?;

        // Get the last message which should be the tool response containing status
        let status_message = result.last().unwrap();
        let status_content = status_message
            .content
            .first()
            .and_then(|content| content.as_tool_response_text())
            .unwrap_or_default();


        // Verify that "hello" is within the response, should be just under 20 tokens with "hello"
        assert!(status_content.contains("hello"));
        assert!(!status_content.contains("goodbye"));

        Ok(())
    }
}