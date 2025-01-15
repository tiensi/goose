use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Authentication error: {0}")]
    Unauthorized(String),

    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Server error: {0}")]
    ServerError(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("JSON parse error: {0}")]
    JsonParseError(String),
}
