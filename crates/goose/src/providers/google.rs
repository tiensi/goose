use crate::message::{Message, MessageContent};
use crate::providers::base::{Provider, ProviderUsage, Usage};
use crate::providers::configs::ModelConfig;
use crate::providers::utils::{
    handle_response, is_valid_function_name, sanitize_function_name, unescape_json_values,
};
use anyhow::Result;
use async_trait::async_trait;
use mcp_core::ToolError;
use mcp_core::{Content, Role, Tool, ToolCall};
use reqwest::Client;
use serde_json::{json, Map, Value};
use std::time::Duration;

pub const GOOGLE_API_HOST: &str = "https://generativelanguage.googleapis.com";
pub const GOOGLE_DEFAULT_MODEL: &str = "gemini-1.5-flash";

#[derive(Debug, serde::Serialize)]
pub struct GoogleProvider {
    #[serde(skip)]
    client: Client,
    host: String,
    api_key: String,
    model: ModelConfig,
}

impl GoogleProvider {
    pub fn from_env() -> Result<Self> {
        let api_key = crate::key_manager::get_keyring_secret("GOOGLE_API_KEY", Default::default())?;
        let host = std::env::var("GOOGLE_HOST").unwrap_or_else(|_| GOOGLE_API_HOST.to_string());
        let model_name =
            std::env::var("GOOGLE_MODEL").unwrap_or_else(|_| GOOGLE_DEFAULT_MODEL.to_string());

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
            "{}/v1beta/models/{}:generateContent?key={}",
            self.host.trim_end_matches('/'),
            self.model.model_name,
            self.api_key
        );

        let response = self
            .client
            .post(&url)
            .header("CONTENT_TYPE", "application/json")
            .json(&payload)
            .send()
            .await?;

        handle_response(payload, response).await
    }

    fn messages_to_google_spec(&self, messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|message| {
                let role = if message.role == Role::User {
                    "user"
                } else {
                    "model"
                };
                let mut parts = Vec::new();
                for message_content in message.content.iter() {
                    match message_content {
                        MessageContent::Text(text) => {
                            if !text.text.is_empty() {
                                parts.push(json!({"text": text.text}));
                            }
                        }
                        MessageContent::ToolRequest(request) => match &request.tool_call {
                            Ok(tool_call) => {
                                let mut function_call_part = Map::new();
                                function_call_part.insert(
                                    "name".to_string(),
                                    json!(sanitize_function_name(&tool_call.name)),
                                );
                                if tool_call.arguments.is_object()
                                    && !tool_call.arguments.as_object().unwrap().is_empty()
                                {
                                    function_call_part
                                        .insert("args".to_string(), tool_call.arguments.clone());
                                }
                                parts.push(json!({
                                    "functionCall": function_call_part
                                }));
                            }
                            Err(e) => {
                                parts.push(json!({"text":format!("Error: {}", e)}));
                            }
                        },
                        MessageContent::ToolResponse(response) => {
                            match &response.tool_result {
                                Ok(contents) => {
                                    // Send only contents with no audience or with Assistant in the audience
                                    let abridged: Vec<_> = contents
                                        .iter()
                                        .filter(|content| {
                                            content.audience().is_none_or(|audience| {
                                                audience.contains(&Role::Assistant)
                                            })
                                        })
                                        .map(|content| content.unannotated())
                                        .collect();

                                    for content in abridged {
                                        match content {
                                            Content::Image(image) => {
                                                parts.push(json!({
                                                    "inline_data": {
                                                        "mime_type": image.mime_type,
                                                        "data": image.data,
                                                    }
                                                }));
                                            }
                                            _ => {
                                                parts.push(json!({
                                                    "functionResponse": {
                                                        "name": response.id,
                                                        "response": {"content": content},
                                                    }}
                                                ));
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    parts.push(json!({"text":format!("Error: {}", e)}));
                                }
                            }
                        }

                        _ => {}
                    }
                }
                json!({"role": role, "parts": parts})
            })
            .collect()
    }

    fn tools_to_google_spec(&self, tools: &[Tool]) -> Vec<Value> {
        tools
            .iter()
            .map(|tool| {
                let mut parameters = Map::new();
                parameters.insert("name".to_string(), json!(tool.name));
                parameters.insert("description".to_string(), json!(tool.description));
                let tool_input_schema = tool.input_schema.as_object().unwrap();
                let tool_input_schema_properties = tool_input_schema
                    .get("properties")
                    .unwrap_or(&json!({}))
                    .as_object()
                    .unwrap()
                    .clone();
                if !tool_input_schema_properties.is_empty() {
                    let accepted_tool_schema_attributes = vec![
                        "type".to_string(),
                        "format".to_string(),
                        "description".to_string(),
                        "nullable".to_string(),
                        "enum".to_string(),
                        "maxItems".to_string(),
                        "minItems".to_string(),
                        "properties".to_string(),
                        "required".to_string(),
                        "items".to_string(),
                    ];
                    parameters.insert(
                        "parameters".to_string(),
                        json!(process_map(
                            tool_input_schema,
                            &accepted_tool_schema_attributes,
                            None
                        )),
                    );
                }
                json!(parameters)
            })
            .collect()
    }

    fn google_response_to_message(&self, response: Value) -> anyhow::Result<Message> {
        let mut content = Vec::new();
        let binding = vec![];
        let candidates: &Vec<Value> = response
            .get("candidates")
            .and_then(|v| v.as_array())
            .unwrap_or(&binding);
        let candidate = candidates.first();
        let role = Role::Assistant;
        let created = chrono::Utc::now().timestamp();
        if candidate.is_none() {
            return Ok(Message {
                role,
                created,
                content,
            });
        }
        let candidate = candidate.unwrap();
        let parts = candidate
            .get("content")
            .and_then(|content| content.get("parts"))
            .and_then(|parts| parts.as_array())
            .unwrap_or(&binding);
        for part in parts {
            if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                content.push(MessageContent::text(text.to_string()));
            } else if let Some(function_call) = part.get("functionCall") {
                let id = function_call["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let name = function_call["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                if !is_valid_function_name(&name) {
                    let error = ToolError::NotFound(format!(
                        "The provided function name '{}' had invalid characters, it must match this regex [a-zA-Z0-9_-]+",
                        name
                    ));
                    content.push(MessageContent::tool_request(id, Err(error)));
                } else {
                    let parameters = function_call.get("args");
                    if let Some(params) = parameters {
                        content.push(MessageContent::tool_request(
                            id,
                            Ok(ToolCall::new(&name, params.clone())),
                        ));
                    }
                }
            }
        }
        Ok(Message {
            role,
            created,
            content,
        })
    }
}

fn process_map(
    map: &Map<String, Value>,
    accepted_keys: &[String],
    parent_key: Option<&str>, // Track the parent key
) -> Value {
    let mut filtered_map: Map<String, serde_json::Value> = map
        .iter()
        .filter_map(|(key, value)| {
            let should_remove = !accepted_keys.contains(key) && parent_key != Some("properties");
            if should_remove {
                return None;
            }
            // Process nested maps recursively
            let filtered_value = match value {
                Value::Object(nested_map) => process_map(
                    &nested_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    accepted_keys,
                    Some(key),
                ),
                _ => value.clone(),
            };

            Some((key.clone(), filtered_value))
        })
        .collect();
    if parent_key != Some("properties") && !filtered_map.contains_key("type") {
        filtered_map.insert("type".to_string(), Value::String("string".to_string()));
    }

    Value::Object(filtered_map)
}

#[async_trait]
impl Provider for GoogleProvider {
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
        let mut payload = Map::new();
        payload.insert(
            "system_instruction".to_string(),
            json!({"parts": [{"text": system}]}),
        );
        payload.insert(
            "contents".to_string(),
            json!(self.messages_to_google_spec(messages)),
        );
        if !tools.is_empty() {
            payload.insert(
                "tools".to_string(),
                json!({"functionDeclarations": self.tools_to_google_spec(tools)}),
            );
        }
        let mut generation_config = Map::new();
        if let Some(temp) = self.model.temperature {
            generation_config.insert("temperature".to_string(), json!(temp));
        }
        if let Some(tokens) = self.model.max_tokens {
            generation_config.insert("maxOutputTokens".to_string(), json!(tokens));
        }
        if !generation_config.is_empty() {
            payload.insert("generationConfig".to_string(), json!(generation_config));
        }

        // Make request
        let response = self.post(Value::Object(payload.clone())).await?;
        // Parse response
        let message = self.google_response_to_message(unescape_json_values(&response))?;
        let usage = self.get_usage(&response)?;
        let model = match response.get("modelVersion") {
            Some(model_version) => model_version.as_str().unwrap_or_default().to_string(),
            None => self.model.model_name.clone(),
        };
        super::utils::emit_debug_trace(self, &payload, &response, &usage, None);
        let provider_usage = ProviderUsage::new(model, usage, None);
        Ok((message, provider_usage))
    }

    fn get_usage(&self, data: &Value) -> anyhow::Result<Usage> {
        if let Some(usage_meta_data) = data.get("usageMetadata") {
            let input_tokens = usage_meta_data
                .get("promptTokenCount")
                .and_then(|v| v.as_u64())
                .map(|v| v as i32);
            let output_tokens = usage_meta_data
                .get("candidatesTokenCount")
                .and_then(|v| v.as_u64())
                .map(|v| v as i32);
            let total_tokens = usage_meta_data
                .get("totalTokenCount")
                .and_then(|v| v.as_u64())
                .map(|v| v as i32);
            Ok(Usage::new(input_tokens, output_tokens, total_tokens))
        } else {
            // If no usage data, return None for all values
            Ok(Usage::new(None, None, None))
        }
    }
}

#[cfg(test)] // Only compiles this module when running tests
mod tests {
    use super::*;

    fn set_up_provider() -> GoogleProvider {
        GoogleProvider {
            client: Client::builder().build().unwrap(),
            host: "dummy_host".to_string(),
            api_key: "dummy_key".to_string(),
            model: ModelConfig::new("dummy_model".to_string()),
        }
    }

    fn set_up_text_message(text: &str, role: Role) -> Message {
        Message {
            role,
            created: 0,
            content: vec![MessageContent::text(text.to_string())],
        }
    }

    fn set_up_tool_request_message(id: &str, tool_call: ToolCall) -> Message {
        Message {
            role: Role::User,
            created: 0,
            content: vec![MessageContent::tool_request(id.to_string(), Ok(tool_call))],
        }
    }

    fn set_up_tool_response_message(id: &str, tool_response: Vec<Content>) -> Message {
        Message {
            role: Role::Assistant,
            created: 0,
            content: vec![MessageContent::tool_response(
                id.to_string(),
                Ok(tool_response),
            )],
        }
    }

    fn set_up_tool(name: &str, description: &str, params: Value) -> Tool {
        Tool {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: json!({
                "properties": params
            }),
        }
    }

    #[test]
    fn test_get_usage() {
        let provider = set_up_provider();
        let data = json!({
            "usageMetadata": {
                "promptTokenCount": 1,
                "candidatesTokenCount": 2,
                "totalTokenCount": 3
            }
        });
        let usage = provider.get_usage(&data).unwrap();
        assert_eq!(usage.input_tokens, Some(1));
        assert_eq!(usage.output_tokens, Some(2));
        assert_eq!(usage.total_tokens, Some(3));
    }

    #[test]
    fn test_message_to_google_spec_text_message() {
        let provider = set_up_provider();
        let messages = vec![
            set_up_text_message("Hello", Role::User),
            set_up_text_message("World", Role::Assistant),
        ];
        let payload = provider.messages_to_google_spec(&messages);
        assert_eq!(payload.len(), 2);
        assert_eq!(payload[0]["role"], "user");
        assert_eq!(payload[0]["parts"][0]["text"], "Hello");
        assert_eq!(payload[1]["role"], "model");
        assert_eq!(payload[1]["parts"][0]["text"], "World");
    }

    #[test]
    fn test_message_to_google_spec_tool_request_message() {
        let provider = set_up_provider();
        let arguments = json!({
            "param1": "value1"
        });
        let messages = vec![set_up_tool_request_message(
            "id",
            ToolCall::new("tool_name", json!(arguments)),
        )];
        let payload = provider.messages_to_google_spec(&messages);
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0]["role"], "user");
        assert_eq!(payload[0]["parts"][0]["functionCall"]["args"], arguments);
    }

    #[test]
    fn test_message_to_google_spec_tool_result_message() {
        let provider = set_up_provider();
        let tool_result: Vec<Content> = vec![Content::text("Hello")];
        let messages = vec![set_up_tool_response_message("response_id", tool_result)];
        let payload = provider.messages_to_google_spec(&messages);
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0]["role"], "model");
        assert_eq!(
            payload[0]["parts"][0]["functionResponse"]["name"],
            "response_id"
        );
        assert_eq!(
            payload[0]["parts"][0]["functionResponse"]["response"]["content"]["text"],
            "Hello"
        );
    }

    #[test]
    fn tools_to_google_spec_with_valid_tools() {
        let provider = set_up_provider();
        let params1 = json!({
            "param1": {
                "type": "string",
                "description": "A parameter",
                "field_does_not_accept": ["value1", "value2"]
            }
        });
        let params2 = json!({
            "param2": {
                "type": "string",
                "description": "B parameter",
            }
        });
        let tools = vec![
            set_up_tool("tool1", "description1", params1),
            set_up_tool("tool2", "description2", params2),
        ];
        let result = provider.tools_to_google_spec(&tools);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["name"], "tool1");
        assert_eq!(result[0]["description"], "description1");
        assert_eq!(
            result[0]["parameters"]["properties"],
            json!({"param1": json!({
                "type": "string",
                "description": "A parameter"
            })})
        );
        assert_eq!(result[1]["name"], "tool2");
        assert_eq!(result[1]["description"], "description2");
        assert_eq!(
            result[1]["parameters"]["properties"],
            json!({"param2": json!({
                "type": "string",
                "description": "B parameter"
            })})
        );
    }

    #[test]
    fn tools_to_google_spec_with_empty_properties() {
        let provider = set_up_provider();
        let tools = vec![Tool {
            name: "tool1".to_string(),
            description: "description1".to_string(),
            input_schema: json!({
                "properties": {}
            }),
        }];
        let result = provider.tools_to_google_spec(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "tool1");
        assert_eq!(result[0]["description"], "description1");
        assert!(result[0]["parameters"].get("properties").is_none());
    }

    #[test]
    fn google_response_to_message_with_no_candidates() {
        let provider = set_up_provider();
        let response = json!({});
        let message = provider.google_response_to_message(response).unwrap();
        assert_eq!(message.role, Role::Assistant);
        assert!(message.content.is_empty());
    }

    #[test]
    fn google_response_to_message_with_text_part() {
        let provider = set_up_provider();
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "text": "Hello, world!"
                    }]
                }
            }]
        });
        let message = provider.google_response_to_message(response).unwrap();
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(message.content.len(), 1);
        if let MessageContent::Text(text) = &message.content[0] {
            assert_eq!(text.text, "Hello, world!");
        } else {
            panic!("Expected text content");
        }
    }

    #[test]
    fn google_response_to_message_with_invalid_function_name() {
        let provider = set_up_provider();
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "invalid name!",
                            "args": {}
                        }
                    }]
                }
            }]
        });
        let message = provider.google_response_to_message(response).unwrap();
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(message.content.len(), 1);
        if let Err(error) = &message.content[0].as_tool_request().unwrap().tool_call {
            assert!(matches!(error, ToolError::NotFound(_)));
        } else {
            panic!("Expected tool request error");
        }
    }

    #[test]
    fn google_response_to_message_with_valid_function_call() {
        let provider = set_up_provider();
        let response = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "valid_name",
                            "args": {
                                "param": "value"
                            }
                        }
                    }]
                }
            }]
        });
        let message = provider.google_response_to_message(response).unwrap();
        assert_eq!(message.role, Role::Assistant);
        assert_eq!(message.content.len(), 1);
        if let Ok(tool_call) = &message.content[0].as_tool_request().unwrap().tool_call {
            assert_eq!(tool_call.name, "valid_name");
            assert_eq!(tool_call.arguments["param"], "value");
        } else {
            panic!("Expected valid tool request");
        }
    }
}