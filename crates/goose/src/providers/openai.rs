use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use super::base::{Provider, ProviderUsage, Usage};
use super::configs::ModelConfig;
use super::model_pricing::cost;
use super::model_pricing::model_pricing_for;
use super::utils::{emit_debug_trace, get_model, handle_response};
use crate::message::Message;
use crate::providers::openai_utils::{
    check_openai_context_length_error, create_openai_request_payload, get_openai_usage,
    openai_response_to_message,
};
use mcp_core::tool::Tool;


pub const OPEN_AI_DEFAULT_MODEL: &str = "gpt-4o";

#[derive(Debug, serde::Serialize)]
pub struct OpenAiProvider {
    #[serde(skip)]
    client: Client,
    host: String,
    api_key: String,
    model: ModelConfig,
}

impl OpenAiProvider {
    pub fn from_env() -> Result<Self> {
        let api_key = crate::key_manager::get_keyring_secret("OPENAI_API_KEY", Default::default())?;
        let host =
            std::env::var("OPENAI_HOST").unwrap_or_else(|_| "https://api.openai.com".to_string());
        let model_name =
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| OPEN_AI_DEFAULT_MODEL.to_string());

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

    async fn post(&self, payload: Value) -> Result<Value> {
        let url = format!("{}/v1/chat/completions", self.host.trim_end_matches('/'));

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?;

        handle_response(payload, response).await
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
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
        // Not checking for o1 model here since system message is not supported by o1
        let payload = create_openai_request_payload(&self.model, system, messages, tools)?;

        // Make request
        let response = self.post(payload.clone()).await?;

        // Raise specific error if context length is exceeded
        if let Some(error) = response.get("error") {
            if let Some(err) = check_openai_context_length_error(error) {
                return Err(err.into());
            }
            return Err(anyhow!("OpenAI API error: {}", error));
        }

        // Parse response
        let message = openai_response_to_message(response.clone())?;
        let usage = self.get_usage(&response)?;
        let model = get_model(&response);
        let cost = cost(&usage, &model_pricing_for(&model));
        emit_debug_trace(self, &payload, &response, &usage, cost);
        Ok((message, ProviderUsage::new(model, usage, cost)))
    }

    fn get_usage(&self, data: &Value) -> Result<Usage> {
        get_openai_usage(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageContent;
    use mcp_core::tool::Tool;
    use serde_json::json;

    fn create_test_tool() -> Tool {
        Tool::new(
            "get_weather",
            "Gets the current weather for a location",
            json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city and state, e.g. New York, NY"
                    }
                },
                "required": ["location"]
            }),
        )
    }

    #[test]
    fn test_request_payload_construction() -> Result<()> {
        let model = ModelConfig::new("gpt-4o".to_string()).with_temperature(Some(0.7));
        let messages = vec![Message::user().with_text("Hello?")];
        let system = "You are a helpful assistant.";
        let tools = vec![create_test_tool()];

        let payload = create_openai_request_payload(&model, system, &messages, &tools)?;

        // Verify payload structure
        assert_eq!(payload["model"], "gpt-4o");
        assert!(payload["temperature"].as_f64().unwrap() - 0.7 < 0.0001); // Compare with tolerance for float precision
        
        let messages = payload["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2); // System + user message
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], system);
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello?");

        let tools = payload["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "get_weather");

        Ok(())
    }

    #[test]
    fn test_response_parsing_basic() -> Result<()> {
        let response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I assist you today?",
                    "tool_calls": null
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 15,
                "total_tokens": 27
            },
            "model": "gpt-4o"
        });

        let message = openai_response_to_message(response.clone())?;
        let usage = get_openai_usage(&response)?;

        // Verify message parsing
        if let MessageContent::Text(text) = &message.content[0] {
            assert_eq!(text.text, "Hello! How can I assist you today?");
        } else {
            panic!("Expected Text content");
        }

        // Verify usage parsing
        assert_eq!(usage.input_tokens, Some(12));
        assert_eq!(usage.output_tokens, Some(15));
        assert_eq!(usage.total_tokens, Some(27));

        Ok(())
    }

    #[test]
    fn test_response_parsing_tool_calls() -> Result<()> {
        let response = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"San Francisco, CA\"}"
                        }
                    }]
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 15,
                "total_tokens": 27
            },
            "model": "gpt-4o"
        });

        let message = openai_response_to_message(response.clone())?;
        let usage = get_openai_usage(&response)?;

        // Verify tool call parsing
        if let MessageContent::ToolRequest(tool_request) = &message.content[0] {
            let tool_call = tool_request.tool_call.as_ref().unwrap();
            assert_eq!(tool_call.name, "get_weather");
            assert_eq!(
                tool_call.arguments,
                json!({"location": "San Francisco, CA"})
            );
        } else {
            panic!("Expected ToolRequest content");
        }

        // Verify usage parsing
        assert_eq!(usage.input_tokens, Some(12));
        assert_eq!(usage.output_tokens, Some(15));
        assert_eq!(usage.total_tokens, Some(27));

        Ok(())
    }

    #[test]
    fn test_error_response_parsing() -> Result<()> {
        let error_response = json!({
            "error": {
                "message": "This model's maximum context length is 4097 tokens.",
                "type": "invalid_request_error",
                "code": "context_length_exceeded"
            }
        });

        let err = check_openai_context_length_error(&error_response["error"]);
        assert!(err.is_some());
        
        Ok(())
    }
}