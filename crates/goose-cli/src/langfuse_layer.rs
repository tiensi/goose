use chrono::Utc;
use serde_json::{json, Value};
use tracing::{span, Id, Subscriber, Level, Metadata, Event};
use tracing_subscriber::Layer;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;
use std::env;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tracing_subscriber::registry::LookupSpan;
use tracing::field::{Field, Visit};
use reqwest::{Client, StatusCode};
use tracing_subscriber::layer::Context;

const DEFAULT_LANGFUSE_URL: &str = "http://localhost:3000";

#[derive(Debug, Serialize, Deserialize)]
struct IngestionResponse {
    successes: Vec<IngestionSuccess>,
    errors: Vec<IngestionError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IngestionSuccess {
    id: String,
    status: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct IngestionError {
    id: String,
    status: i32,
    message: Option<String>,
    error: Option<Value>,
}

#[derive(Debug)]
struct SpanData {
    langfuse_id: String,
    observation_id: String,
    name: String,
    start_time: String,
    target: String,
    level: String,
    metadata: serde_json::Map<String, Value>,
    parent_span: Option<u64>,
}

#[derive(Debug)]
struct JsonVisitor {
    recorded_fields: serde_json::Map<String, Value>,
}

impl JsonVisitor {
    fn new() -> Self {
        Self {
            recorded_fields: serde_json::Map::new(),
        }
    }

    fn insert_value(&mut self, field: &Field, value: Value) {
        self.recorded_fields.insert(field.name().to_string(), value);
    }
}

macro_rules! record_field {
    ($fn_name:ident, $type:ty) => {
        fn $fn_name(&mut self, field: &Field, value: $type) {
            self.insert_value(field, Value::from(value));
        }
    };
}

impl Visit for JsonVisitor {
    record_field!(record_i64, i64);
    record_field!(record_u64, u64);
    record_field!(record_bool, bool);
    record_field!(record_str, &str);

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.insert_value(field, Value::String(format!("{:?}", value)));
    }
}

#[derive(Debug, Clone)]
pub struct LangfuseLayer {
    state: Arc<Mutex<LangfuseState>>,
    client: Client,
    base_url: String,
}

#[derive(Debug)]
struct LangfuseState {
    public_key: String,
    secret_key: String,
    batch: Vec<Value>,
    current_trace_id: Option<String>,
    current_observation_id: Option<String>,
    active_spans: HashMap<u64, (String, String)>, // (langfuse_id, observation_id)
    span_hierarchy: HashMap<u64, u64>, // child span -> parent span
}

impl LangfuseLayer {
    pub fn new(public_key: String, secret_key: String) -> Self {
        let langfuse_url = env::var("LANGFUSE_URL")
            .unwrap_or_else(|_| DEFAULT_LANGFUSE_URL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        let layer = Self {
            state: Arc::new(Mutex::new(LangfuseState {
                public_key,
                secret_key,
                batch: Vec::new(),
                current_trace_id: None,
                current_observation_id: None,
                active_spans: HashMap::new(),
                span_hierarchy: HashMap::new(),
            })),
            client,
            base_url: langfuse_url,
        };

        layer.spawn_batch_sender();
        layer
    }

    fn spawn_batch_sender(&self) {
        const BATCH_INTERVAL: Duration = Duration::from_secs(5);
        let layer = self.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(BATCH_INTERVAL).await;
                if let Err(e) = layer.send_batch().await {
                    tracing::error!(
                        error.msg = %e,
                        error.type = %std::any::type_name_of_val(&e),
                        "Failed to send batch to Langfuse"
                    );
                }
            }
        });
    }

    fn map_level(level: &Level) -> &'static str {
        match *level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARNING",
            Level::INFO => "DEFAULT",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "DEBUG",
        }
    }

    async fn send_batch(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.lock().await;
        if state.batch.is_empty() {
            return Ok(());
        }

        let payload = json!({ "batch": state.batch });
        let url = format!("{}/api/public/ingestion", self.base_url);

        let response = self.client
            .post(&url)
            .basic_auth(&state.public_key, Some(&state.secret_key))
            .json(&payload)
            .send()
            .await?;
        
        match response.status() {
            status if status.is_success() => {
                let response_body: IngestionResponse = response.json().await?;
                
                for error in &response_body.errors {
                    tracing::error!(
                        id = %error.id,
                        status = error.status,
                        message = error.message.as_deref().unwrap_or("No message"),
                        error = ?error.error,
                        "Partial failure in batch ingestion"
                    );
                }
        
                if !response_body.successes.is_empty() {
                    state.batch.clear();
                }
        
                if response_body.successes.is_empty() && !response_body.errors.is_empty() {
                    Err("Langfuse ingestion failed for all items".into())
                } else {
                    Ok(())
                }
            }
            status @ (StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | 
                     StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED) => {
                let err_text = response.text().await.unwrap_or_default();
                tracing::error!(status = %status, error = %err_text, "Langfuse API error");
                Err(format!("Langfuse API error: {}: {}", status, err_text).into())
            }
            status => {
                let err_text = response.text().await.unwrap_or_default();
                tracing::error!(status = %status, error = %err_text, "Unexpected Langfuse API error");
                Err(format!("Unexpected status code: {}: {}", status, err_text).into())
            }
        }
    }

    async fn create_trace_if_needed(&self) -> String {
        let mut state = self.state.lock().await;
        if let Some(ref trace_id) = state.current_trace_id {
            return trace_id.clone();
        }

        let trace_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let trace_event = json!({
            "id": Uuid::new_v4().to_string(),
            "timestamp": now.to_rfc3339(),
            "type": "trace-create",
            "body": {
                "id": trace_id,
                "name": now.timestamp().to_string(),
                "timestamp": now.to_rfc3339(),
                "input": {},
                "metadata": {},
                "tags": [],
                "public": false
            }
        });

        state.batch.push(trace_event);
        state.current_trace_id = Some(trace_id.clone());
        trace_id
    }

    async fn create_event(&self, event_type: &str, body: Value) {
        let mut state = self.state.lock().await;
        let event = json!({
            "id": Uuid::new_v4().to_string(),
            "timestamp": Utc::now().to_rfc3339(),
            "type": event_type,
            "body": body
        });
        state.batch.push(event);
    }

    async fn manage_span(&self, span_id: u64, langfuse_id: String, observation_id: String, parent_span: Option<u64>) {
        let mut state = self.state.lock().await;
        if let Some(parent_id) = parent_span {
            state.span_hierarchy.insert(span_id, parent_id);
        }
        state.active_spans.insert(span_id, (langfuse_id, observation_id));
    }

    async fn handle_span_enter(&self, span_id: u64) {
        let span = self.is_active_span(span_id).await;
        let observation_id = span.map(|(_, id)| id);
        self.set_current_observation_id(observation_id).await;
    }

    async fn handle_span_exit(&self, parent_span_id: Option<u64>) {
        let observation_id = match parent_span_id {
            Some(parent_id) => {
                let span = self.is_active_span(parent_id).await;
                span.map(|(_, id)| id)
            }
            None => None
        };
        self.set_current_observation_id(observation_id).await;
    }

    async fn set_current_observation_id(&self, observation_id: Option<String>) {
        let mut state = self.state.lock().await;
        state.current_observation_id = observation_id;
    }

    async fn is_active_span(&self, span_id: u64) -> Option<(String, String)> {
        let state = self.state.lock().await;
        state.active_spans.get(&span_id).cloned()
    }

    async fn remove_active_span(&self, span_id: u64) -> Option<(String, String)> {
        let mut state = self.state.lock().await;
        state.active_spans.remove(&span_id)
    }


    async fn handle_new_span(&self, span_id: u64, span_data: SpanData) {
        // First store the span in our active spans so it's available for lookups
        self.manage_span(
            span_id,
            span_data.langfuse_id.clone(),
            span_data.observation_id.clone(),
            span_data.parent_span
        ).await;

        // Now we can safely look up the parent's observation ID
        let parent_observation_id = if let Some(parent_span_id) = span_data.parent_span {
            let state = self.state.lock().await;
            let parent_obs_id = state.active_spans.get(&parent_span_id)
                .map(|(_, observation_id)| {
                    observation_id.clone()
                });
            parent_obs_id
        } else {
            None
        };

        // Create the observation with the correct parent ID
        let trace_id = self.create_trace_if_needed().await;
        
        // Create observation event with proper parent ID
        let observation_event = json!({
            "id": span_data.observation_id,
            "traceId": trace_id,
            "type": "SPAN",
            "name": span_data.name,
            "startTime": span_data.start_time,
            "parentObservationId": parent_observation_id,
            "metadata": span_data.metadata,
            "level": span_data.level
        });
        
        self.create_event("observation-create", observation_event).await;
    }

    async fn handle_span_close(&self, span_id: u64) {
        if let Some((_langfuse_id, observation_id)) = self.remove_active_span(span_id).await {
            let trace_id = self.create_trace_if_needed().await;
            self.create_event("observation-update", json!({
                "id": observation_id,
                "type": "SPAN",
                "traceId": trace_id,
                "endTime": Utc::now().to_rfc3339()
            })).await;
        }
    }

    async fn handle_event(&self, current_span_id: Option<u64>, metadata: serde_json::Map<String, Value>) {
        let trace_id = self.create_trace_if_needed().await;

        let observation_id = if let Some(span_id) = current_span_id {
            self.is_active_span(span_id).await
                .map(|(_, observation_id)| observation_id)
        } else {
            None
        };

        if let Some(kind) = metadata.get("kind").and_then(|v| v.as_str()) {
            match kind {
                "input" | "output" => {
                    if let Some(val) = metadata.get(kind).and_then(|v| v.as_str()) {
                        let update_event = json!({
                            "id": Uuid::new_v4().to_string(),
                            "timestamp": Utc::now().to_rfc3339(),
                            "type": "span-update",
                            "body": {
                                "traceId": trace_id,
                                "id": observation_id.unwrap(),
                                kind: serde_json::from_str(val).unwrap_or(json!({}))
                            }
                        });
                        let mut state = self.state.lock().await;
                        state.batch.push(update_event);
                    }
                    return;
                }
                
                _ => {}
            }
        }
    }
}

impl<S> Layer<S> for LangfuseLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &span::Attributes<'_>,
        id: &span::Id,
        ctx: Context<'_, S>,
    ) {
        let span_id = id.into_u64();
        
        // Instead of lookup_current, use the scope to find parent
        let parent_span = ctx.span_scope(id)
            .and_then(|scope| {
                scope.skip(1).next()  // Skip the current span, get the next one (parent)
                    .map(|parent| parent.id().into_u64())
            });
    
        let mut visitor = JsonVisitor::new();
        attrs.record(&mut visitor);
    
        let span_data = SpanData {
            langfuse_id: Uuid::new_v4().to_string(),
            observation_id: Uuid::new_v4().to_string(),
            name: attrs.metadata().name().to_string(),
            start_time: Utc::now().to_rfc3339(),
            target: attrs.metadata().target().to_string(),
            level: Self::map_level(attrs.metadata().level()).to_owned(),
            metadata: visitor.recorded_fields,
            parent_span,
        };
    
        let layer = self.clone();
        tokio::spawn(async move {
            layer.handle_new_span(span_id, span_data).await;
        });
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        let span_id = id.into_u64();
        let layer = self.clone();
        
        tokio::spawn(async move {
            layer.handle_span_close(span_id).await;
        });
    }

    fn enabled(&self, metadata: &Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        metadata.target().starts_with("goose::")
    }

    fn on_record(&self, span: &Id, values: &span::Record<'_>, _ctx: Context<'_, S>) {
        let span_id = span.into_u64();
        let mut visitor = JsonVisitor::new();
        values.record(&mut visitor);
        let metadata = visitor.recorded_fields;

        if !metadata.is_empty() {
            let layer = self.clone();
            tokio::spawn(async move {
                if let Some((_, observation_id)) = layer.is_active_span(span_id).await {
                    let trace_id = layer.create_trace_if_needed().await;
                    
                    // Create a flattened metadata object with direct key-value pairs
                    let mut flattened_metadata = serde_json::Map::new();
                    for (key, value) in metadata {
                        match value {
                            Value::String(s) => {
                                flattened_metadata.insert(key, json!(s));
                            },
                            Value::Object(mut obj) => {
                                // Handle nested objects by flattening them
                                if let Some(text) = obj.remove("text") {
                                    flattened_metadata.insert(key, text);
                                } else {
                                    flattened_metadata.insert(key, json!(obj));
                                }
                            },
                            _ => {
                                flattened_metadata.insert(key, value);
                            }
                        }
                    }
                                        
                    // Create update event with metadata
                    layer.create_event("observation-update", json!({
                        "id": observation_id,
                        "traceId": trace_id,
                        "metadata": flattened_metadata,
                        "type": "SPAN"
                    })).await;
                } else {
                    println!("No active span found for ID {} when recording fields", span_id);
                }
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = JsonVisitor::new();
        event.record(&mut visitor);
        let metadata = visitor.recorded_fields;
        let layer = self.clone();
        let current_span_id = ctx.lookup_current().map(|span| span.id().into_u64());
        
        tokio::spawn(async move {
            layer.handle_event(current_span_id, metadata).await;
        });
    }

    fn on_enter(&self, span: &Id, _ctx: Context<'_, S>) {
        let span_id = span.into_u64();
        let layer = self.clone();
        
        tokio::spawn(async move {
            layer.handle_span_enter(span_id).await;
        });
    }

    fn on_exit(&self, _span: &Id, ctx: Context<'_, S>) {
        let layer = self.clone();
        let parent_span_id = ctx.lookup_current()
            .and_then(|span| span.parent())
            .map(|s| s.id().into_u64());
        
        tokio::spawn(async move {
            layer.handle_span_exit(parent_span_id).await;
        });
    }
}

pub fn create_langfuse_layer() -> Option<LangfuseLayer> {
    let public_key = env::var("LANGFUSE_PUBLIC_KEY")
        .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_PUBLIC_KEY"))
        .unwrap_or_else(|_| "publickey-local".to_string());
    
    let secret_key = env::var("LANGFUSE_SECRET_KEY")
        .or_else(|_| env::var("LANGFUSE_INIT_PROJECT_SECRET_KEY"))
        .unwrap_or_else(|_| "secretkey-local".to_string());

    Some(LangfuseLayer::new(public_key, secret_key))
}