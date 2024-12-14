use anyhow::Result;
use async_trait::async_trait;
use mcp_core::types::JsonRpcMessage;
use tokio::sync::mpsc::{Receiver, Sender};

// Stream types for consistent interface
pub type ReadStream = Receiver<Result<JsonRpcMessage>>;
pub type WriteStream = Sender<JsonRpcMessage>;

// Common trait for transport implementations
#[async_trait]
pub trait Transport {
    async fn connect(&self) -> Result<(ReadStream, WriteStream)>;
}
