use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Rate limit exceeded. Retry after {retry_after} seconds")]
    RateLimitExceeded { retry_after: u64 },

    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },

    #[error("Resource not found: {resource}")]
    NotFound { resource: String },

    #[error("Invalid request: {message}")]
    BadRequest { message: String },

    #[error("Server error: {status} - {message}")]
    ServerError { status: u16, message: String },

    #[error("Invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Request timeout after {attempts} attempts")]
    Timeout { attempts: usize },

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),
}

impl ApiError {
    pub fn is_retryable(&self) -> bool {
        match self {
            ApiError::RateLimitExceeded { .. } => true,
            ApiError::ServerError { status, .. } if *status >= 500 => true,
            ApiError::Timeout { .. } => true,
            _ => false,
        }
    }

    pub fn suggestion(&self) -> Option<&str> {
        match self {
            ApiError::AuthenticationFailed { .. } => {
                Some("Verify your API token using: atlassiancli auth test")
            }
            ApiError::RateLimitExceeded { .. } => {
                Some("Consider reducing request frequency or use bulk operations")
            }
            ApiError::NotFound { .. } => Some("Check if the resource ID is correct"),
            ApiError::BadRequest { .. } => Some("Review the request parameters"),
            ApiError::Timeout { .. } => Some("Check your network connection or try again later"),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;
