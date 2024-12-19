use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, Mutex};
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
    router: Mutex<Option<MessageRouter>>,
    initialized: AtomicBool,
}

impl<T: Transport> TransportServiceInner<T> {
    async fn ensure_initialized(&self) -> Result<MessageRouter, ServiceError> {
        if !self.initialized.load(Ordering::SeqCst) {
            let mut router_guard = self.router.lock().await;

            // Double-check after acquiring lock
            if !self.initialized.load(Ordering::SeqCst) {
                // Start the transport
                let transport_tx = self
                    .transport
                    .start()
                    .await
                    .map_err(ServiceError::Transport)?;

                // Create shutdown channel
                let (shutdown_tx, _shutdown_rx) = mpsc::channel(1);

                // Create and store the router
                let router = MessageRouter::new(transport_tx, shutdown_tx);
                *router_guard = Some(router);

                self.initialized.store(true, Ordering::SeqCst);
            }
        }

        // Get a clone of the router
        Ok(self
            .router
            .lock()
            .await
            .as_ref()
            .ok_or(ServiceError::NotInitialized)?
            .clone())
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
                router: Mutex::new(None),
                initialized: AtomicBool::new(false),
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
                        .send_notification(JsonRpcMessage::Notification(notification))
                        .await
                        .map_err(ServiceError::Transport)?;
                    Ok(JsonRpcMessage::Nil)
                }
                JsonRpcMessage::Request(request) => router
                    .send_request(JsonRpcMessage::Request(request))
                    .await
                    .map_err(ServiceError::Transport),
                _ => Err(ServiceError::Other("Invalid message type".to_string())),
            }
        })
    }
}

impl<T: Transport> Drop for TransportServiceInner<T> {
    fn drop(&mut self) {
        if self.initialized.load(Ordering::SeqCst) {
            // Create a new runtime for cleanup if needed
            let rt = tokio::runtime::Runtime::new().unwrap();

            // Request shutdown through the router if it exists
            if let Some(router) = rt.block_on(self.router.lock()).as_ref() {
                let _ = rt.block_on(router.shutdown());
            }

            // Close the transport
            let _ = rt.block_on(self.transport.close());
        }
    }
}
