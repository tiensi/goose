use async_trait::async_trait;
use tokio::sync::Mutex;

use super::Agent;
use crate::providers::base::{Provider, ProviderUsage};
use crate::systems::System;

/// Base implementation of an Agent
pub struct BaseAgent {
    systems: Vec<Box<dyn System>>,
    provider: Box<dyn Provider>,
    provider_usage: Mutex<Vec<ProviderUsage>>,
}

impl BaseAgent {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            systems: Vec::new(),
            provider,
            provider_usage: Mutex::new(Vec::new()),
        }
    }

    pub fn add_system(&mut self, system: Box<dyn System>) {
        self.systems.push(system);
    }
}

#[async_trait]
impl Agent for BaseAgent {
    fn add_system(&mut self, system: Box<dyn System>) {
        self.systems.push(system);
    }

    fn get_systems(&self) -> &Vec<Box<dyn System>> {
        &self.systems
    }

    fn get_provider(&self) -> &Box<dyn Provider> {
        &self.provider
    }

    fn get_provider_usage(&self) -> &Mutex<Vec<ProviderUsage>> {
        &self.provider_usage
    }
}

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
    use rust_decimal_macros::dec;
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

        async fn call(&self, tool_call: ToolCall) -> crate::errors::AgentResult<Vec<Content>> {
            match tool_call.name.as_str() {
                "echo" => Ok(vec![Content::text(
                    tool_call.arguments["message"].as_str().unwrap_or(""),
                )]),
                _ => Err(crate::errors::AgentError::ToolNotFound(tool_call.name)),
            }
        }

        async fn read_resource(&self, uri: &str) -> crate::errors::AgentResult<String> {
            self.resource_content.get(uri).cloned().ok_or_else(|| {
                crate::errors::AgentError::InvalidParameters(format!(
                    "Resource {} could not be found",
                    uri
                ))
            })
        }
    }

    #[tokio::test]
    async fn test_simple_response() -> anyhow::Result<()> {
        let response = Message::assistant().with_text("Hello!");
        let provider = MockProvider::new(vec![response.clone()]);
        let agent = BaseAgent::new(Box::new(provider));

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

    #[tokio::test]
    async fn test_usage_rollup() -> anyhow::Result<()> {
        let response = Message::assistant().with_text("Hello!");
        let provider = MockProvider::new(vec![response.clone()]);
        let agent = BaseAgent::new(Box::new(provider));

        let initial_message = Message::user().with_text("Hi");
        let initial_messages = vec![initial_message];

        let mut stream = agent.reply(&initial_messages).await?;
        while stream.try_next().await?.is_some() {}

        // Second message
        let mut stream = agent.reply(&initial_messages).await?;
        while stream.try_next().await?.is_some() {}

        let usage = agent.usage().await?;
        assert_eq!(usage.len(), 1); // 2 messages rolled up to one usage per model
        assert_eq!(usage[0].usage.input_tokens, Some(2));
        assert_eq!(usage[0].usage.output_tokens, Some(2));
        assert_eq!(usage[0].usage.total_tokens, Some(4));
        assert_eq!(usage[0].model, "mock");
        assert_eq!(usage[0].cost, Some(dec!(2)));
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call() -> anyhow::Result<()> {
        let mut agent = BaseAgent::new(Box::new(MockProvider::new(vec![
            Message::assistant().with_tool_request(
                "1",
                Ok(ToolCall::new("test_echo", json!({"message": "test"}))),
            ),
            Message::assistant().with_text("Done!"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test")));

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
    async fn test_invalid_tool() -> anyhow::Result<()> {
        let mut agent = BaseAgent::new(Box::new(MockProvider::new(vec![
            Message::assistant()
                .with_tool_request("1", Ok(ToolCall::new("invalid_tool", json!({})))),
            Message::assistant().with_text("Error occurred"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test")));

        let initial_message = Message::user().with_text("Invalid tool");
        let initial_messages = vec![initial_message];

        let mut stream = agent.reply(&initial_messages).await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        // Should have three messages: failed tool request, fail response, and model text
        assert_eq!(messages.len(), 3);
        assert!(messages[0]
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
        assert_eq!(
            messages[2].content[0],
            MessageContent::text("Error occurred")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_tool_calls() -> anyhow::Result<()> {
        let mut agent = BaseAgent::new(Box::new(MockProvider::new(vec![
            Message::assistant()
                .with_tool_request(
                    "1",
                    Ok(ToolCall::new("test_echo", json!({"message": "first"}))),
                )
                .with_tool_request(
                    "2",
                    Ok(ToolCall::new("test_echo", json!({"message": "second"}))),
                ),
            Message::assistant().with_text("All done!"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test")));

        let initial_message = Message::user().with_text("Multiple calls");
        let initial_messages = vec![initial_message];

        let mut stream = agent.reply(&initial_messages).await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        // Should have three messages: tool requests, responses, and model text
        assert_eq!(messages.len(), 3);
        assert!(messages[0]
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
        assert_eq!(messages[2].content[0], MessageContent::text("All done!"));
        Ok(())
    }

    #[tokio::test]
    async fn test_prepare_inference_trims_resources_when_budget_exceeded() -> anyhow::Result<()> {
        // Create a mock provider
        let provider = MockProvider::new(vec![]);
        let mut agent = BaseAgent::new(Box::new(provider));

        // Create a mock system with two resources
        let mut system = MockSystem::new("test");

        // Add two resources with different priorities
        let string_10toks = "hello ".repeat(10);
        system.add_resource("high_priority", &string_10toks, 0.8);
        system.add_resource("low_priority", &string_10toks, 0.1);

        agent.add_system(Box::new(system));

        // Set up test parameters
        // 18 tokens with system + user msg in chat format
        let system_prompt = "This is a system prompt";
        let messages = vec![Message::user().with_text("Hi there")];
        let tools = vec![];
        let pending = vec![];

        // Approx count is 40, so target limit of 35 will force trimming
        let target_limit = 35;

        // Call prepare_inference
        let result = agent
            .prepare_inference(system_prompt, &tools, &messages, &pending, target_limit)
            .await?;

        // Get the last message which should be the tool response containing status
        let status_message = result.last().unwrap();
        let status_content = status_message
            .content
            .first()
            .and_then(|content| content.as_tool_response_text())
            .unwrap_or_default();

        // Verify that only the high priority resource is included in the status
        assert!(status_content.contains("high_priority"));
        assert!(!status_content.contains("low_priority"));

        // Now test with a target limit that allows both resources (no trimming)
        let target_limit = 100;

        // Call prepare_inference
        let result = agent
            .prepare_inference(system_prompt, &tools, &messages, &pending, target_limit)
            .await?;

        // Get the last message which should be the tool response containing status
        let status_message = result.last().unwrap();
        let status_content = status_message
            .content
            .first()
            .and_then(|content| content.as_tool_response_text())
            .unwrap_or_default();

        // Verify that only the high priority resource is included in the status
        assert!(status_content.contains("high_priority"));
        assert!(status_content.contains("low_priority"));
        Ok(())
    }

    #[tokio::test]
    async fn test_context_trimming_with_custom_model_config() -> anyhow::Result<()> {
        let provider = MockProvider::with_config(
            vec![],
            ModelConfig::new("test_model".to_string()).with_context_limit(Some(20)),
        );
        let mut agent = BaseAgent::new(Box::new(provider));

        // Create a mock system with a resource that will exceed the context limit
        let mut system = MockSystem::new("test");

        // Add a resource that will exceed our tiny context limit
        let hello_1_tokens = "hello ".repeat(1); // 1 tokens
        let goodbye_10_tokens = "goodbye ".repeat(10); // 10 tokens
        system.add_resource("test_resource_removed", &goodbye_10_tokens, 0.1);
        system.add_resource("test_resource_expected", &hello_1_tokens, 0.5);

        agent.add_system(Box::new(system));

        // Set up test parameters
        // 18 tokens with system + user msg in chat format
        let system_prompt = "This is a system prompt";
        let messages = vec![Message::user().with_text("Hi there")];
        let tools = vec![];
        let pending = vec![];

        // Use the context limit from the model config
        let target_limit = agent.get_provider().get_model_config().context_limit();
        assert_eq!(target_limit, 20, "Context limit should be 20");

        // Call prepare_inference
        let result = agent
            .prepare_inference(system_prompt, &tools, &messages, &pending, target_limit)
            .await?;

        // Get the last message which should be the tool response containing status
        let status_message = result.last().unwrap();
        let status_content = status_message
            .content
            .first()
            .and_then(|content| content.as_tool_response_text())
            .unwrap_or_default();

        // verify that "hello" is within the response, should be just under 20 tokens with "hello"
        assert!(status_content.contains("hello"));

        Ok(())
    }
}
