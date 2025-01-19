use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;

use super::base::{Provider, ProviderUsage, Usage};
use super::configs::ModelConfig;
use super::model_pricing::cost;
use super::model_pricing::model_pricing_for;
use super::utils::{emit_debug_trace, get_model, non_ok_response_to_provider_error};
use crate::message::{Message, MessageContent};
use mcp_core::content::Content;
use mcp_core::role::Role;
use mcp_core::tool::{Tool, ToolCall};

pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-3-5-sonnet-latest";

#[derive(serde::Serialize)]
pub struct AnthropicProvider {
    #[serde(skip)]
    client: Client,
    host: String,
    api_key: String,
    model: ModelConfig,
}

impl AnthropicProvider {
    pub fn from_env() -> Result<Self> {
        let api_key =
            crate::key_manager::get_keyring_secret("ANTHROPIC_API_KEY", Default::default())?;
        let host = std::env::var("ANTHROPIC_HOST")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());
        let model_name = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or_else(|_| ANTHROPIC_DEFAULT_MODEL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;

        Ok(Self {
            client,
            host,
            api_key,
            model: ModelConfig::new(model_name),
        })
    }

    fn tools_to_anthropic_spec(tools: &[Tool]) -> Vec<Value> {
        let mut unique_tools = HashSet::new();
        let mut tool_specs = Vec::new();

        for tool in tools {
            if unique_tools.insert(tool.name.clone()) {
                tool_specs.push(json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema
                }));
            }
        }

        // Add "cache_control" to the last tool spec, if any. This means that all tool definitions,
        // will be cached as a single prefix.
        if let Some(last_tool) = tool_specs.last_mut() {
            last_tool
                .as_object_mut()
                .unwrap()
                .insert("cache_control".to_string(), json!({ "type": "ephemeral" }));
        }

        tool_specs
    }

    fn system_to_anthropic_spec(system: &str) -> Value {
        json!([{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" }
        }])
    }

    fn messages_to_anthropic_spec(messages: &[Message]) -> Vec<Value> {
        let mut anthropic_messages = Vec::new();

        // Convert messages to Anthropic format
        for message in messages {
            let role = match message.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };

            let mut content = Vec::new();
            for msg_content in &message.content {
                match msg_content {
                    MessageContent::Text(text) => {
                        content.push(json!({
                            "type": "text",
                            "text": text.text
                        }));
                    }
                    MessageContent::ToolRequest(tool_request) => {
                        if let Ok(tool_call) = &tool_request.tool_call {
                            content.push(json!({
                                "type": "tool_use",
                                "id": tool_request.id,
                                "name": tool_call.name,
                                "input": tool_call.arguments
                            }));
                        }
                    }
                    MessageContent::ToolResponse(tool_response) => {
                        if let Ok(result) = &tool_response.tool_result {
                            let text = result
                                .iter()
                                .filter_map(|c| match c {
                                    Content::Text(t) => Some(t.text.clone()),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join("\n");

                            content.push(json!({
                                "type": "tool_result",
                                "tool_use_id": tool_response.id,
                                "content": text
                            }));
                        }
                    }
                    MessageContent::Image(_) => continue, // Anthropic doesn't support image content yet
                }
            }

            // Skip messages with empty content
            if !content.is_empty() {
                anthropic_messages.push(json!({
                    "role": role,
                    "content": content
                }));
            }
        }

        // If no messages, add a default one
        if anthropic_messages.is_empty() {
            anthropic_messages.push(json!({
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": "Ignore"
                }]
            }));
        }

        // Add "cache_control" to the last and second-to-last "user" messages.
        // During each turn, we mark the final message with cache_control so the conversation can be
        // incrementally cached. The second-to-last user message is also marked for caching with the
        // cache_control parameter, so that this checkpoint can read from the previous cache.
        let mut user_count = 0;
        for message in anthropic_messages.iter_mut().rev() {
            if message.get("role") == Some(&json!("user")) {
                if let Some(content) = message.get_mut("content") {
                    if let Some(content_array) = content.as_array_mut() {
                        if let Some(last_content) = content_array.last_mut() {
                            last_content.as_object_mut().unwrap().insert(
                                "cache_control".to_string(),
                                json!({ "type": "ephemeral" }),
                            );
                        }
                    }
                }
                user_count += 1;
                if user_count >= 2 {
                    break;
                }
            }
        }

        anthropic_messages
    }

    fn parse_anthropic_response(response: Value) -> Result<Message> {
        let content_blocks = response
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| anyhow!("Invalid response format: missing content array"))?;

        let mut message = Message::assistant();

        for block in content_blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        message = message.with_text(text.to_string());
                    }
                }
                Some("tool_use") => {
                    let id = block
                        .get("id")
                        .and_then(|i| i.as_str())
                        .ok_or_else(|| anyhow!("Missing tool_use id"))?;
                    let name = block
                        .get("name")
                        .and_then(|n| n.as_str())
                        .ok_or_else(|| anyhow!("Missing tool_use name"))?;
                    let input = block
                        .get("input")
                        .ok_or_else(|| anyhow!("Missing tool_use input"))?;

                    let tool_call = ToolCall::new(name, input.clone());
                    message = message.with_tool_request(id, Ok(tool_call));
                }
                _ => continue,
            }
        }

        Ok(message)
    }

    async fn post(&self, payload: Value) -> Result<Value> {
        let url = format!("{}/v1/messages", self.host.trim_end_matches('/'));

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => Ok(response.json().await?),
            _ => {
                let provider_error = non_ok_response_to_provider_error(payload, response).await;
                Err(anyhow::anyhow!(provider_error.to_string()))
            }
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    fn get_model_config(&self) -> &ModelConfig {
        &self.model
    }

    #[tracing::instrument(
        skip(self, system, messages, tools),
        fields(
            model_config,
            input,
            output,
            input_tokens,
            output_tokens,
            total_tokens,
            cost
        )
    )]
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, ProviderUsage)> {
        let anthropic_messages = Self::messages_to_anthropic_spec(messages);
        let tool_specs = Self::tools_to_anthropic_spec(tools);
        let system_spec = Self::system_to_anthropic_spec(system);

        // Check if we have any messages to send
        if anthropic_messages.is_empty() {
            return Err(anyhow!("No valid messages to send to Anthropic API"));
        }

        let mut payload = json!({
            "model": self.model.model_name,
            "messages": anthropic_messages,
            "max_tokens": self.model.max_tokens.unwrap_or(4096)
        });

        // Add system message if present
        if !system.is_empty() {
            payload
                .as_object_mut()
                .unwrap()
                .insert("system".to_string(), json!(system_spec));
        }

        // Add tools if present
        if !tool_specs.is_empty() {
            payload
                .as_object_mut()
                .unwrap()
                .insert("tools".to_string(), json!(tool_specs));
        }

        // Add temperature if specified
        if let Some(temp) = self.model.temperature {
            payload
                .as_object_mut()
                .unwrap()
                .insert("temperature".to_string(), json!(temp));
        }

        // Make request
        let response = self.post(payload.clone()).await?;

        // Parse response
        let message = Self::parse_anthropic_response(response.clone())?;
        let usage = self.get_usage(&response)?;
        let model = get_model(&response);
        let cost = cost(&usage, &model_pricing_for(&model));
        emit_debug_trace(self, &payload, &response, &usage, cost);
        Ok((message, ProviderUsage::new(model, usage, cost)))
    }

    fn get_usage(&self, data: &Value) -> Result<Usage> {
        // Extract usage data if available
        if let Some(usage) = data.get("usage") {
            let input_tokens = usage
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as i32);
            let output_tokens = usage
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as i32);
            let total_tokens = match (input_tokens, output_tokens) {
                (Some(i), Some(o)) => Some(i + o),
                _ => None,
            };

            Ok(Usage::new(input_tokens, output_tokens, total_tokens))
        } else {
            // If no usage data, return None for all values
            Ok(Usage::new(None, None, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider() -> AnthropicProvider {
        AnthropicProvider {
            client: Client::builder().build().unwrap(),
            host: "https://api.anthropic.com".to_string(),
            api_key: "test_api_key".to_string(),
            model: ModelConfig::new("claude-3-sonnet-20241022".to_string())
                .with_temperature(Some(0.7))
                .with_context_limit(Some(200_000)),
        }
    }

    #[test]
    fn test_parse_text_response() -> Result<()> {
        let response = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "text",
                "text": "Hello! How can I assist you today?"
            }],
            "model": "claude-3-5-sonnet-latest",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 12,
                "output_tokens": 15,
                "cache_creation_input_tokens": 12,
                "cache_read_input_tokens": 0
            }
        });

        let message = AnthropicProvider::parse_anthropic_response(response.clone())?;
        let usage = create_provider().get_usage(&response)?;

        if let MessageContent::Text(text) = &message.content[0] {
            assert_eq!(text.text, "Hello! How can I assist you today?");
        } else {
            panic!("Expected Text content");
        }

        assert_eq!(usage.input_tokens, Some(12));
        assert_eq!(usage.output_tokens, Some(15));
        assert_eq!(usage.total_tokens, Some(27));

        Ok(())
    }

    #[test]
    fn test_parse_tool_response() -> Result<()> {
        let response = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "tool_1",
                "name": "calculator",
                "input": {
                    "expression": "2 + 2"
                }
            }],
            "model": "claude-3-sonnet-20240229",
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 15,
                "output_tokens": 20,
                "cache_creation_input_tokens": 15,
                "cache_read_input_tokens": 0,
            }
        });

        let message = AnthropicProvider::parse_anthropic_response(response.clone())?;
        let usage = create_provider().get_usage(&response)?;

        if let MessageContent::ToolRequest(tool_request) = &message.content[0] {
            let tool_call = tool_request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "calculator");
            assert_eq!(tool_call.arguments, json!({"expression": "2 + 2"}));
        } else {
            panic!("Expected ToolRequest content");
        }

        assert_eq!(usage.input_tokens, Some(15));
        assert_eq!(usage.output_tokens, Some(20));
        assert_eq!(usage.total_tokens, Some(35));

        Ok(())
    }

    #[test]
    fn test_message_to_anthropic_spec() {
        let messages = vec![
            Message::user().with_text("Hello"),
            Message::assistant().with_text("Hi there"),
            Message::user().with_text("How are you?"),
        ];

        let spec = AnthropicProvider::messages_to_anthropic_spec(&messages);

        assert_eq!(spec.len(), 3);
        assert_eq!(spec[0]["role"], "user");
        assert_eq!(spec[0]["content"][0]["type"], "text");
        assert_eq!(spec[0]["content"][0]["text"], "Hello");
        assert_eq!(spec[1]["role"], "assistant");
        assert_eq!(spec[1]["content"][0]["text"], "Hi there");
        assert_eq!(spec[2]["role"], "user");
        assert_eq!(spec[2]["content"][0]["text"], "How are you?");
    }

    #[test]
    fn test_tools_to_anthropic_spec() {
        let tools = vec![
            Tool::new(
                "calculator",
                "Calculate mathematical expressions",
                json!({
                    "type": "object",
                    "properties": {
                        "expression": {
                            "type": "string",
                            "description": "The mathematical expression to evaluate"
                        }
                    }
                }),
            ),
            Tool::new(
                "weather",
                "Get weather information",
                json!({
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The location to get weather for"
                        }
                    }
                }),
            ),
        ];

        let spec = AnthropicProvider::tools_to_anthropic_spec(&tools);

        assert_eq!(spec.len(), 2);
        assert_eq!(spec[0]["name"], "calculator");
        assert_eq!(spec[0]["description"], "Calculate mathematical expressions");
        assert_eq!(spec[1]["name"], "weather");
        assert_eq!(spec[1]["description"], "Get weather information");
        
        // Verify cache control is added to last tool
        assert!(spec[1].get("cache_control").is_some());
    }

    #[test]
    fn test_system_to_anthropic_spec() {
        let system = "You are a helpful assistant.";
        let spec = AnthropicProvider::system_to_anthropic_spec(system);

        assert!(spec.is_array());
        let spec_array = spec.as_array().unwrap();
        assert_eq!(spec_array.len(), 1);
        assert_eq!(spec_array[0]["type"], "text");
        assert_eq!(spec_array[0]["text"], system);
        assert!(spec_array[0].get("cache_control").is_some());
    }
}