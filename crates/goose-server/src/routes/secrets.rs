use axum::{
    extract::State,
    routing::post,
    routing::delete,
    Json, Router,
};
use once_cell::sync::Lazy;  // TODO: investigate if we need
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::state::AppState;
use http::{HeaderMap, StatusCode};
use goose::key_manager::{save_to_keyring, get_keyring_secret, delete_from_keyring, KeyRetrievalStrategy};

#[derive(Serialize)]
struct SecretResponse {
    error: bool,
}

#[derive(Deserialize)]
struct SecretRequest {
    key: String,
    value: String,
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

    match save_to_keyring(&request.key, &request.value) {
        Ok(_) => Ok(Json(SecretResponse { error: false })),
        Err(_) => Ok(Json(SecretResponse { error: true })),
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderSecretRequest {
    pub providers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretStatus {
    pub is_set: bool,
    pub location: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub supported: bool,
    pub secret_status: HashMap<String, SecretStatus>,
}

static PROVIDER_ENV_REQUIREMENTS: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let contents = include_str!("providers_and_keys.json");
    serde_json::from_str(contents).expect("Failed to parse providers_and_keys.json")
});


// Helper function to check if a key is set somewhere
fn check_key_status(key: &str) -> (bool, Option<String>) {
    if let Ok(_value) = std::env::var(key) {
        (true, Some("env".to_string()))
    } else if let Ok(_) = get_keyring_secret(key, KeyRetrievalStrategy::KeyringOnly) {
        (true, Some("keyring".to_string()))
    } else {
        (false, None)
    }
}

async fn check_provider_secrets(
    Json(request): Json<ProviderSecretRequest>,
) -> Result<Json<HashMap<String, ProviderStatus>>, StatusCode> {
    let mut response = HashMap::new();

    for provider_name in request.providers {
        if let Some(keys) = PROVIDER_ENV_REQUIREMENTS.get(&provider_name) {
            let mut secret_status = HashMap::new();

            for key in keys {
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
            response.insert(provider_name, ProviderStatus {
                supported: false,
                secret_status: HashMap::new(),
            });
        }
    }

    Ok(Json(response))
}

#[derive(Deserialize)]
struct DeleteSecretRequest {
    key: String,
}

async fn delete_secret(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DeleteSecretRequest>,
) -> Result<StatusCode, StatusCode> {
    // Verify secret key
    let secret_key = headers
        .get("X-Secret-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if secret_key != state.secret_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Attempt to delete the key
    match delete_from_keyring(&request.key) {
        Ok(_) => Ok(StatusCode::NO_CONTENT),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/secrets/providers", post(check_provider_secrets))
        .route("/secrets/store", post(store_secret))
        .route("/secrets/delete", delete(delete_secret))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unsupported_provider() {
        // Setup
        let request = ProviderSecretRequest {
            providers: vec!["unsupported_provider".to_string()],
        };

        // Execute
        let result = check_provider_secrets(Json(request)).await;

        // Assert
        assert!(result.is_ok());
        let Json(response) = result.unwrap();

        let provider_status = response.get("unsupported_provider").expect("Provider should exist");
        assert!(!provider_status.supported);
        assert!(provider_status.secret_status.is_empty());
    }

}