use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::OnceCell;
use tower::Service;

use crate::transport::{Error as TransportError, MessageRouter, Transport};
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

struct TransportServiceInner<T: Transport> {
    transport: Arc<T>,
    router: OnceCell<MessageRouter>,
}

impl<T: Transport> TransportServiceInner<T> {
    async fn ensure_initialized(&self) -> Result<MessageRouter, ServiceError> {
        self.router
            .get_or_try_init(|| async {
                // This async block is only called once per process lifetime
                let transport_tx = self
                    .transport
                    .start()
                    .await
                    .map_err(ServiceError::Transport)?;

                Ok(MessageRouter::new(transport_tx))
            })
            .await
            .map(Clone::clone)
    }
}

/// A Tower `Service` implementation that uses a `Transport` to send/receive JsonRpcMessages.
pub struct TransportService<T: Transport> {
    inner: Arc<TransportServiceInner<T>>,
}

impl<T: Transport> TransportService<T> {
    pub fn new(transport: T) -> Self {
        Self {
            inner: Arc::new(TransportServiceInner {
                transport: Arc::new(transport),
                router: OnceCell::new(),
            }),
        }
    }
}

impl<T: Transport> Clone for TransportService<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: Transport> Service<JsonRpcMessage> for TransportService<T> {
    type Response = JsonRpcMessage;
    type Error = ServiceError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Always ready since we do lazy initialization in call()
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: JsonRpcMessage) -> Self::Future {
        let inner = Arc::clone(&self.inner);

        Box::pin(async move {
            // Ensure transport is initialized
            let router = inner.ensure_initialized().await?;

            match message {
                JsonRpcMessage::Notification(notification) => {
                    router
                        .send_notification(notification)
                        .await
                        .map_err(ServiceError::Transport)?;
                    Ok(JsonRpcMessage::Nil)
                }
                JsonRpcMessage::Request(request) => router
                    .send_request(request)
                    .await
                    .map_err(ServiceError::Transport),
                _ => Err(ServiceError::Other("Invalid message type".to_string())),
            }
        })
    }
}
