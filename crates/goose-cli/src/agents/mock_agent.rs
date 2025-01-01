use async_trait::async_trait;
use futures::stream::BoxStream;
use goose::providers::mock::MockProvider;
use goose::{
    agents::Agent,
    errors::AgentResult,
    message::Message,
    providers::base::{Provider, ProviderUsage},
    systems::System,
};
use serde_json::Value;
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
    async fn add_system(&mut self, system: Box<dyn System>) -> AgentResult<()> {
        self.systems.push(system);
        Ok(())
    }

    async fn remove_system(&mut self, name: &str) -> AgentResult<()> {
        self.systems.retain(|s| s.name() != name);
        Ok(())
    }

    async fn list_systems(&self) -> AgentResult<Vec<(String, String)>> {
        Ok(self.systems.iter()
            .map(|s| (s.name().to_string(), s.description().to_string()))
            .collect())
    }

    async fn passthrough(&self, _system: &str, _request: Value) -> AgentResult<Value> {
        Ok(Value::Null)
    }

    async fn reply(&self, _messages: &[Message]) -> anyhow::Result<BoxStream<'_, anyhow::Result<Message>>> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn usage(&self) -> AgentResult<Vec<ProviderUsage>> {
        Ok(vec![ProviderUsage::new(
            "mock".to_string(),
            Default::default(),
            None,
        )])
    }
}
