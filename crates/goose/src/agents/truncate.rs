use async_trait::async_trait;
use futures::stream::BoxStream;
use serde_json::json;
use std::collections::VecDeque;
use tokio::sync::Mutex;
use tracing::{debug, instrument};

use super::Agent;
use crate::agents::capabilities::{Capabilities, ResourceItem};
use crate::agents::system::{SystemConfig, SystemError, SystemResult};
use crate::message::{Message, MessageContent, ToolRequest};
use crate::providers::base::Provider;
use crate::providers::base::ProviderUsage;
use crate::register_agent;
use crate::token_counter::TokenCounter;
use mcp_core::{Content, Tool, ToolCall};
use serde_json::Value;
/// Agent impl. that truncates oldest messages when payload over LLM ctx-limit
pub struct TruncateAgent {
    capabilities: Mutex<Capabilities>,
    token_counter: TokenCounter,
}

impl TruncateAgent {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            capabilities: Mutex::new(Capabilities::new(provider)),
            token_counter: TokenCounter::new(),
        }
    }

    /// Setup the next inference by budgeting the context window
    async fn prepare_inference(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        messages: &[Message],
        target_limit: usize,
        model_name: &str,
        resource_items: &mut [ResourceItem],
    ) -> SystemResult<Vec<Message>> {
        // Flatten all resource content into a vector of strings
        let resources: Vec<String> = resource_items
            .iter()
            .map(|item| item.content.clone())
            .collect();

        let approx_count = self.token_counter.count_everything(
            system_prompt,
            messages,
            tools,
            &resources,
            Some(model_name),
        );
        let mut status_content: Vec<String> = Vec::new();

        // Create status messages from all resources when no trimming needed
        for item in resource_items {
            status_content.push(format!("{}\n```\n{}\n```\n", item.name, item.content));
        }

        // Join status content and create status message
        let status_str = status_content.join("\n");

        let mut new_messages = messages.to_vec();
        if approx_count > target_limit {
            println!("[WARNING] Token budget exceeded. Current count: {} \n Difference: {} tokens over budget. Removing context", approx_count, approx_count - target_limit);

            let mut trimmed_items: VecDeque<Message> = VecDeque::from(messages.to_vec());
            let mut current_tokens = approx_count;

            // Remove messages until we're under target limit
            for msg in messages.iter() {
                if let Some(content) = msg.content.first() {
                    if let Some(text) = content.as_text() {
                        let count = self.token_counter.count_tokens(text, Some(model_name)) as u32;
                        if current_tokens > target_limit && !trimmed_items.is_empty() {
                            let _ = trimmed_items.pop_front().unwrap();
                            // Subtract removed message’s token_count
                            current_tokens = current_tokens.saturating_sub(count as usize);
                        }
                    }
                }
            }

            if trimmed_items.is_empty() {
                return Err(SystemError::ContextLimit());
            }

            // use trimmed message-history
            new_messages = Vec::from(trimmed_items);
        }

        // Finally add the status messages
        let message_use =
            Message::assistant().with_tool_request("000", Ok(ToolCall::new("status", json!({}))));

        let message_result =
            Message::user().with_tool_response("000", Ok(vec![Content::text(status_str)]));

        new_messages.push(message_use);
        new_messages.push(message_result);

        Ok(new_messages)
    }
}

#[async_trait]
impl Agent for TruncateAgent {
    async fn add_system(&mut self, system: SystemConfig) -> SystemResult<()> {
        let mut capabilities = self.capabilities.lock().await;
        capabilities.add_system(system).await
    }

    async fn remove_system(&mut self, name: &str) {
        let mut capabilities = self.capabilities.lock().await;
        capabilities
            .remove_system(name)
            .await
            .expect("Failed to remove system");
    }

    async fn list_systems(&self) -> Vec<String> {
        let capabilities = self.capabilities.lock().await;
        capabilities
            .list_systems()
            .await
            .expect("Failed to list systems")
    }

    async fn passthrough(&self, _system: &str, _request: Value) -> SystemResult<Value> {
        // TODO implement
        Ok(Value::Null)
    }

    #[instrument(skip(self, messages), fields(user_message))]
    async fn reply(
        &self,
        messages: &[Message],
    ) -> anyhow::Result<BoxStream<'_, anyhow::Result<Message>>> {
        let reply_span = tracing::Span::current();
        let mut capabilities = self.capabilities.lock().await;
        let tools = capabilities.get_prefixed_tools().await?;
        let system_prompt = capabilities.get_system_prompt().await;
        let estimated_limit = capabilities
            .provider()
            .get_model_config()
            .get_estimated_limit();

        // Set the user_message field in the span instead of creating a new event
        if let Some(content) = messages
            .last()
            .and_then(|msg| msg.content.first())
            .and_then(|c| c.as_text())
        {
            debug!("user_message" = &content);
        }

        // Update conversation history for the start of the reply
        let mut messages = self
            .prepare_inference(
                &system_prompt,
                &tools,
                messages,
                estimated_limit,
                &capabilities.provider().get_model_config().model_name,
                &mut capabilities.get_resources().await?,
            )
            .await?;

        Ok(Box::pin(async_stream::try_stream! {
            let _reply_guard = reply_span.enter();
            loop {
                // Get completion from provider
                let (response, usage) = capabilities.provider().complete(
                    &system_prompt,
                    &messages,
                    &tools,
                ).await?;
                capabilities.record_usage(usage).await;

                // Yield the assistant's response
                yield response.clone();

                tokio::task::yield_now().await;

                // First collect any tool requests
                let tool_requests: Vec<&ToolRequest> = response.content
                    .iter()
                    .filter_map(|content| content.as_tool_request())
                    .collect();

                if tool_requests.is_empty() {
                    break;
                }

                // Then dispatch each in parallel
                let futures: Vec<_> = tool_requests
                    .iter()
                    .filter_map(|request| request.tool_call.clone().ok())
                    .map(|tool_call| capabilities.dispatch_tool_call(tool_call))
                    .collect();

                // Process all the futures in parallel but wait until all are finished
                let outputs = futures::future::join_all(futures).await;

                // Create a message with the responses
                let mut message_tool_response = Message::user();
                // Now combine these into MessageContent::ToolResponse using the original ID
                for (request, output) in tool_requests.iter().zip(outputs.into_iter()) {
                    message_tool_response = message_tool_response.with_tool_response(
                        request.id.clone(),
                        output,
                    );
                }

                yield message_tool_response.clone();

                // Now we have to remove the previous status tooluse and toolresponse
                // before we add pending messages, then the status msgs back again
                if let Some(message) = messages.last() {
                    if let MessageContent::ToolResponse(result) = &message.content[0] {
                        if result.id == "000" {
                            messages.pop();
                            messages.pop();
                        }
                    }
                }


                messages = self.prepare_inference(&system_prompt, &tools, &messages, estimated_limit, &capabilities.provider().get_model_config().model_name, &mut capabilities.get_resources().await?).await?;
            }
        }))
    }

    async fn usage(&self) -> Vec<ProviderUsage> {
        let capabilities = self.capabilities.lock().await;
        capabilities.get_usage().await
    }
}

register_agent!("truncate", TruncateAgent);
