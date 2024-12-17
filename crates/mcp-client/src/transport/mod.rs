use async_trait::async_trait;
use thiserror::Error;

/// A generic error type for transport operations.
#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Transport was not connected or is already closed")]
    NotConnected,

    #[error("Unexpected transport error: {0}")]
    Other(String),
}

/// A generic asynchronous transport trait.
///
/// Implementations are expected to handle:
/// - starting the underlying communication channel (e.g., launching a child process, connecting a socket)
/// - sending JSON-RPC messages as strings
/// - receiving JSON-RPC messages as strings
/// - closing the transport cleanly
#[async_trait]
pub trait Transport: Send + 'static {
    /// Start the transport and establish the underlying connection.
    async fn start(&mut self) -> Result<(), Error>;

    /// Send a raw JSON-encoded message through the transport.
    async fn send(&mut self, msg: String) -> Result<(), Error>;

    /// Receive a raw JSON-encoded message from the transport.
    ///
    /// This should return a single line representing one JSON message.
    async fn receive(&mut self) -> Result<String, Error>;

    /// Close the transport and free any resources.
    async fn close(&mut self) -> Result<(), Error>;
}

pub mod stdio;
pub use stdio::StdioTransport;
