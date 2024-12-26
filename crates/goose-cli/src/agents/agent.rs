// use anyhow::Result;
use async_trait::async_trait;
// use futures::stream::BoxStream;
use tokio::sync::Mutex;
use goose::{
    agents::Agent, providers::base::ProviderUsage, systems::System, providers::base::Provider
};

pub struct GooseAgent {
    systems: Vec<Box<dyn System>>,
    provider: Box<dyn Provider>,
    provider_usage: Mutex<Vec<ProviderUsage>>,
}

impl GooseAgent {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            systems: Vec::new(),
            provider,
            provider_usage: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Agent for GooseAgent {
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
    // async fn reply(&self, messages: &[Message]) -> Result<BoxStream<'_, Result<Message>>> {
    //     self.reply(messages).await
    // }

    // async fn usage(&self) -> Result<Vec<ProviderUsage>> {
    //     self.usage().await
    // }
}
