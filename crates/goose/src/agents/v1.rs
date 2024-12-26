use async_trait::async_trait;
use tokio::sync::Mutex;
use anyhow::Result;

use super::Agent;
use crate::errors::AgentResult;
use crate::message::Message;
use crate::providers::base::{Provider, ProviderUsage};
use crate::systems::System;
use mcp_core::Tool;

/// A version of the agent that uses a more aggressive context management strategy

pub struct AgentV1 {
    systems: Vec<Box<dyn System>>,
    provider: Box<dyn Provider>,
    provider_usage: Mutex<Vec<ProviderUsage>>,
}

impl AgentV1 {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            systems: Vec::new(),
            provider,
            provider_usage: Mutex::new(Vec::new()),
        }
    }

    pub fn add_system(&mut self, system: Box<dyn System>) {
        self.systems.push(system);
    }
}

#[async_trait]
impl Agent for AgentV1 {
    fn add_system(&mut self, system: Box<dyn System>) {
        self.systems.push(system);
    }

    fn get_systems(&self) -> &Vec<Box<dyn System>> {
        &self.systems
    }

    fn get_provider(&self) -> &Box<dyn Provider> {
        &self.provider
    }

    fn get_provider_usage(&self) -> &Mutex<Vec<ProviderUsage>> {
        &self.provider_usage
    }
    async fn prepare_inference(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        messages: &[Message],
        pending: &[Message],
        target_limit: usize,
    ) -> AgentResult<Vec<Message>> {
        todo!();
        // return Ok(messages.to_vec());
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::providers::mock::MockProvider;
//     use futures::TryStreamExt;

//     #[tokio::test]
//     async fn test_v1_agent() -> Result<(), anyhow::Error> {
//         // Create a mock provider that returns a simple response
//         let response = Message::assistant().with_text("Hello!");
//         let provider = MockProvider::new(vec![response.clone()]);
//         let agent = AgentV1::new(Box::new(provider));

//         // Test basic reply functionality
//         let initial_message = Message::user().with_text("Hi");
//         let initial_messages = vec![initial_message];

//         let mut stream = agent.reply(&initial_messages).await?;
//         let mut messages = Vec::new();
//         while let Some(msg) = stream.try_next().await? {
//             messages.push(msg);
//         }

//         assert_eq!(messages.len(), 1);
//         assert_eq!(messages[0], response);
//         Ok(())
//     }
// }