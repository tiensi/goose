use anyhow::Result;
use async_stream;
use futures::stream::BoxStream;
use serde_json::json;
use std::collections::HashMap;
use tokio::sync::mpsc::Receiver;

use crate::errors::{AgentError, AgentResult};
use crate::models::content::Content;
use crate::models::message::{ApprovalRequest, Message, MessageContent, ToolRequest};
use crate::models::tool::{Tool, ToolCall};
use crate::prompt_template::load_prompt_file;
use crate::providers::base::Provider;
use crate::systems::System;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
struct SystemInfo {
    name: String,
    description: String,
    instructions: String,
}

impl SystemInfo {
    fn new(name: &str, description: &str, instructions: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            instructions: instructions.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct SystemStatus {
    name: String,
    status: String,
}

impl SystemStatus {
    fn new(name: &str, status: String) -> Self {
        Self {
            name: name.to_string(),
            status,
        }
    }
}

/// Agent integrates a foundational LLM with the systems it needs to pilot
pub struct Agent {
    systems: Vec<Box<dyn System>>,
    provider: Box<dyn Provider>,
}

pub struct ApprovalMonitor {
    pub approval_rx: Receiver<String>,
    pub rejection_rx: Receiver<String>,
}

impl Agent {
    /// Create a new Agent with the specified provider
    pub fn new(provider: Box<dyn Provider>) -> Self {
        Self {
            systems: Vec::new(),
            provider,
        }
    }

    /// Add a system to the agent
    pub fn add_system(&mut self, system: Box<dyn System>) {
        self.systems.push(system);
    }

    /// Get all tools from all systems with proper system prefixing
    fn get_prefixed_tools(&self) -> Vec<Tool> {
        let mut tools = Vec::new();
        for system in &self.systems {
            for tool in system.tools() {
                tools.push(Tool::new(
                    format!("{}__{}", system.name(), tool.name),
                    &tool.description,
                    tool.input_schema.clone(),
                ));
            }
        }
        tools
    }

    /// Find the appropriate system for a tool call based on the prefixed name
    fn get_system_for_tool(&self, prefixed_name: &str) -> Option<&dyn System> {
        let parts: Vec<&str> = prefixed_name.split("__").collect();
        if parts.len() != 2 {
            return None;
        }
        let system_name = parts[0];
        self.systems
            .iter()
            .find(|sys| sys.name() == system_name)
            .map(|v| &**v)
    }

    /// Dispatch a single tool call to the appropriate system
    async fn dispatch_tool_call(
        &self,
        tool_call: AgentResult<ToolCall>,
        permission_denied: bool,
    ) -> AgentResult<Vec<Content>> {
        let call = tool_call?;
        let system = self
            .get_system_for_tool(&call.name)
            .ok_or_else(|| AgentError::ToolNotFound(call.name.clone()))?;

        let tool_name = call
            .name
            .split("__")
            .nth(1)
            .ok_or_else(|| AgentError::InvalidToolName(call.name.clone()))?;
        let system_tool_call = ToolCall::new(tool_name, call.arguments);

        if permission_denied {
            // This should really end up with the audience of assistant as users don't need to see this.
            permission_denied_content().await
        } else {
            system.call(system_tool_call).await
        }
    }

    fn get_system_prompt(&self) -> AgentResult<String> {
        let mut context = HashMap::new();
        let systems_info: Vec<SystemInfo> = self
            .systems
            .iter()
            .map(|system| {
                SystemInfo::new(system.name(), system.description(), system.instructions())
            })
            .collect();

        context.insert("systems", systems_info);
        load_prompt_file("system.md", &context).map_err(|e| AgentError::Internal(e.to_string()))
    }

    /// Fetches the current status of all systems and formats it as a status message
    async fn get_system_status(&self) -> AgentResult<String> {
        // Get status of all systems
        let status = if !self.systems.is_empty() {
            let mut context = HashMap::new();
            let mut systems_status: Vec<SystemStatus> = Vec::new();
            for system in &self.systems {
                let system_status = system
                    .status()
                    .await
                    .map_err(|e| AgentError::Internal(e.to_string()))?;

                // Format the status into a readable string
                let status_str = serde_json::to_string(&system_status).unwrap_or_default();

                systems_status.push(SystemStatus::new(system.name(), status_str));
            }
            context.insert("systems", systems_status);
            load_prompt_file("status.md", &context)
                .map_err(|e| AgentError::Internal(e.to_string()))?
        } else {
            "No systems loaded".to_string()
        };

        Ok(status)
    }

    // Initialize a new reply round, which may call multiple tools
    async fn rewrite_messages_on_reply(
        &self,
        messages: &mut Vec<Message>,
        status: String,
    ) -> AgentResult<()> {
        // Create tool use message for status check
        let message_use =
            Message::assistant().with_tool_request("000", Ok(ToolCall::new("status", json!({}))));

        // Create tool result message with status
        let message_result =
            Message::user().with_tool_response("000", Ok(vec![Content::text(status)]));

        messages.push(message_use);
        messages.push(message_result);
        Ok(())
    }

    // Rewrite the exchange as needed after each tool call
    async fn rewrite_messages_on_tool_response(
        &self,
        messages: &mut Vec<Message>,
        pending: Vec<Message>,
        status: String,
    ) -> AgentResult<()> {
        // Remove the last two messages (status and tool response)
        messages.pop();
        messages.pop();

        // Append the pending messages
        messages.extend(pending);

        // Add back a fresh status and tool response
        let message_use =
            Message::assistant().with_tool_request("000", Ok(ToolCall::new("status", json!({}))));

        let message_result =
            Message::user().with_tool_response("000", Ok(vec![Content::text(status)]));

        messages.push(message_use);
        messages.push(message_result);

        Ok(())
    }

    /// Create a stream that yields each message as it's generated by the agent.
    /// This includes both the assistant's responses and any tool responses.
    pub async fn reply(
        &self,
        messages: &[Message],
        mut approval_monitor: ApprovalMonitor,
    ) -> Result<BoxStream<'_, Result<Message>>> {
        let mut messages = messages.to_vec();
        let tools = self.get_prefixed_tools();
        let system_prompt = self.get_system_prompt()?;

        // Update conversation history for the start of the reply
        let status = self.get_system_status().await?;
        self.rewrite_messages_on_reply(&mut messages, status)
            .await?;

        Ok(Box::pin(async_stream::try_stream! {
            loop {
                // Get completion from provider
                let (mut response, _) = self.provider.complete(
                    &system_prompt,
                    &messages,
                    &tools,
                ).await?;

                // Add approval requests to any tool requests
                let mut approval_map: HashMap<String, String> = HashMap::new();
                let mut new_content: Vec<MessageContent> = vec![];
                response.content.iter().for_each(|content| {
                    let c = if let MessageContent::ToolRequest(tool_request) = content {
                        let approval_request = ApprovalRequest::internal();
                        approval_map.insert(approval_request.id.clone(), tool_request.id.clone());
                        MessageContent::tool_request_with_approval(tool_request.id.clone(), tool_request.tool_call.clone(), Some(approval_request))
                    } else {
                        content.clone()
                    };
                    new_content.push(c);
                });
                response.content = new_content;

                // The assistant's response is added in rewrite_messages_on_tool_response
                // Yield the assistant's response
                yield response.clone();

                // Not sure why this is needed, but this ensures that the above message is yielded
                // before the following potentially long-running commands start processing
                tokio::task::yield_now().await;

                // First collect any tool requests
                let tool_requests: Vec<&ToolRequest> = response.content
                    .iter()
                    .filter_map(|content| content.as_tool_request())
                    .collect();

                if tool_requests.is_empty() {
                    // No more tool calls, end the reply loop
                    break;
                }

                // Wait for all approvals to be handled by the user
                let mut rejected_tool_request_ids: Vec<String> = Vec::new();
                while !approval_map.is_empty() {
                    tokio::select! {
                        approval = approval_monitor.approval_rx.recv() => {
                            if let Some(approval) = approval {
                                approval_map.remove(&approval);
                            }
                        }
                        rejection = approval_monitor.rejection_rx.recv() => {
                            if let Some(rejection) = rejection {
                                if let Some(tool_request_id) = approval_map.remove(&rejection) {
                                    rejected_tool_request_ids.push(tool_request_id.clone());
                                }
                            }
                        }
                    }
                }

                // If we want user role on the requests, have to deal with the duplication of the tool requests from the initial provider.complete yielding.
                //
                // let message_tool_response = Message::user();
                // yield tool_requests.iter().fold(message_tool_response, |msg, request| {
                //     // TOOD: request.id is irrelevant here, only the approval id within the tool_call matters.
                //     msg.with_tool_request_and_approval(request.id.clone(), request.tool_call.clone(), Some(ApprovalRequest::internal()))
                // });

                // Then dispatch each in parallel
                let futures: Vec<_> = tool_requests
                    .iter()
                    .map(|request| {
                        self.dispatch_tool_call(request.tool_call.clone(), rejected_tool_request_ids.contains(&request.id))
                    })
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

                // Update conversation history after the tool call round
                let status = self.get_system_status().await?;
                self.rewrite_messages_on_tool_response(
                    &mut messages,
                    vec![response.clone(), message_tool_response],
                    status,
                ).await?;
            }
        }))
    }
}

async fn permission_denied_content() -> AgentResult<Vec<Content>> {
    Ok(vec![Content::text(
        "Permission denied by the user, try an alternative approach.",
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::message::MessageContent;
    use crate::providers::mock::MockProvider;
    use async_trait::async_trait;
    use futures::TryStreamExt;
    use serde_json::json;
    use std::collections::HashMap;

    // Mock system for testing
    struct MockSystem {
        name: String,
        tools: Vec<Tool>,
    }

    impl MockSystem {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                tools: vec![Tool::new(
                    "echo",
                    "Echoes back the input",
                    json!({"type": "object", "properties": {"message": {"type": "string"}}, "required": ["message"]}),
                )],
            }
        }
    }

    #[async_trait]
    impl System for MockSystem {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            "A mock system for testing"
        }

        fn instructions(&self) -> &str {
            "Mock system instructions"
        }

        fn tools(&self) -> &[Tool] {
            &self.tools
        }

        async fn status(&self) -> anyhow::Result<HashMap<String, serde_json::Value>> {
            Ok(HashMap::new())
        }

        async fn call(&self, tool_call: ToolCall) -> AgentResult<Vec<Content>> {
            match tool_call.name.as_str() {
                "echo" => Ok(vec![Content::text(
                    tool_call.arguments["message"].as_str().unwrap_or(""),
                )]),
                _ => Err(AgentError::ToolNotFound(tool_call.name)),
            }
        }
    }

    #[tokio::test]
    async fn test_simple_response() -> Result<()> {
        let response = Message::assistant().with_text("Hello!");
        let provider = MockProvider::new(vec![response.clone()]);
        let agent = Agent::new(Box::new(provider));

        let initial_message = Message::user().with_text("Hi");
        let initial_messages = vec![initial_message];

        let mut stream = agent
            .reply(&initial_messages, mock_approval_monitor())
            .await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], response);
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call() -> Result<()> {
        let mut agent = Agent::new(Box::new(MockProvider::new(vec![
            Message::assistant().with_tool_request(
                "1",
                Ok(ToolCall::new("test_echo", json!({"message": "test"}))),
            ),
            Message::assistant().with_text("Done!"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test")));

        let initial_message = Message::user().with_text("Echo test");
        let initial_messages = vec![initial_message];

        let mut stream = agent
            .reply(&initial_messages, mock_approval_monitor())
            .await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        // Should have three messages: tool request, response, and model text
        assert_eq!(messages.len(), 3);
        assert!(messages[0]
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
        assert_eq!(messages[2].content[0], MessageContent::text("Done!"));
        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_tool() -> Result<()> {
        let mut agent = Agent::new(Box::new(MockProvider::new(vec![
            Message::assistant()
                .with_tool_request("1", Ok(ToolCall::new("invalid_tool", json!({})))),
            Message::assistant().with_text("Error occurred"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test")));

        let initial_message = Message::user().with_text("Invalid tool");
        let initial_messages = vec![initial_message];

        let mut stream = agent
            .reply(&initial_messages, mock_approval_monitor())
            .await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        // Should have three messages: failed tool request, fail response, and model text
        assert_eq!(messages.len(), 3);
        assert!(messages[0]
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
        assert_eq!(
            messages[2].content[0],
            MessageContent::text("Error occurred")
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_tool_calls() -> Result<()> {
        let mut agent = Agent::new(Box::new(MockProvider::new(vec![
            Message::assistant()
                .with_tool_request(
                    "1",
                    Ok(ToolCall::new("test_echo", json!({"message": "first"}))),
                )
                .with_tool_request(
                    "2",
                    Ok(ToolCall::new("test_echo", json!({"message": "second"}))),
                ),
            Message::assistant().with_text("All done!"),
        ])));

        agent.add_system(Box::new(MockSystem::new("test")));

        let initial_message = Message::user().with_text("Multiple calls");
        let initial_messages = vec![initial_message];

        let mut stream = agent
            .reply(&initial_messages, mock_approval_monitor())
            .await?;
        let mut messages = Vec::new();
        while let Some(msg) = stream.try_next().await? {
            messages.push(msg);
        }

        // Should have three messages: tool requests, responses, and model text
        assert_eq!(messages.len(), 3);
        assert!(messages[0]
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::ToolRequest(_))));
        assert_eq!(messages[2].content[0], MessageContent::text("All done!"));
        Ok(())
    }

    fn mock_approval_monitor() -> ApprovalMonitor {
        let (approval_tx, approval_rx) = tokio::sync::mpsc::channel(1);
        let (rejection_tx, rejection_rx) = tokio::sync::mpsc::channel(1);
        ApprovalMonitor {
            approval_rx,
            rejection_rx,
        }
    }
}
