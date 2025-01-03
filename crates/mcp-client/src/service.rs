use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::transport::{Error as TransportError, TransportHandle};
use mcp_core::protocol::JsonRpcMessage;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Request timed out")]
    Timeout(#[from] tower::timeout::error::Elapsed),

    #[error("Transport not initialized")]
    NotInitialized,

    #[error("Transport already initialized")]
    AlreadyInitialized,

    #[error("Other error: {0}")]
    Other(String),

    #[error("Unexpected server response")]
    UnexpectedResponse,
}

/// A Tower `Service` implementation that uses a `Transport` to send/receive JsonRpcMessages.
pub struct TransportService {
    handle: TransportHandle,
}

impl TransportService {
    pub fn new(handle: TransportHandle) -> Self {
        Self { handle }
    }
}

impl tower::Service<JsonRpcMessage> for TransportService {
    type Response = JsonRpcMessage;
    type Error = ServiceError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: JsonRpcMessage) -> Self::Future {
        let handle = self.handle.clone();
        Box::pin(async move {
            match handle.send(message).await {
                Ok(response) => Ok(response),
                Err(e) => Err(ServiceError::Transport(e)),
            }
        })
    }
}
