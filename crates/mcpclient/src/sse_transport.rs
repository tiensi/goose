use crate::transport::{ReadStream, Transport, WriteStream};
use crate::types::JsonRpcMessage;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{header, Client, Response};
use tokio::sync::{mpsc, oneshot};
use url::Url;
use std::sync::Arc;
use tokio::sync::Mutex;


pub struct SSEServerParams {
    pub url: Url,
    pub headers: Option<header::HeaderMap>,
    pub timeout: std::time::Duration,
    pub sse_read_timeout: std::time::Duration,
}

impl Clone for SSEServerParams {
    fn clone(&self) -> Self {
        SSEServerParams {
            url: self.url.clone(),
            headers: self.headers.clone(),
            timeout: self.timeout,
            sse_read_timeout: self.sse_read_timeout,
        }
    }
}

pub struct SSETransport {
    pub params: SSEServerParams,
    endpoint_url: Arc<Mutex<Option<Url>>>,
}

impl SSETransport {
    pub fn new(params: SSEServerParams) -> Self {
        Self {
            params,
            endpoint_url: Arc::new(Mutex::new(None))
        }
    }

    async fn handle_sse_events(
        client: Client,
        url: &str,
        endpoint_sender: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send>> {
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].to_string();
                buffer = buffer[pos + 1..].to_string();

                if line.trim().is_empty() {
                    continue;
                }

                println!("Received line: {}", line);

                if line.starts_with("event:") {
                    let event_type = line[6..].trim();
                    if let Some(pos) = buffer.find('\n') {
                        let data_line = buffer[..pos].to_string();
                        buffer = buffer[pos + 1..].to_string();

                        if data_line.starts_with("data:") {
                            let data = data_line[5..].trim();
                            println!("Parsed event: {}, data: {}", event_type, data);

                            match event_type {
                                "endpoint" => {
                                    if let Ok(url) = base_url.join(data) {
                                        // Validate URL origin matches
                                        if url.scheme() != base_url.scheme() || url.host() != base_url.host() {
                                            eprintln!("Endpoint origin does not match connection origin: {}", url);
                                            return Err(Box::new(std::io::Error::new(
                                                std::io::ErrorKind::InvalidData,
                                                "Invalid endpoint origin"
                                            )));
                                        }
                                        let mut endpoint_guard = endpoint_url.lock().await;
                                        *endpoint_guard = Some(url);
                                        println!("Updated endpoint URL: {}", endpoint_guard.as_ref().unwrap());
                                    }
                                }
                                "message" => {
                                    match serde_json::from_str::<JsonRpcMessage>(data) {
                                        Ok(msg) => {
                                            println!("Received message: {:?}", msg);
                                            if tx_read.send(Ok(msg)).await.is_err() {
                                                eprintln!("Failed to send message to channel");
                                                return Ok(());
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("Failed to parse message: {}", e);
                                            let _ = tx_read.send(Err(Box::new(e))).await;
                                        }
                                    }
                                }
                                _ => println!("Unknown event type: {}", event_type),
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn send_request(
        client: &Client,
        endpoint_url: &str,
        request: &JsonRpcMessage,
    ) -> Result<String, Box<dyn std::error::Error>> {
        // Log the request being sent
        println!("\nSending request to {}: {:?}", endpoint_url, request);

        let response = client.post(endpoint_url).json(request).send().await?;

        // Small delay to ensure server processes initialize
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let status = response.status();
        let text = response.text().await?;

        if status != reqwest::StatusCode::ACCEPTED {
            return Err(format!("Request failed: {} - {}", status, text).into());
        }

        Ok(text)
    }

}

#[async_trait]
impl Transport for SSETransport {
    async fn connect(
        &self,
    ) -> Result<(ReadStream, WriteStream), Box<dyn std::error::Error + Send>> {
        let (tx_read, rx_read) = mpsc::channel(100);
        let (tx_write, rx_write) = mpsc::channel(100);

        let client = Client::builder()
            .timeout(self.params.timeout)
            .build()?;

        let sse_url = self.params.url.join("sse")?;
        println!("Connecting to SSE endpoint: {}", sse_url);

        // oneshot channel to send the endpoint url to the main task
        // handle SSE events is supposed to populate the stream but we need to send the endpoint url to the main task
        let (endpoint_sender, endpoint_receiver) = oneshot::channel();
        tokio::spawn(async move {
            if let Err(_) = endpoint_sender.send(sse_url) {
                println!("the receiver dropped");
            }
        });

        match endpoint_receiver.await {
            Ok(v) => println!("got = {:?}", v),
            Err(_) => println!("the sender dropped"),
        }


        // spawn the SSE event handler
        let endpoint_url = self.params.url.join("sse")?;
        let sse_client = client.clone();
        tokio::spawn(async move {
            Self::handle_sse_events(sse_client, sse_url, endpoint_sender).await;
        });

        let response = client
            .get(sse_url.as_str())
            .header("Accept", "text/event-stream")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to connect to SSE endpoint: {}", response.status()).into());
        }

        let endpoint_url = self.endpoint_url.clone();

        // Clone URL before moving into spawn
        let base_url = sse_url.clone();
        tokio::spawn(Self::handle_sse_events(
            response,
            tx_read.clone(),
            endpoint_url.clone(),
            base_url,
        ));

        // Spawn POST request handler
        let client_clone = client.clone();
        tokio::spawn(async move {
            while let Some(message) = rx_write.recv().await {
                if let Some(endpoint) = &*endpoint_url.lock().await {
                    Self::send_request(&client_clone, endpoint.as_str(), &message).await;
                }
            }
        });

        Ok((rx_read, tx_write))
    }
}
