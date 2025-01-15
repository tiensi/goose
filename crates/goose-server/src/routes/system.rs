use crate::state::AppState;
use axum::{extract::State, routing::post, Json, Router};
use goose::agents::SystemConfig;
use http::{HeaderMap, StatusCode};
use serde::Serialize;

#[derive(Serialize)]
struct SystemResponse {
    error: bool,
}

async fn add_system(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SystemConfig>,
) -> Result<Json<SystemResponse>, StatusCode> {
    // Verify secret key
    let secret_key = headers
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if secret_key != state.secret_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // TODO: fix when `goosed agent` starts a MCP server, it doesn't write logs to ~/.config/goose/logs 
    // Instead, the logs get written to the current directory, example: logs/mcp-server.log.2025-01-15
    // We would need the system name to write to the correct log file
    let mut agent = state.agent.lock().await;
    let response = agent.add_system(request).await;

    Ok(Json(SystemResponse {
        error: response.is_err(),
    }))
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/systems/add", post(add_system))
        .with_state(state)
}
