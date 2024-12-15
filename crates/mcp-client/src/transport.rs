use anyhow::Result;
use async_trait::async_trait;
use mcp_core::types::JsonRpcMessage;
use tokio::sync::mpsc::{Receiver, Sender};
use thiserror::Error;

/// Errors related to reading from a transport
#[derive(Debug, Error)]
pub enum ReadError {
    #[error("Invalid JSON message: {0}")]
    InvalidMessage(String),
    #[error("Transport connection was closed")]
    TransportClosed,
    #[error("Child process terminated: {0}")]
    ChildTerminated(String),
    #[error("Unknown read error: {0}")]
    Unknown(String),
}

/// Errors related to writing to a transport
#[derive(Debug, Error)]
pub enum WriteError {
    #[error("Failed to serialize message: {0}")]
    SerializationError(String),
    #[error("Transport write channel was closed")]
    TransportClosed,
    #[error("Unknown write error: {0}")]
    Unknown(String),
}

/// Errors related to establishing a transport connection
#[derive(Debug, Error)]
pub enum ConnectError {
    #[error("Failed to spawn child process: {0}")]
    SpawnError(String),
    #[error("Unsupported transport configuration")]
    UnsupportedConfiguration,
    #[error("Unknown connection error: {0}")]
    Unknown(String),
}


// Stream types for consistent interface
pub type ReadStream = Receiver<Result<JsonRpcMessage, ReadError>>;
pub type WriteStream = Sender<Result<JsonRpcMessage, WriteError>>;


// Common trait for transport implementations
#[async_trait]
pub trait Transport {
    async fn connect(&self) -> Result<(ReadStream, WriteStream), ConnectError>;
}
