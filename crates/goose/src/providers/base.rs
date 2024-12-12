use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::models::message::Message;
use crate::models::tool::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub total_tokens: Option<i32>,
}

impl Usage {
    pub fn new(
        input_tokens: Option<i32>,
        output_tokens: Option<i32>,
        total_tokens: Option<i32>,
    ) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens,
        }
    }
}

use async_trait::async_trait;

/// Base trait for AI providers (OpenAI, Anthropic, etc)
#[async_trait]
pub trait Provider: Send + Sync {
    /// Generate the next message using the configured model and other parameters
    ///
    /// # Arguments
    /// * `system` - The system prompt that guides the model's behavior
    /// * `messages` - The conversation history as a sequence of messages
    /// * `tools` - Optional list of tools the model can use
    ///
    /// # Returns
    /// A tuple containing the model's response message and usage statistics
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<(Message, Usage)>;

    /// Providers should implement this method to return their total usage statistics from provider.complete (or others)
    fn total_usage(&self) -> Usage;
}

/// A simple struct to reuse for collecting usage statistics for provider implementations.
pub struct ProviderUsageCollector {
    pub usage: Usage,
}

impl ProviderUsageCollector {
    pub fn new() -> Self {
        Self {
            usage: Usage::default(),
        }
    }

    pub fn add_usage(&mut self, usage: Usage) {
        if let Some(input_tokens) = usage.input_tokens {
            self.usage.input_tokens = Some(self.usage.input_tokens.unwrap_or(0) + input_tokens);
        }
        if let Some(output_tokens) = usage.output_tokens {
            self.usage.output_tokens = Some(self.usage.output_tokens.unwrap_or(0) + output_tokens);
        }
        if let Some(total_tokens) = usage.total_tokens {
            self.usage.total_tokens = Some(self.usage.total_tokens.unwrap_or(0) + total_tokens);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_usage_creation() {
        let usage = Usage::new(Some(10), Some(20), Some(30));
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(20));
        assert_eq!(usage.total_tokens, Some(30));
    }

    #[test]
    fn test_usage_serialization() -> Result<()> {
        let usage = Usage::new(Some(10), Some(20), Some(30));
        let serialized = serde_json::to_string(&usage)?;
        let deserialized: Usage = serde_json::from_str(&serialized)?;

        assert_eq!(usage.input_tokens, deserialized.input_tokens);
        assert_eq!(usage.output_tokens, deserialized.output_tokens);
        assert_eq!(usage.total_tokens, deserialized.total_tokens);

        // Test JSON structure
        let json_value: serde_json::Value = serde_json::from_str(&serialized)?;
        assert_eq!(json_value["input_tokens"], json!(10));
        assert_eq!(json_value["output_tokens"], json!(20));
        assert_eq!(json_value["total_tokens"], json!(30));

        Ok(())
    }
}
