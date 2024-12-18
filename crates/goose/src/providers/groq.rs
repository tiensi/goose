use std::time::Duration;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Map, Value};
use mcp_core::Tool;
use crate::message::Message;
use crate::providers::base::{Provider, ProviderUsage};
use crate::providers::configs::{GroqProviderConfig, ModelConfig};
use crate::providers::google::GoogleProvider;
use crate::providers::utils::unescape_json_values;

pub const GROQ_API_HOST: &str = "https://api.groq.com";
pub const GROQ_DEFAULT_MODEL: &str = "llama3-70b-8192";

pub struct GroqProvider {
    client: Client,
    config: GroqProviderConfig,
}

impl GroqProvider {
    pub fn new(config: GroqProviderConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(600)) // 10 minutes timeout
            .build()?;

        Ok(Self { client, config })
    }
}

#[async_trait]
impl Provider for GroqProvider {
    fn get_model_config(&self) -> &ModelConfig {
        self.config.model_config()
    }

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
            json!(self.messages_to_google_spec(&messages)),
        );
        if !tools.is_empty() {
            payload.insert(
                "tools".to_string(),
                json!({"functionDeclarations": self.tools_to_google_spec(&tools)}),
            );
        }
        let mut generation_config = Map::new();
        if let Some(temp) = self.config.model.temperature {
            generation_config.insert("temperature".to_string(), json!(temp));
        }
        if let Some(tokens) = self.config.model.max_tokens {
            generation_config.insert("maxOutputTokens".to_string(), json!(tokens));
        }
        if !generation_config.is_empty() {
            payload.insert("generationConfig".to_string(), json!(generation_config));
        }

        // Make request
        let response = self.post(Value::Object(payload)).await?;
        // Parse response
        let message = self.google_response_to_message(unescape_json_values(&response))?;
        let usage = self.get_usage(&response)?;
        let model = match response.get("modelVersion") {
            Some(model_version) => model_version.as_str().unwrap_or_default().to_string(),
            None => self.config.model.model_name.clone(),
        };
        let provider_usage = ProviderUsage::new(model, usage, None);
        Ok((message, provider_usage))
    }
}