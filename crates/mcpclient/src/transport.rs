use crate::types::JsonRpcMessage;
use async_trait::async_trait;
use tokio::sync::mpsc::{Receiver, Sender};

// Stream types for consistent interface
pub type ReadStream = Receiver<Result<JsonRpcMessage, Box<dyn std::error::Error + Send>>>;
pub type WriteStream = Sender<JsonRpcMessage>;

// Common trait for transport implementations
#[async_trait]
pub trait Transport {
    async fn connect(&self)
        -> Result<(ReadStream, WriteStream), Box<dyn std::error::Error + Send>>;
}
