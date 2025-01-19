use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use super::base::{Provider, ProviderUsage, Usage};
use super::configs::ModelConfig;
use super::model_pricing::{cost, model_pricing_for};
use super::oauth;
use super::utils::{check_bedrock_context_length_error, get_model, handle_response, ImageFormat};
use crate::message::Message;
use crate::providers::openai_utils::{
    check_openai_context_length_error, get_openai_usage, messages_to_openai_spec,
    openai_response_to_message, tools_to_openai_spec,
};
use mcp_core::tool::Tool;

const DEFAULT_CLIENT_ID: &str = "databricks-cli";
const DEFAULT_REDIRECT_URL: &str = "http://localhost:8020";
const DEFAULT_SCOPES: &[&str] = &["all-apis"];
pub const DATABRICKS_DEFAULT_MODEL: &str = "claude-3-5-sonnet-2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabricksAuth {
    Token(String),
    OAuth {
        host: String,
        client_id: String,
        redirect_url: String,
        scopes: Vec<String>,
    },
}

impl DatabricksAuth {
    /// Create a new OAuth configuration with default values
    pub fn oauth(host: String) -> Self {
        Self::OAuth {
            host,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            redirect_url: DEFAULT_REDIRECT_URL.to_string(),
            scopes: DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect(),
        }
    }
    pub fn token(token: String) -> Self {
        Self::Token(token)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct DatabricksProvider {
    #[serde(skip)]
    client: Client,
    host: String,
    auth: DatabricksAuth,
    model: ModelConfig,
    image_format: ImageFormat,
}

impl DatabricksProvider {
    pub fn from_env() -> Result<Self> {
        let host = std::env::var("DATABRICKS_HOST")
            .unwrap_or_else(|_| "https://api.databricks.com".to_string());
        let model_name = std::env::var("DATABRICKS_MODEL")
            .unwrap_or_else(|_| DATABRICKS_DEFAULT_MODEL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(600))
            .build()?;

        // If we find a databricks token we prefer that
        if let Ok(api_key) =
            crate::key_manager::get_keyring_secret("DATABRICKS_TOKEN", Default::default())
        {
            return Ok(Self {
                client,
                host: host.clone(),
                auth: DatabricksAuth::token(api_key),
                model: ModelConfig::new(model_name),
                image_format: ImageFormat::Anthropic,
            });
        }

        // Otherwise use Oauth flow
        Ok(Self {
            client,
            host: host.clone(),
            auth: DatabricksAuth::oauth(host),
            model: ModelConfig::new(model_name),
            image_format: ImageFormat::Anthropic,
        })
    }

    async fn ensure_auth_header(&self) -> Result<String> {
        match &self.auth {
            DatabricksAuth::Token(token) => Ok(format!("Bearer {}", token)),
            DatabricksAuth::OAuth {
                host,
                client_id,
                redirect_url,
                scopes,
            } => {
                let token =
                    oauth::get_oauth_token_async(host, client_id, redirect_url, scopes).await?;
                Ok(format!("Bearer {}", token))
            }
        }
    }

    async fn post(&self, payload: Value) -> Result<Value> {
        let url = format!(
            "{}/serving-endpoints/{}/invocations",
            self.host.trim_end_matches('/'),
            self.model.model_name
        );

        let auth_header = self.ensure_auth_header().await?;
        let response = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .json(&payload)
            .send()
            .await?;

        handle_response(payload, response).await
    }
}

#[async_trait]
impl Provider for DatabricksProvider {
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
        // Prepare messages and tools
        let concat_tool_response_contents = false;
        let messages_spec =
            messages_to_openai_spec(messages, &self.image_format, concat_tool_response_contents);
        let tools_spec = if !tools.is_empty() {
            tools_to_openai_spec(tools)?
        } else {
            vec![]
        };

        // Build payload with system message
        let mut messages_array = vec![json!({ "role": "system", "content": system })];
        messages_array.extend(messages_spec);

        let mut payload = json!({ "messages": messages_array });

        // Add optional parameters
        if !tools_spec.is_empty() {
            payload["tools"] = json!(tools_spec);
        }
        if let Some(temp) = self.model.temperature {
            payload["temperature"] = json!(temp);
        }
        if let Some(tokens) = self.model.max_tokens {
            payload["max_tokens"] = json!(tokens);
        }

        // Remove null values
        let payload = serde_json::Value::Object(
            payload
                .as_object()
                .unwrap()
                .iter()
                .filter(|&(_, v)| !v.is_null())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );

        // Make request
        let response = self.post(payload.clone()).await?;

        // Raise specific error if context length is exceeded
        if let Some(error) = response.get("error") {
            if let Some(err) = check_openai_context_length_error(error) {
                return Err(err.into());
            } else if let Some(err) = check_bedrock_context_length_error(error) {
                return Err(err.into());
            }
            return Err(anyhow!("Databricks API error: {}", error));
        }

        // Parse response
        let message = openai_response_to_message(response.clone())?;
        let usage = self.get_usage(&response)?;
        let model = get_model(&response);
        let cost = cost(&usage, &model_pricing_for(&model));
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
        let model = ModelConfig::new(DATABRICKS_DEFAULT_MODEL.to_string());
        let messages = vec![Message::user().with_text("Hello?")];
        let system = "You are a helpful assistant.";
        let tools = vec![create_test_tool()];

        let mut messages_array = vec![json!({ "role": "system", "content": system })];
        messages_array.extend(messages_to_openai_spec(
            &messages,
            &ImageFormat::Anthropic,
            false,
        ));

        let mut payload = json!({
            "messages": messages_array,
            "temperature": model.temperature.unwrap_or(0.7)
        });
        payload["tools"] = json!(tools_to_openai_spec(&tools)?);

        // Verify payload structure
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
            "model": DATABRICKS_DEFAULT_MODEL
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
            "model": DATABRICKS_DEFAULT_MODEL
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

    #[test]
    fn test_auth_token_construction() -> Result<()> {
        let provider = DatabricksProvider {
            client: Client::builder().build().unwrap(),
            host: "https://example.com".to_string(),
            auth: DatabricksAuth::Token("test-token".to_string()),
            model: ModelConfig::new(DATABRICKS_DEFAULT_MODEL.to_string()),
            image_format: ImageFormat::Anthropic,
        };

        let auth_header = tokio::runtime::Runtime::new()?.block_on(provider.ensure_auth_header())?;
        assert_eq!(auth_header, "Bearer test-token");

        Ok(())
    }
}