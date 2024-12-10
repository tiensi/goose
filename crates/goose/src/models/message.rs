use super::content::{Content, ImageContent, TextContent};
use super::role::Role;
use super::tool::ToolCall;
use crate::errors::AgentResult;
use chrono::Utc;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolRequest {
    pub id: String,
    pub tool_call: AgentResult<ToolCall>,
    pub approval_request: Option<ApprovalRequest>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub approval_type: ApprovalType,
}
impl ApprovalRequest {
    pub fn internal() -> Self {
        ApprovalRequest {
            id: uuid::Uuid::new_v4().to_string(),
            approval_type: ApprovalType::Internal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ApprovalType {
    Internal, // Send an approval response to goose with the approval request id.
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolResponse {
    pub id: String,
    pub tool_result: AgentResult<Vec<Content>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// Content passed inside a message, which can be both simple content and tool content
pub enum MessageContent {
    Text(TextContent),
    Image(ImageContent),
    ToolRequest(ToolRequest),
    ToolResponse(ToolResponse),
}

impl MessageContent {
    pub fn text<S: Into<String>>(text: S) -> Self {
        MessageContent::Text(TextContent {
            text: text.into(),
            audience: None,
            priority: None,
        })
    }

    pub fn image<S: Into<String>, T: Into<String>>(data: S, mime_type: T) -> Self {
        MessageContent::Image(ImageContent {
            data: data.into(),
            mime_type: mime_type.into(),
            audience: None,
            priority: None,
        })
    }

    pub fn tool_request<S: Into<String>>(id: S, tool_call: AgentResult<ToolCall>) -> Self {
        MessageContent::ToolRequest(ToolRequest {
            id: id.into(),
            tool_call,
            approval_request: None,
        })
    }

    pub fn tool_request_with_approval<S: Into<String>>(
        id: S,
        tool_call: AgentResult<ToolCall>,
        approval_request: Option<ApprovalRequest>,
    ) -> Self {
        MessageContent::ToolRequest(ToolRequest {
            id: id.into(),
            tool_call,
            approval_request: approval_request,
        })
    }

    pub fn tool_response<S: Into<String>>(id: S, tool_result: AgentResult<Vec<Content>>) -> Self {
        MessageContent::ToolResponse(ToolResponse {
            id: id.into(),
            tool_result,
        })
    }

    pub fn as_tool_request(&self) -> Option<&ToolRequest> {
        if let MessageContent::ToolRequest(ref tool_request) = self {
            Some(tool_request)
        } else {
            None
        }
    }

    /// Get the text content if this is a TextContent variant
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(text) => Some(&text.text),
            _ => None,
        }
    }
}

impl From<Content> for MessageContent {
    fn from(content: Content) -> Self {
        match content {
            Content::Text(text) => MessageContent::Text(text),
            Content::Image(image) => MessageContent::Image(image),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
/// A message to or from an LLM
pub struct Message {
    pub role: Role,
    pub created: i64,
    pub content: Vec<MessageContent>,
}

impl Message {
    /// Create a new user message with the current timestamp
    pub fn user() -> Self {
        Message {
            role: Role::User,
            created: Utc::now().timestamp(),
            content: Vec::new(),
        }
    }

    /// Create a new assistant message with the current timestamp
    pub fn assistant() -> Self {
        Message {
            role: Role::Assistant,
            created: Utc::now().timestamp(),
            content: Vec::new(),
        }
    }

    /// Add any MessageContent to the message
    pub fn with_content(mut self, content: MessageContent) -> Self {
        self.content.push(content);
        self
    }

    /// Add text content to the message
    pub fn with_text<S: Into<String>>(self, text: S) -> Self {
        self.with_content(MessageContent::text(text))
    }

    /// Add image content to the message
    pub fn with_image<S: Into<String>, T: Into<String>>(self, data: S, mime_type: T) -> Self {
        self.with_content(MessageContent::image(data, mime_type))
    }

    /// Add a tool request to the message
    pub fn with_tool_request<S: Into<String>>(
        self,
        id: S,
        tool_call: AgentResult<ToolCall>,
    ) -> Self {
        self.with_content(MessageContent::tool_request(id, tool_call))
    }

    pub fn with_tool_request_and_approval<S: Into<String>>(
        self,
        id: S,
        tool_call: AgentResult<ToolCall>,
        approval_request: Option<ApprovalRequest>,
    ) -> Self {
        self.with_content(MessageContent::tool_request_with_approval(
            id,
            tool_call,
            approval_request,
        ))
    }

    /// Add a tool response to the message
    pub fn with_tool_response<S: Into<String>>(
        self,
        id: S,
        result: AgentResult<Vec<Content>>,
    ) -> Self {
        self.with_content(MessageContent::tool_response(id, result))
    }
}
