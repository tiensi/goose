use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Registry};
use tracing::dispatcher::set_global_default;

use crate::tracing::{langfuse_layer, observation_layer::{BatchManager, ObservationLayer, SpanTracker}};

struct ConsoleLogger {
    batch: Vec<Value>,
}

impl ConsoleLogger {
    fn new() -> Self {
        Self {
            batch: Vec::new(),
        }
    }
}

impl BatchManager for ConsoleLogger {
    fn add_event(&mut self, _event_type: &str, body: Value) {
        self.batch.push(body);
    }

    fn send(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.batch.clear();
        Ok(())
    }
}

fn get_log_directory() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let base_log_dir = PathBuf::from(home).join(".config").join("goose").join("logs");
    
    // Create date-based subdirectory
    let now = chrono::Local::now();
    let date_dir = base_log_dir.join(now.format("%Y-%m-%d").to_string());
    
    // Ensure log directory exists
    fs::create_dir_all(&date_dir).context("Failed to create log directory")?;
    
    Ok(date_dir)
}

fn create_observation_layer() -> ObservationLayer {
    let batch_manager = Arc::new(Mutex::new(ConsoleLogger::new()));
    ObservationLayer {
        batch_manager,
        span_tracker: Arc::new(Mutex::new(SpanTracker::new())),
    }
}

pub fn setup_logging() -> Result<()> {
    // Set up file appender for goose module logs
    let log_dir = get_log_directory()?;
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S%.3f").to_string();
    
    // Create non-rolling file appender
    let file_appender = tracing_appender::rolling::RollingFileAppender::new(
        Rotation::NEVER,
        log_dir,
        &format!("{}.log", timestamp),
    );

    // Create JSON file logging layer
    let file_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_writer(file_appender)
        .with_ansi(false)
        .with_file(true);

    // Update filter to include debug level
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("goose=debug"));

    // Build the base subscriber
    let subscriber = Registry::default()
        .with(file_layer)
        .with(filter)
        .with(create_observation_layer());

    // Set up the dispatcher
    let dispatcher = if let Some(langfuse) = langfuse_layer::create_langfuse_observer() {
        subscriber.with(langfuse).into()
    } else {
        subscriber.into()
    };

    // Set the subscriber as the default
    set_global_default(dispatcher)
        .map_err(|e| anyhow::anyhow!("Failed to set global subscriber: {}", e))?;

    Ok(())
}
