use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, Command};
use tokio::sync::broadcast;

use crate::types::JsonRpcRequest;

pub struct StdioClient {
    process: Child,
    writer: BufWriter<tokio::process::ChildStdin>,
    // message_rx: broadcast::Receiver<String>,
    message_tx: broadcast::Sender<String>,
}

impl StdioClient {
    pub async fn new(command: &str, args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut process = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = process.stdin.take().expect("Failed to get stdin");
        let stdout = process.stdout.take().expect("Failed to get stdout");

        let writer = BufWriter::new(stdin);
        let reader = BufReader::new(stdout);
        let (message_tx, _message_rx) = broadcast::channel(100);

        let tx = message_tx.clone();
        tokio::spawn(async move {
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Err(_e) = tx.send(line) {
                    println!("Receiver dropped, stopping reader task");
                    break;
                }
            }
        });

        Ok(Self {
            process,
            writer,
            // message_rx,
            message_tx,
        })
    }

    pub async fn send_message(&mut self, message: &str) -> Result<(), std::io::Error> {
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_request(&mut self, request: &JsonRpcRequest) -> Result<(), std::io::Error> {
        let json = serde_json::to_string(&request)?;
        println!("\nSending: {}", json);
        self.send_message(&json).await
    }

    pub async fn send_notification(
        &mut self,
        notification: &JsonRpcRequest,
    ) -> Result<(), std::io::Error> {
        let json = serde_json::to_string(&notification)?;
        println!("\nSending notification: {}", json);
        self.send_message(&json).await
    }

    pub fn message_receiver(&self) -> broadcast::Receiver<String> {
        self.message_tx.subscribe()
    }

    pub async fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.process.kill().await?;
        Ok(())
    }
}
