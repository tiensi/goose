use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use crate::message::Message;
use mcp_core::tool::Tool;

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
    usage: Mutex<Usage>,
}

impl Default for ProviderUsageCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderUsageCollector {
    pub fn new() -> Self {
        Self {
            usage: Mutex::new(Usage::default()),
        }
    }

    pub fn add_usage(&self, usage: Usage) {
        if let Ok(mut current) = self.usage.lock() {
            if let Some(input_tokens) = usage.input_tokens {
                current.input_tokens = Some(current.input_tokens.unwrap_or(0) + input_tokens);
            }
            if let Some(output_tokens) = usage.output_tokens {
                current.output_tokens = Some(current.output_tokens.unwrap_or(0) + output_tokens);
            }
            if let Some(total_tokens) = usage.total_tokens {
                current.total_tokens = Some(current.total_tokens.unwrap_or(0) + total_tokens);
            }
        }
    }

    pub fn get_usage(&self) -> Usage {
        self.usage
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
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

    #[test]
    fn test_usage_collector() {
        let collector = ProviderUsageCollector::new();

        // Add first usage
        collector.add_usage(Usage::new(Some(10), Some(20), Some(30)));
        let usage1 = collector.get_usage();
        assert_eq!(usage1.input_tokens, Some(10));
        assert_eq!(usage1.output_tokens, Some(20));
        assert_eq!(usage1.total_tokens, Some(30));

        // Add second usage
        collector.add_usage(Usage::new(Some(5), Some(10), Some(15)));
        let usage2 = collector.get_usage();
        assert_eq!(usage2.input_tokens, Some(15));
        assert_eq!(usage2.output_tokens, Some(30));
        assert_eq!(usage2.total_tokens, Some(45));
    }
}
