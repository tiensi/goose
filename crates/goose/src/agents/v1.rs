use async_trait::async_trait;
use tokio::sync::Mutex;

use super::Agent;
use crate::errors::AgentResult;
use crate::message::Message;
use crate::providers::base::{Provider, ProviderUsage};
use crate::register_agent;
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
        _system_prompt: &str,
        _tools: &[Tool],
        _messages: &[Message],
        _pending: &[Message],
        _target_limit: usize,
    ) -> AgentResult<Vec<Message>> {
        todo!();
    }
}

register_agent!("v1", AgentV1);
