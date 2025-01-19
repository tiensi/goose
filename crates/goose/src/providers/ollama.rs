use super::base::{Provider, ProviderUsage, Usage};
use super::configs::ModelConfig;
use super::utils::{get_model, handle_response};
use crate::message::Message;
use crate::providers::openai_utils::{
    create_openai_request_payload, get_openai_usage, openai_response_to_message,
};
use anyhow::Result;
use async_trait::async_trait;
use mcp_core::tool::Tool;
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

pub const OLLAMA_HOST: &str = "http://localhost:11434";
pub const OLLAMA_MODEL: &str = "qwen2.5";

#[derive(serde::Serialize)]
pub struct OllamaProvider {
    #[serde(skip)]
    client: Client,
    host: String,
    model: ModelConfig,
}

impl OllamaProvider {
    pub fn from_env() -> Result<Self> {
        let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| OLLAMA_HOST.to_string());
        let model_name = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| OLLAMA_MODEL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;

        Ok(Self {
            client,
            host,
            model: ModelConfig::new(model_name),
        })
    }

    async fn post(&self, payload: Value) -> Result<Value> {
        let url = format!("{}/v1/chat/completions", self.host.trim_end_matches('/'));

        let response = self.client.post(&url).json(&payload).send().await?;

        handle_response(payload, response).await
    }
}

#[async_trait]
impl Provider for OllamaProvider {
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
        let payload = create_openai_request_payload(&self.model, system, messages, tools)?;

        let response = self.post(payload.clone()).await?;

        // Parse response
        let message = openai_response_to_message(response.clone())?;
        let usage = self.get_usage(&response)?;
        let model = get_model(&response);
        let cost = None;
        super::utils::emit_debug_trace(self, &payload, &response, &usage, cost);
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
        let model = ModelConfig::new(OLLAMA_MODEL.to_string());
        let messages = vec![Message::user().with_text("Hello?")];
        let system = "You are a helpful assistant.";
        let tools = vec![create_test_tool()];

        let payload = create_openai_request_payload(&model, system, &messages, &tools)?;

        // Verify payload structure
        assert_eq!(payload["model"], OLLAMA_MODEL);
        
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
            "model": OLLAMA_MODEL
        });

        let message = openai_response_to_message(response.clone())?;
        let usage = OllamaProvider::from_env()?.get_usage(&response)?;

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
            "model": OLLAMA_MODEL
        });

        let message = openai_response_to_message(response.clone())?;
        let usage = OllamaProvider::from_env()?.get_usage(&response)?;

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
