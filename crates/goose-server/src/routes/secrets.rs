use axum::{
    routing::{get, post},
    Json, Router,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::state::AppState;
use goose::key_manager::{get_keyring_secret, KeyRetrievalStrategy};

// Define the types needed for the API
#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderRequest {
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

// Define the provider requirements
static PROVIDER_ENV_REQUIREMENTS: Lazy<HashMap<String, Vec<String>>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "test_provider".to_string(),
        vec!["TEST_API_KEY".to_string(), "TEST_SECRET".to_string()]
    );
    m
});

// Helper function to check key status
fn check_key_status(key: &str) -> (bool, Option<String>) {
    if let Ok(value) = std::env::var(key) {
        (true, Some("env".to_string()))
    } else if let Ok(_) = get_keyring_secret(key, KeyRetrievalStrategy::KeyringOnly) {
        (true, Some("keyring".to_string()))
    } else {
        (false, None)
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

    Json(response)
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        .route("/provider", post(check_provider_secrets))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Cleanup
        std::env::remove_var("TEST_API_KEY");
        std::env::remove_var("TEST_SECRET");
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