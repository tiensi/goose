use crate::message::Message;
use crate::providers::base::{Provider, ProviderUsage, Usage};
use crate::providers::configs::{GroqProviderConfig, ModelConfig, ProviderModelConfig};
use crate::providers::google::GoogleProvider;
use crate::providers::utils::{
    create_openai_request_payload, get_model, get_openai_usage, handle_response,
    openai_response_to_message, unescape_json_values,
};
use anyhow::anyhow;
use async_trait::async_trait;
use mcp_core::Tool;
use reqwest::Client;
use serde_json::{json, Map, Value};
use std::time::Duration;

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

    fn get_usage(data: &Value) -> anyhow::Result<Usage> {
        get_openai_usage(data)
    }

    async fn post(&self, payload: Value) -> anyhow::Result<Value> {
        let url = format!(
            "{}/openai/v1/chat/completions",
            self.config.host.trim_end_matches('/')
        );

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&payload)
            .send()
            .await?;

        handle_response(payload, response).await?
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
        let payload = create_openai_request_payload(&self.config.model, system, messages, tools)?;

        let response = self.post(payload).await?;

        let message = openai_response_to_message(response.clone())?;
        let usage = Self::get_usage(&response)?;
        let model = get_model(&response);

        Ok((message, ProviderUsage::new(model, usage, None)))
    }
}
