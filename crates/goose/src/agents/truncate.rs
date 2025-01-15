use async_trait::async_trait;
use futures::stream::BoxStream;
use std::collections::{HashMap, VecDeque};
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
use mcp_core::{TextContent, Tool};
use serde_json::Value;
use crate::prompt_template::load_prompt_file;

/// Agent impl. that truncates oldest messages when payload over LLM ctx-limit
pub struct TruncateAgent {
    capabilities: Mutex<Capabilities>,
    _token_counter: TokenCounter,
}

impl TruncateAgent {
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            capabilities: Mutex::new(Capabilities::new(provider)),
            _token_counter: TokenCounter::new(),
        }
    }

    async fn prepare_inference(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        messages: &[Message],
        target_limit: usize,
        model_name: &str,
        resource_items: &mut [ResourceItem],
    ) -> SystemResult<TruncatedConversation> {
        // Flatten all resource content into a vector of strings
        let resources: Vec<String> = resource_items
            .iter()
            .map(|item| item.content.clone())
            .collect();

        let model = Some(model_name);
        let approx_count =
            self._token_counter
                .count_everything(system_prompt, messages, tools, &resources, model);

        let mut trunc_conv = TruncatedConversation::new(
            messages.to_vec(),
            None,
        );

        if approx_count > target_limit {
            let user_msg_size = self.text_content_size(messages.last(), model);
            if user_msg_size > target_limit {
                debug!(
                    "[WARNING] User message {} exceeds token budget {}.",
                    user_msg_size,
                    user_msg_size - target_limit
                );
                return Err(SystemError::ContextLimit);
            }

            trunc_conv = self.chop_front_messages(messages, approx_count, target_limit, model);
            if trunc_conv.conversation.is_empty() {
                return Err(SystemError::ContextLimit);
            }
        }

        Ok(trunc_conv)
    }

    fn text_content_size(&self, message: Option<&Message>, model: Option<&str>) -> usize {
        let text = message
            .and_then(|msg| msg.content.first())
            .and_then(|c| c.as_text());

        if let Some(txt) = text {
            let count = self._token_counter.count_tokens(txt, model);
            return count;
        }

        let default_size = 0;

        default_size
    }
    fn clip_message(&self, message: &Message, clip_size: Option<usize>) -> String {
        let clip_size = clip_size.unwrap_or(25);
        let text = message.content
            .first()
            .and_then(|c| c.as_text())
            .unwrap();

        let words = text
            .split_whitespace()
            .take(clip_size)
            .collect::<Vec<&str>>()
            .join(" ");

        words
    }
    fn chop_front_messages(
        &self,
        messages: &[Message],
        approx_count: usize,
        target_limit: usize,
        model: Option<&str>,
    ) -> TruncatedConversation {
        debug!(
            "[WARNING] Conversation history has size: {} exceeding the token budget of {}. \
            Dropping oldest messages.",
            approx_count,
            approx_count - target_limit
        );

        let mut message_clippings: Vec<String> = vec![];
        let mut truncated_conversation: VecDeque<Message> = VecDeque::from(messages.to_vec());
        let mut current_tokens = approx_count;

        // Remove messages until we're under target limit
        for msg in messages.iter() {
            if current_tokens < target_limit || truncated_conversation.is_empty() {
                break;
            }
            let count = self.text_content_size(Some(msg), model);
            current_tokens = current_tokens.saturating_sub(count);
            let chopped_msg = truncated_conversation.pop_front().unwrap();

            // gather message clippings for assistant's truncation notif to user
            let speaker_text = self.clip_message(&chopped_msg, None);
            let snippet = format!("{:?}: {}", chopped_msg.role, speaker_text);
            message_clippings.push(snippet);
        }

        // todo extract first and last
        let mut context = HashMap::new();
        context.insert("snippets", message_clippings);

        // use trimmed message-history
        let conversation = Vec::from(truncated_conversation);
        let trunc_notif = load_prompt_file("trunc_messages_notif.md",
                                           &context);

        match trunc_notif {
            Ok(notif) => TruncatedConversation::new(conversation, Some(notif)),
            _ => TruncatedConversation::new(conversation, None)
        }
    }

    fn append_message(&self, base_msg: Message, appendage: Option<String>) -> Message {
        match appendage {
            Some(appendage) => {
                let mut msg = base_msg.clone();
                let assistant_msg = msg.content
                    .first()
                    .and_then(|c| c.as_text())
                    .unwrap();

                let augmented_msg = format!("{}\n{}", assistant_msg, appendage);
                let annos = msg.content
                    .first()
                    .and_then(|c| match c {
                        MessageContent::Text(text) => Some(text.annotations.clone()),
                        _ => None,
                    })
                    .unwrap();

                msg.content = vec!(MessageContent::Text(TextContent {
                    text: augmented_msg.into(),
                    annotations: annos,
                }));

                msg
            }
            None => base_msg
        }
    }
}

#[async_trait]
impl Agent for TruncateAgent {
    #[instrument(skip(self, messages), fields(user_message))]
    async fn reply(
        &self,
        messages: &[Message],
    ) -> anyhow::Result<BoxStream<'_, anyhow::Result<Message>>> {
        let reply_span = tracing::Span::current();
        let mut capabilities = self.capabilities.lock().await;
        let tools = capabilities.get_prefixed_tools().await?;
        let system_prompt = capabilities.get_system_prompt().await;
        let _estimated_limit = capabilities
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
        let mut trunc_conv = self
            .prepare_inference(
                &system_prompt,
                &tools,
                messages,
                _estimated_limit,
                &capabilities.provider().get_model_config().model_name,
                &mut capabilities.get_resources().await?,
            )
            .await?;

        let mut messages = trunc_conv.conversation.clone();

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
                let response_with_notif = self.append_message(
                    response.clone(),
                    trunc_conv.notification.clone());

                yield response_with_notif.clone();
                // yield response.clone(); todo delete

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

                trunc_conv = self.prepare_inference(
                    &system_prompt,
                    &tools,
                    &messages,
                    _estimated_limit,
                    &capabilities.provider().get_model_config().model_name,
                    &mut capabilities.get_resources().await?
                ).await?;
                messages = trunc_conv.conversation;

                messages.push(response);
                messages.push(message_tool_response);
            }
        }))
    }

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

    async fn usage(&self) -> Vec<ProviderUsage> {
        let capabilities = self.capabilities.lock().await;
        capabilities.get_usage().await
    }
}

struct TruncatedConversation {
    conversation: Vec<Message>,
    notification: Option<String>,
}

impl TruncatedConversation {
    fn new(conversation: Vec<Message>, truncation_message: Option<String>) -> Self {
        TruncatedConversation { conversation, notification: truncation_message }
    }
}

register_agent!("truncate", TruncateAgent);
