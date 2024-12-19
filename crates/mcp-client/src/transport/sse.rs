// use std::sync::Arc;
// use async_trait::async_trait;
// use reqwest::Client as HttpClient;
// use tokio::sync::{mpsc, Mutex, oneshot};
// use tokio::task::JoinHandle;
// use eventsource_client::SSE;

// use super::{Error, Transport, TransportMessage};
// use mcp_core::protocol::JsonRpcMessage;

// /// A transport implementation that uses Server-Sent Events (SSE) for receiving messages
// /// and HTTP POST for sending messages.
// pub struct SseTransport {
//     sse_url: String,
//     http_client: HttpClient,
//     post_endpoint: Arc<Mutex<Option<String>>>,
//     sse_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
//     pending_requests: Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<JsonRpcMessage, Error>>>>>,
// }

// impl SseTransport {
//     /// Create a new SSE transport with the given SSE endpoint URL
//     pub fn new<S: Into<String>>(sse_url: S) -> Self {
//         Self {
//             sse_url: sse_url.into(),
//             http_client: HttpClient::new(),
//             post_endpoint: Arc::new(Mutex::new(None)),
//             sse_handle: Arc::new(Mutex::new(None)),
//             pending_requests: Arc::new(Mutex::new(std::collections::HashMap::new())),
//         }
//     }

//     async fn handle_message(
//         message: JsonRpcMessage,
//         pending_requests: Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<JsonRpcMessage, Error>>>>>,
//     ) {
//         if let JsonRpcMessage::Response(response) = &message {
//             if let Some(id) = &response.id {
//                 if let Some(tx) = pending_requests.lock().await.remove(&id.to_string()) {
//                     let _ = tx.send(Ok(message));
//                 }
//             }
//         }
//     }

//     async fn process_messages(
//         mut message_rx: mpsc::Receiver<TransportMessage>,
//         http_client: HttpClient,
//         post_endpoint: Arc<Mutex<Option<String>>>,
//         sse_url: String,
//         pending_requests: Arc<Mutex<std::collections::HashMap<String, oneshot::Sender<Result<JsonRpcMessage, Error>>>>>,
//     ) {
//         // Set up SSE client
//         let mut client = match eventsource_client::ClientBuilder::for_url(&sse_url) {
//             Ok(builder) => builder.build(),
//             Err(e) => {
//                 eprintln!("Failed to create SSE client: {}", e);
//                 return;
//             }
//         };

//         // Wait for endpoint event to get POST URL
//         while let Some(event) = client.next().await {
//             match event {
//                 Ok(SSE::Event(event)) if event.event_type == "endpoint" => {
//                     *post_endpoint.lock().await = Some(event.data);
//                     break;
//                 }
//                 Ok(_) => continue,
//                 Err(e) => {
//                     eprintln!("SSE connection error: {}", e);
//                     return;
//                 }
//             }
//         }

//         // Spawn SSE message handler
//         let pending_clone = pending_requests.clone();
//         let mut client_clone = client.clone();
//         let sse_handle = tokio::spawn(async move {
//             while let Some(event) = client_clone.next().await {
//                 match event {
//                     Ok(SSE::Event(event)) if event.event_type == "message" => {
//                         if let Ok(message) = serde_json::from_str::<JsonRpcMessage>(&event.data) {
//                             Self::handle_message(message, pending_clone.clone()).await;
//                         }
//                     }
//                     Ok(_) => continue,
//                     Err(e) => {
//                         eprintln!("SSE message error: {}", e);
//                         break;
//                     }
//                 }
//             }
//         });

//         // Process outgoing messages
//         while let Some(transport_msg) = message_rx.recv().await {
//             let post_url = match post_endpoint.lock().await.as_ref() {
//                 Some(url) => url.clone(),
//                 None => {
//                     eprintln!("No POST endpoint available");
//                     continue;
//                 }
//             };

//             // Store response channel if this is a request
//             if let Some(response_tx) = transport_msg.response_tx {
//                 if let JsonRpcMessage::Request(request) = &transport_msg.message {
//                     if let Some(id) = &request.id {
//                         pending_requests.lock().await.insert(id.to_string(), response_tx);
//                     }
//                 }
//             }

//             // Send message via HTTP POST
//             let message_str = match serde_json::to_string(&transport_msg.message) {
//                 Ok(s) => s,
//                 Err(e) => {
//                     eprintln!("Failed to serialize message: {}", e);
//                     continue;
//                 }
//             };

//             if let Err(e) = http_client
//                 .post(&post_url)
//                 .header("Content-Type", "application/json")
//                 .body(message_str)
//                 .send()
//                 .await
//             {
//                 eprintln!("Failed to send message: {}", e);
//             }
//         }

//         // Clean up
//         sse_handle.abort();
//     }
// }

// #[async_trait]
// impl Transport for SseTransport {
//     async fn start(&self) -> Result<mpsc::Sender<TransportMessage>, Error> {
//         let (message_tx, message_rx) = mpsc::channel(32);

//         let http_client = self.http_client.clone();
//         let post_endpoint = self.post_endpoint.clone();
//         let sse_url = self.sse_url.clone();
//         let pending_requests = self.pending_requests.clone();

//         let handle = tokio::spawn(Self::process_messages(
//             message_rx,
//             http_client,
//             post_endpoint,
//             sse_url,
//             pending_requests,
//         ));

//         *self.sse_handle.lock().await = Some(handle);

//         Ok(message_tx)
//     }

//     async fn close(&self) -> Result<(), Error> {
//         // Abort the SSE handler task
//         if let Some(handle) = self.sse_handle.lock().await.take() {
//             handle.abort();
//         }

//         // Clear any pending requests
//         self.pending_requests.lock().await.clear();

//         Ok(())
//     }
// }

// impl Drop for SseTransport {
//     fn drop(&mut self) {
//         // Create a new runtime for cleanup if needed
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         let _ = rt.block_on(self.close());
//     }
// }
