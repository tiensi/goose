use std::vec;

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use goose::providers::mock::MockProvider;
use goose::{
    agents::Agent,
    message::Message,
    providers::base::{Provider, ProviderUsage},
    systems::System,
};
use tokio::sync::Mutex;

pub struct MockAgent {
    systems: Vec<Box<dyn System>>,
    provider: Box<dyn Provider>,
    provider_usage: Mutex<Vec<ProviderUsage>>,
}

impl MockAgent {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
            provider: Box::new(MockProvider::new(Vec::new())),
            provider_usage: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Agent for MockAgent {
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

    async fn reply(&self, _messages: &[Message]) -> Result<BoxStream<'_, Result<Message>>> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn usage(&self) -> Result<Vec<ProviderUsage>> {
        Ok(vec![ProviderUsage::new(
            "mock".to_string(),
            Default::default(),
            None,
        )])
    }
}
