use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use std::sync::Mutex;

use crate::message::Message;
use crate::providers::base::{Provider, Usage};
use mcp_core::tool::Tool;

/// A mock provider that returns pre-configured responses for testing
pub struct MockProvider {
    responses: Arc<Mutex<Vec<Message>>>,
}

impl MockProvider {
    /// Create a new mock provider with a sequence of responses
    pub fn new(responses: Vec<Message>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _system_prompt: &str,
        _messages: &[Message],
        _tools: &[Tool],
    ) -> Result<(Message, Usage)> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            // Return empty response if no more pre-configured responses
            Ok((Message::assistant().with_text(""), Usage::default()))
        } else {
            Ok((responses.remove(0), Usage::default()))
        }
    }
}
