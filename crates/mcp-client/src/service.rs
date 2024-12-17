use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::Mutex;
use tower::Service;

use crate::transport::{Error as TransportError, Transport};
use mcp_core::protocol::{JsonRpcMessage, JsonRpcRequest};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),

    #[error("Unexpected server response")]
    UnexpectedResponse,
}

/// A Tower `Service` implementation that uses a `Transport` to send/receive JsonRpcRequests and JsonRpcMessages.
pub struct TransportService<T> {
    transport: Arc<Mutex<T>>,
    initialized: AtomicBool,
}

impl<T: Transport> TransportService<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(Mutex::new(transport)),
            initialized: AtomicBool::new(false),
        }
    }
}

impl<T: Transport> Service<JsonRpcRequest> for TransportService<T> {
    type Response = JsonRpcMessage;
    type Error = ServiceError;
    type Future = Pin<Box<dyn Future<Output = Result<JsonRpcMessage, ServiceError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Always ready. We do on-demand initialization in call().
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: JsonRpcRequest) -> Self::Future {
        let transport = Arc::clone(&self.transport);
        let started = self.initialized.load(Ordering::SeqCst);

        Box::pin(async move {
            let mut transport = transport.lock().await;

            // Initialize (start) transport on the first call.
            if !started {
                transport.start().await?;
            }

            // Serialize request to JSON line
            let msg = serde_json::to_string(&request)?;
            transport.send(msg).await?;

            let line = transport.receive().await?;
            let response_msg: JsonRpcMessage = serde_json::from_str(&line)?;

            Ok(response_msg)
        })
    }
}
