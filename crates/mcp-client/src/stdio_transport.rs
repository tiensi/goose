use crate::transport::{ConnectError, ReadError, ReadStream, Transport, WriteError, WriteStream};
use anyhow::Result;
use async_trait::async_trait;
use mcp_core::types::*;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

pub struct StdioServerParams {
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

pub struct StdioTransport {
    pub params: StdioServerParams,
}

impl StdioTransport {
    fn get_default_environment() -> std::collections::HashMap<String, String> {
        let default_vars = if cfg!(windows) {
            vec!["APPDATA", "PATH", "TEMP", "USERNAME"] // Simplified list
        } else {
            vec!["HOME", "PATH", "SHELL", "USER"] // Simplified list
        };

        std::env::vars()
            .filter(|(key, value)| default_vars.contains(&key.as_str()) && !value.starts_with("()"))
            .collect()
    }

    async fn monitor_child(
        mut child: Child,
        tx_read: mpsc::Sender<Result<JsonRpcMessage, ReadError>>,
    ) {
        match child.wait().await {
            Ok(status) => {
                let msg = if status.success() {
                    format!("Terminated normally with status: {}", status)
                } else {
                    format!("Terminated with error status: {}", status)
                };
                let _ = tx_read.send(Err(ReadError::ChildTerminated(msg))).await;
            }
            Err(e) => {
                let _ = tx_read
                    .send(Err(ReadError::ChildTerminated(e.to_string())))
                    .await;
            }
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn connect(&self) -> Result<(ReadStream, WriteStream), ConnectError> {
        let mut child = Command::new(&self.params.command)
            .args(&self.params.args)
            .env_clear()
            .envs(
                self.params
                    .env
                    .clone()
                    .unwrap_or_else(Self::get_default_environment),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| ConnectError::SpawnError(e.to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| ConnectError::Unknown("Missing stdin handle".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ConnectError::Unknown("Missing stdout handle".to_string()))?;

        let (tx_read, rx_read) = mpsc::channel::<Result<JsonRpcMessage, ReadError>>(100);
        let (tx_write, mut rx_write) = mpsc::channel::<Result<JsonRpcMessage, WriteError>>(100);

        // Clone tx_read for the child monitor
        let tx_read_monitor = tx_read.clone();

        // Spawn child process monitor
        tokio::spawn(Self::monitor_child(child, tx_read_monitor));

        // Spawn stdout reader task
        let stdout_reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut lines = stdout_reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<JsonRpcMessage>(&line) {
                    Ok(msg) => {
                        if tx_read.send(Ok(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx_read
                            .send(Err(ReadError::InvalidMessage(e.to_string())))
                            .await;
                    }
                }
            }
            // // Notify on EOF
            // let _ = tx_read.send(Err(ReadError::TransportClosed)).await;
        });

        // Spawn stdin writer task
        tokio::spawn(async move {
            let mut stdin = stdin;

            // Serialize the message into JSON
            fn serialize_message(message: &JsonRpcMessage) -> Result<String, WriteError> {
                serde_json::to_string(message)
                    .map_err(|err| WriteError::SerializationError(err.to_string()))
            }

            // Write the JSON message to the transport (stdin)
            async fn write_to_transport(
                stdin: &mut tokio::process::ChildStdin,
                json: &str,
            ) -> Result<(), WriteError> {
                stdin
                    .write_all(format!("{}\n", json).as_bytes())
                    .await
                    .map_err(|_| WriteError::TransportClosed)
            }

            // Log an error received from the channel
            fn log_channel_error(error: &WriteError) {
                match error {
                    WriteError::SerializationError(msg) => {
                        eprintln!("Serialization error: {}", msg);
                    }
                    WriteError::TransportClosed => {
                        eprintln!("Transport closed; stopping writer task.");
                    }
                    WriteError::Unknown(msg) => {
                        eprintln!("Unknown error: {}", msg);
                    }
                }
            }

            // Handle the result of receiving a message and writing it to the transport
            async fn handle_message_result(
                result: Result<JsonRpcMessage, WriteError>,
                stdin: &mut tokio::process::ChildStdin,
            ) -> Result<(), WriteError> {
                match result {
                    Ok(message) => {
                        // Serialize and write the message
                        let json = serialize_message(&message)?;
                        write_to_transport(stdin, &json).await?;
                        Ok(())
                    }
                    Err(error) => {
                        log_channel_error(&error);
                        Err(error)
                    }
                }
            }

            while let Some(message) = rx_write.recv().await {
                // Handle the message or break on fatal errors
                if let Err(error) = handle_message_result(message, &mut stdin).await {
                    // Only break if the error is fatal
                    if matches!(error, WriteError::TransportClosed) {
                        break;
                    }
                }
            }
        });

        Ok((rx_read, tx_write))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_stdio_transport() {
        let transport = StdioTransport {
            params: StdioServerParams {
                command: "tee".to_string(), // tee will echo back what it receives
                args: vec![],
                env: None,
            },
        };

        let (mut rx, tx) = transport.connect().await.unwrap();

        // Create test messages
        let request = JsonRpcMessage::Request(JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(1),
            method: "ping".to_string(),
            params: None,
        });

        let response = JsonRpcMessage::Response(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(2),
            result: Some(json!({})),
            error: None,
        });

        // Send messages
        tx.send(Ok(request.clone())).await.unwrap();
        tx.send(Ok(response.clone())).await.unwrap();

        // Receive and verify messages
        let mut read_messages = Vec::new();

        // Use timeout to avoid hanging if messages aren't received
        for _ in 0..2 {
            match timeout(Duration::from_secs(1), rx.recv()).await {
                Ok(Some(Ok(msg))) => read_messages.push(msg),
                Ok(Some(Err(e))) => panic!("Received error: {}", e),
                Ok(None) => break,
                Err(_) => panic!("Timeout waiting for message"),
            }
        }

        assert_eq!(read_messages.len(), 2, "Expected 2 messages");
        assert_eq!(read_messages[0], request);
        assert_eq!(read_messages[1], response);
    }

    #[tokio::test]
    async fn test_process_termination() {
        let transport = StdioTransport {
            params: StdioServerParams {
                command: "sleep".to_string(),
                args: vec!["0.3".to_string()],
                env: None,
            },
        };
        let (mut rx, _tx) = transport.connect().await.unwrap();

        // should get an error about process termination - either normal termination or transport connection was closed
        match timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Some(Err(e))) => {
                assert!(
                    e.to_string().contains("Terminated normally") || e.to_string().contains("Transport connection was closed"),
                    "Expected process termination error, got: {}",
                    e
                );
            }
            _ => panic!("Expected error, got a different message"),
        }
    }
}
