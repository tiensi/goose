use futures::future::{BoxFuture, FutureExt};
use std::task::{Context, Poll};
use tokio::time::{sleep, Duration};
use tower::{Service, ServiceBuilder};

// Define a simple service that takes some time to respond
struct SlowService;

impl Service<String> for SlowService {
    type Response = String;
    type Error = &'static str;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: String) -> Self::Future {
        println!("Processing request: {}", req);

        // Use an async block to create the future
        async move {
            // Simulate a slow response
            sleep(Duration::from_secs(3)).await;
            Ok(format!("Processed: {}", req))
        }
        .boxed() // Convert the future into a BoxFuture
    }
}

#[tokio::main]
async fn main() {
    // Create the base service
    let service = SlowService;

    // Wrap the service with a timeout layer
    let timeout_service = ServiceBuilder::new()
        .timeout(Duration::from_secs(1))
        .service(service);

    // Use the timeout-wrapped service
    let mut svc = timeout_service;

    match svc.call("Hello Tower!".to_string()).await {
        Ok(response) => println!("Response: {}", response),
        Err(err) => {
            if let Some(_elapsed) = err.downcast_ref::<tower::timeout::error::Elapsed>() {
                println!("Error: Timed out");
            } else {
                println!("Error: {:?}", err);
            }
        }
    }
}
