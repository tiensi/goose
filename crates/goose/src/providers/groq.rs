use crate::message::Message;
use crate::providers::base::{Provider, ProviderUsage, Usage};
use crate::providers::configs::ModelConfig;
use crate::providers::openai_utils::{
    create_openai_request_payload_with_concat_response_content, get_openai_usage,
    openai_response_to_message,
};
use crate::providers::utils::{get_model, handle_response};
use anyhow::Result;
use async_trait::async_trait;
use mcp_core::Tool;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

pub const GROQ_API_HOST: &str = "https://api.groq.com";
pub const GROQ_DEFAULT_MODEL: &str = "llama-3.3-70b-versatile";

#[derive(serde::Serialize)]
pub struct GroqProvider {
    #[serde(skip)]
    client: Client,
    host: String,
    api_key: String,
    model: ModelConfig,
}

impl GroqProvider {
    pub fn from_env() -> Result<Self> {
        let api_key = crate::key_manager::get_keyring_secret("GROQ_API_KEY", Default::default())?;
        let host = std::env::var("GROQ_HOST").unwrap_or_else(|_| GROQ_API_HOST.to_string());
        let model_name =
            std::env::var("GROQ_MODEL").unwrap_or_else(|_| GROQ_DEFAULT_MODEL.to_string());

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

    async fn post(&self, payload: Value) -> anyhow::Result<Value> {
        let url = format!(
            "{}/openai/v1/chat/completions",
            self.host.trim_end_matches('/')
        );

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
impl Provider for GroqProvider {
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
    ) -> anyhow::Result<(Message, ProviderUsage)> {
        let payload = create_openai_request_payload_with_concat_response_content(
            &self.model,
            system,
            messages,
            tools,
        )?;

        let response = self.post(payload.clone()).await?;

        let message = openai_response_to_message(response.clone())?;
        let usage = self.get_usage(&response)?;
        let model = get_model(&response);
        super::utils::emit_debug_trace(self, &payload, &response, &usage, None);
        Ok((message, ProviderUsage::new(model, usage, None)))
    }

    fn get_usage(&self, data: &Value) -> anyhow::Result<Usage> {
        get_openai_usage(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageContent;
    use mcp_core::Tool;
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
        let model = ModelConfig::new(GROQ_DEFAULT_MODEL.to_string());
        let messages = vec![Message::user().with_text("Hello?")];
        let system = "You are a helpful assistant.";
        let tools = vec![create_test_tool()];

        let payload = create_openai_request_payload_with_concat_response_content(
            &model,
            system,
            &messages,
            &tools,
        )?;

        // Verify payload structure
        assert_eq!(payload["model"], GROQ_DEFAULT_MODEL);
        
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
            "model": "llama-3.3-70b-versatile"
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
            "model": "llama-3.3-70b-versatile"
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
}
