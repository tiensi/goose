use crate::state::AppState;
use axum::{extract::State, routing::{post, get}, Json, Router};
use goose::key_manager::save_to_keyring;
use http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use std::{env, collections::HashMap};
use once_cell::sync::Lazy;

#[derive(Serialize)]
struct SecretResponse {
    error: bool,
}

#[derive(Deserialize)]
struct SecretRequest {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct ProviderStatus {
    supported: bool,
    secret_status: HashMap<String, SecretStatus>, // Map of API key names to their statuses
}

#[derive(Serialize)]
struct SecretStatus {
    is_set: bool,          // True if the key is set
    location: Option<String>, // "env", "keychain", or None
}

#[derive(Serialize)]
struct SecretSource {
    key: String,
    source: String,  // "env", "keyring", or "none"
    is_set: bool,    // true if the secret exists, false otherwise
}

#[derive(Serialize)]
struct SecretsListResponse {
    secrets: Vec<SecretSource>,
}

#[derive(Debug, Serialize)]
struct KeyStatus {
    set: bool,
    location: Option<String>,  // "env", "keyring", or null
    supported: bool,
}

#[derive(Debug, Deserialize)]
struct ProviderRequest {
    providers: Vec<String>,
}

static PROVIDER_ENV_REQUIREMENTS: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let contents = include_str!("providers_and_keys.json");
    serde_json::from_str(contents).expect("Failed to parse providers_and_keys.json")
});

fn get_supported_secrets() -> Vec<&'static str> {
    PROVIDER_KEYS.values()
        .flat_map(|keys| keys.iter())
        .map(|s| s.as_str())
        .collect()
}



/// Check the status of a key, including whether it's set and its location.
pub fn check_key_status(key_name: &str) -> (bool, Option<String>) {
    // Current hierarchy: prioritize environment variables over keyring
    if let Ok(_) = env::var(key_name) {
        return (true, Some("env".to_string())); // Found in environment
    }

    if let Ok(_) = get_keyring_secret(key_name, KeyRetrievalStrategy::KeyringOnly) {
        return (true, Some("keychain".to_string())); // Found in keyring
    }

    (false, None) // Not found in either source
}


async fn store_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SecretRequest>,
) -> Result<Json<SecretResponse>, StatusCode> {
    // Verify secret key
    let secret_key = headers
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if secret_key != state.secret_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Verify this is a supported secret key
    let supported_secrets = get_supported_secrets();
    if !supported_secrets.contains(&request.key.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    match save_to_keyring(&request.key, &request.value) {
        Ok(_) => Ok(Json(SecretResponse { error: false })),
        Err(_) => Ok(Json(SecretResponse { error: true })),
    }
}

async fn check_provider_secrets(
    Json(request): Json<ProviderRequest>,
) -> Json<HashMap<String, ProviderStatus>> {
    let mut response = HashMap::new();

    for provider_name in request.providers {
        if let Some(keys) = PROVIDER_ENV_REQUIREMENTS.get(&provider_name) {
            let mut secret_status = HashMap::new();

            for key in keys {
                // Assume check_key_status returns (bool, Option<String>)
                let (key_set, key_location) = check_key_status(key);

                secret_status.insert(
                    key.to_string(),
                    SecretStatus {
                        is_set: key_set,
                        location: key_location,
                    },
                );
            }

            response.insert(provider_name, ProviderStatus {
                supported: true,
                secret_status,
            });
        } else {
            // Provider not supported
            response.insert(provider_name, ProviderStatus {
                supported: false,
                secret_status: HashMap::new(),
            });
        }
    }

    Json(response)
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/secrets/store", post(store_secret))
        .route("/secrets/provider", get(list_provider_secrets))