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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use std::collections::HashMap;
    use lazy_static::lazy_static;

    // Mock PROVIDER_ENV_REQUIREMENTS for testing
    lazy_static! {
        static ref TEST_PROVIDER_REQUIREMENTS: HashMap<String, Vec<String>> = {
            let mut m = HashMap::new();
            m.insert(
                "test_provider".to_string(),
                vec!["TEST_API_KEY".to_string(), "TEST_SECRET".to_string()]
            );
            m
        };
    }

    #[tokio::test]
    async fn test_supported_provider_with_set_keys() {
        // Setup
        let request = ProviderRequest {
            providers: vec!["test_provider".to_string()]
        };

        // Set environment variables for testing
        std::env::set_var("TEST_API_KEY", "dummy_value");
        std::env::set_var("TEST_SECRET", "dummy_secret");

        // Execute
        let Json(response) = check_provider_secrets(Json(request)).await;

        // Assert
        let provider_status = response.get("test_provider").expect("Provider should exist");
        assert!(provider_status.supported);
        
        let secret_status = &provider_status.secret_status;
        assert!(secret_status.get("TEST_API_KEY").unwrap().is_set);
        assert!(secret_status.get("TEST_SECRET").unwrap().is_set);
    }

    #[tokio::test]
    async fn test_unsupported_provider() {
        // Setup
        let request = ProviderRequest {
            providers: vec!["unsupported_provider".to_string()]
        };

        // Execute
        let Json(response) = check_provider_secrets(Json(request)).await;

        // Assert
        let provider_status = response.get("unsupported_provider").expect("Provider should exist");
        assert!(!provider_status.supported);
        assert!(provider_status.secret_status.is_empty());
    }

    #[tokio::test]
    async fn test_supported_provider_with_missing_keys() {
        // Setup
        let request = ProviderRequest {
            providers: vec!["test_provider".to_string()]
        };

        // Remove environment variables if they exist
        std::env::remove_var("TEST_API_KEY");
        std::env::remove_var("TEST_SECRET");

        // Execute
        let Json(response) = check_provider_secrets(Json(request)).await;

        // Assert
        let provider_status = response.get("test_provider").expect("Provider should exist");
        assert!(provider_status.supported);
        
        let secret_status = &provider_status.secret_status;
        assert!(!secret_status.get("TEST_API_KEY").unwrap().is_set);
        assert!(!secret_status.get("TEST_SECRET").unwrap().is_set);
    }

    #[tokio::test]
    async fn test_multiple_providers() {
        // Setup
        let request = ProviderRequest {
            providers: vec![
                "test_provider".to_string(),
                "unsupported_provider".to_string()
            ]
        };

        // Execute
        let Json(response) = check_provider_secrets(Json(request)).await;

        // Assert
        assert_eq!(response.len(), 2);
        
        let supported_status = response.get("test_provider").expect("Supported provider should exist");
        assert!(supported_status.supported);
        
        let unsupported_status = response.get("unsupported_provider").expect("Unsupported provider should exist");
        assert!(!unsupported_status.supported);
    }
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/secrets/store", post(store_secret))
        .route("/secrets/provider", get(list_provider_secrets))
}