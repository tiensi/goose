use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use goose::{
    agent::{Agent as GooseAgent, ApprovalMonitor},
    models::message::Message,
    systems::System,
};

#[async_trait]
pub trait Agent {
    fn add_system(&mut self, system: Box<dyn System>);
    async fn reply(
        &self,
        messages: &[Message],
        approval_monitor: ApprovalMonitor,
    ) -> Result<BoxStream<'_, Result<Message>>>;
}

#[async_trait]
impl Agent for GooseAgent {
    fn add_system(&mut self, system: Box<dyn System>) {
        self.add_system(system);
    }

    async fn reply(
        &self,
        messages: &[Message],
        approval_monitor: ApprovalMonitor,
    ) -> Result<BoxStream<'_, Result<Message>>> {
        self.reply(messages, approval_monitor).await
    }
}
