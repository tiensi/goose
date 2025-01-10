use std::task::{Context, Poll};
use futures::future::BoxFuture;
use mcp_core::protocol::JsonRpcMessage;
use tower::{timeout::Timeout, Service, ServiceBuilder};

use crate::transport::{Error, TransportHandle};

/// A wrapper service that implements Tower's Service trait for MCP transport
#[derive(Clone)]
pub struct McpService<T> {
    inner: T,
}

impl<T> McpService<T> {
    pub fn new(transport: T) -> Self {
        Self { inner: transport }
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T> Service<JsonRpcMessage> for McpService<T>
where
    T: TransportHandle,
{
    type Response = JsonRpcMessage;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // Most transports are always ready, but this could be customized if needed
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: JsonRpcMessage) -> Self::Future {
        let transport = self.inner.clone();
        Box::pin(async move { transport.send(request).await })
    }
}

// Add a convenience constructor for creating a service with timeout
impl<T> McpService<T>
where
    T: TransportHandle,
{
    pub fn with_timeout(transport: T, timeout: std::time::Duration) -> Timeout<McpService<T>> {
        ServiceBuilder::new()
            .timeout(timeout)
            .service(McpService::new(transport))
    }
}

// Implement From<tower::timeout::error::Elapsed> for our Error type
impl From<tower::timeout::error::Elapsed> for Error {
    fn from(_: tower::timeout::error::Elapsed) -> Self {
        Error::Timeout
    }
}
