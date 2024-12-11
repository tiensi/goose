// use futures_util::StreamExt;
// use reqwest::{Client, Url};
// use tokio::sync::{broadcast, mpsc};

// use crate::jsonrpc::{JsonRpcRequest};

// pub struct SseClient {
//     client: Client,
//     message_rx: broadcast::Receiver<String>,
//     endpoint_tx: mpsc::Sender<String>,
//     endpoint_rx: mpsc::Receiver<String>,
// }

// impl SseClient {
//     pub async fn connect(url: &str) -> Result<Self, Box<dyn std::error::Error>> {
//         let client = Client::builder().build()?;
//         let (message_tx, message_rx) = broadcast::channel(100);
//         let (endpoint_tx, endpoint_rx) = mpsc::channel(1);

//         let sse_client = client.clone();
//         let message_tx_clone = message_tx.clone();
//         let endpoint_tx_clone = endpoint_tx.clone();

//         // Spawn SSE handler task
//         tokio::spawn(async move {
//             if let Err(e) = handle_sse_events(sse_client, url, message_tx_clone, endpoint_tx_clone).await {
//                 error!("Error in SSE handler: {:?}", e);
//             }
//         });

//         Ok(Self {
//             client,
//             message_rx,
//             endpoint_tx,
//             endpoint_rx,
//         })
//     }

//     pub async fn get_endpoint(&mut self) -> Result<String, Box<dyn std::error::Error>> {
//         let endpoint_url = self.endpoint_rx
//             .recv()
//             .await
//             .ok_or("Failed to receive endpoint URL")?;

//         // Validate endpoint URL has same origin
//         let base_url = Url::parse(endpoint_url.as_str())?;
//         let endpoint = base_url.join(&endpoint_url)?;

//         info!("Received endpoint URL: {}", endpoint);
//         Ok(endpoint.to_string())
//     }

//     pub fn subscribe_to_messages(&self) -> broadcast::Receiver<String> {
//         self.message_rx.resubscribe()
//     }

//     pub async fn send_request(
//         &self,
//         endpoint: &str,
//         request: &JsonRpcRequest
//     ) -> Result<String, Box<dyn std::error::Error>> {
//         debug!("Sending request to {}: {:?}", endpoint, request);

//         let response = self.client
//             .post(endpoint)
//             .json(request)
//             .send()
//             .await?;

//         let status = response.status();
//         let text = response.text().await?;

//         if !status.is_success() {
//             return Err(format!("Request failed: {} - {}", status, text).into());
//         }

//         Ok(text)
//     }
// }

// async fn handle_sse_events(
//     client: Client,
//     url: &str,
//     message_tx: broadcast::Sender<String>,
//     endpoint_tx: mpsc::Sender<String>,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     debug!("handle_sse_events started");

//     let response = client
//         .get(url)
//         .header("Accept", "text/event-stream")
//         .send()
//         .await?;

//     debug!("SSE connection status: {}", response.status());

//     if !response.status().is_success() {
//         return Err(format!("Failed to connect to SSE endpoint: {}", response.status()).into());
//     }

//     let mut stream = response.bytes_stream();
//     let mut buffer = String::new();

//     while let Some(chunk) = stream.next().await {
//         let chunk = chunk?;
//         let chunk_str = String::from_utf8_lossy(&chunk);
//         buffer.push_str(&chunk_str);

//         while let Some(pos) = buffer.find('\n') {
//             let line = buffer[..pos].to_string();
//             buffer = buffer[pos + 1..].to_string();

//             if line.trim().is_empty() {
//                 continue;
//             }

//             if line.starts_with("event:") {
//                 let event_type = line[6..].trim().to_string();
//                 if let Some(pos) = buffer.find('\n') {
//                     let data_line = buffer[..pos].to_string();
//                     buffer = buffer[pos + 1..].to_string();

//                     if data_line.starts_with("data:") {
//                         let data = data_line[5..].trim().to_string();
//                         debug!("Parsed event: {}, data: {}", event_type, data);

//                         match event_type.as_str() {
//                             "endpoint" => {
//                                 endpoint_tx.send(data.clone()).await?;
//                             }
//                             "message" => {
//                                 if let Err(e) = message_tx.send(data.clone()) {
//                                     error!("Failed to send message: {}", e);
//                                 }
//                             }
//                             _ => {
//                                 debug!("Unhandled event type: {}", event_type);
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//     }

//     Ok(())
// }
