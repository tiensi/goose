use crate::transport::{ReadStream, Transport, WriteStream};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use mcp_core::protocol::JsonRpcMessage;
use reqwest::{Client, Url};
use reqwest_eventsource::{Event, EventSource};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_retry::{
    strategy::{jitter, ExponentialBackoff},
    Retry,
};
use tracing::{debug, error, info, warn};

pub struct SseTransportParams {
    pub url: String,
    pub headers: Option<reqwest::header::HeaderMap>,
}

pub struct SseTransport {
    pub params: SseTransportParams,
}

// Helper function to send a POST request with retry logic
async fn send_with_retry(
    client: &Client,
    endpoint: &str,
    json: serde_json::Value,
) -> Result<reqwest::Response> {
    // Create retry strategy with exponential backoff
    let retry_strategy = ExponentialBackoff::from_millis(100) // Start with 100ms
        .factor(2) // Double the delay each time
        .map(jitter) // Add randomness to prevent thundering herd
        .take(3); // Maximum of 3 retries (4 attempts total)

    Retry::spawn(retry_strategy, || async {
        let response = client.post(endpoint).json(&json).send().await?;

        // If we get a 5xx error or specific connection errors, we should retry
        if response.status().is_server_error()
            || matches!(response.error_for_status_ref(), Err(e) if e.is_connect())
        {
            return Err(anyhow!("Server error: {}", response.status()));
        }

        Ok(response)
    })
    .await
}

#[async_trait]
impl Transport for SseTransport {
    async fn connect(&self) -> Result<(ReadStream, WriteStream)> {
        info!("Connecting to SSE endpoint: {}", self.params.url);
        let (tx_read, rx_read) = mpsc::channel(100);
        let (tx_write, mut rx_write) = mpsc::channel(100);

        let client = Client::new();
        let base_url = Url::parse(&self.params.url).context("Failed to parse SSE URL")?;

        // Create the event source request
        let mut request_builder = client.get(base_url.clone());
        if let Some(headers) = &self.params.headers {
            request_builder = headers
                .iter()
                .fold(request_builder, |req, (key, value)| req.header(key, value));
        }

        let event_source = EventSource::new(request_builder)?;
        let client_for_post = client.clone();

        // Shared state for the endpoint URL
        let endpoint_url = Arc::new(Mutex::new(None::<String>));
        let endpoint_url_reader = endpoint_url.clone();

        // Spawn the SSE reader task
        tokio::spawn({
            let tx_read = tx_read.clone();
            let base_url = base_url.clone();
            async move {
                info!("Starting SSE reader task");
                let mut stream = event_source;
                let mut got_endpoint = false;

                while let Some(event) = stream.next().await {
                    match event {
                        Ok(Event::Open) => {
                            info!("SSE connection opened");
                        }
                        Ok(Event::Message(message)) => {
                            debug!("Received SSE event: {} - {}", message.event, message.data);
                            match message.event.as_str() {
                                "endpoint" => {
                                    // Handle endpoint event
                                    let endpoint = message.data;
                                    info!("Received endpoint URL: {}", endpoint);

                                    // Join with base URL if relative
                                    let endpoint_url_full = if endpoint.starts_with('/') {
                                        match base_url.join(&endpoint) {
                                            Ok(url) => url,
                                            Err(e) => {
                                                error!("Failed to join endpoint URL: {}", e);
                                                let _ = tx_read.send(Err(e.into())).await;
                                                break;
                                            }
                                        }
                                    } else {
                                        match Url::parse(&endpoint) {
                                            Ok(url) => url,
                                            Err(e) => {
                                                error!("Failed to parse endpoint URL: {}", e);
                                                let _ = tx_read.send(Err(e.into())).await;
                                                break;
                                            }
                                        }
                                    };

                                    // Validate endpoint URL has same origin (scheme and host)
                                    if base_url.scheme() != endpoint_url_full.scheme()
                                        || base_url.host_str() != endpoint_url_full.host_str()
                                        || base_url.port() != endpoint_url_full.port()
                                    {
                                        let error = format!(
                                            "Endpoint origin does not match connection origin: {}",
                                            endpoint_url_full
                                        );
                                        error!("{}", error);
                                        let _ = tx_read.send(Err(anyhow!(error))).await;
                                        break;
                                    }

                                    let endpoint_str = endpoint_url_full.to_string();
                                    info!("Using full endpoint URL: {}", endpoint_str);
                                    let mut endpoint_guard = endpoint_url.lock().await;
                                    *endpoint_guard = Some(endpoint_str);
                                    got_endpoint = true;
                                    debug!("Endpoint URL set successfully");
                                }
                                "message" => {
                                    if !got_endpoint {
                                        warn!("Received message before endpoint URL");
                                        continue;
                                    }
                                    // Handle message event
                                    match serde_json::from_str::<JsonRpcMessage>(&message.data) {
                                        Ok(msg) => {
                                            debug!("Received server message: {:?}", msg);
                                            if tx_read.send(Ok(msg)).await.is_err() {
                                                error!("Failed to send message to read channel");
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            error!("Error parsing server message: {}", e);
                                            if tx_read.send(Err(e.into())).await.is_err() {
                                                error!("Failed to send error to read channel");
                                                break;
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    debug!("Ignoring unknown event type: {}", message.event);
                                }
                            }
                        }
                        Err(e) => {
                            error!("SSE error: {}", e);
                            let _ = tx_read.send(Err(e.into())).await;
                            break;
                        }
                    }
                }
                info!("SSE reader task ended");
            }
        });

        // Spawn the writer task
        tokio::spawn(async move {
            info!("Starting writer task");
            // Wait for the endpoint URL before processing messages
            let mut endpoint = None;
            while endpoint.is_none() {
                let guard = endpoint_url_reader.lock().await;
                if let Some(url) = guard.as_ref() {
                    endpoint = Some(url.clone());
                    break;
                }
                drop(guard);
                debug!("Waiting for endpoint URL...");
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            let endpoint = endpoint.unwrap();
            info!("Starting post writer with endpoint URL: {}", endpoint);

            while let Some(message) = rx_write.recv().await {
                match serde_json::to_value(&message) {
                    Ok(json) => {
                        debug!("Sending client message: {:?}", json);
                        match send_with_retry(&client_for_post, &endpoint, json).await {
                            Ok(response) => {
                                if !response.status().is_success() {
                                    let status = response.status();
                                    let text = response.text().await.unwrap_or_default();
                                    error!("Server returned error status {}: {}", status, text);
                                } else {
                                    debug!("Message sent successfully: {}", response.status());
                                }
                            }
                            Err(e) => {
                                error!("Failed to send message after retries: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to serialize message: {}", e);
                    }
                }
            }
            info!("Writer task ended");
        });

        info!("SSE transport connected");
        Ok((rx_read, tx_write))
    }
}
