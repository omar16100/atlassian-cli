use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Rate limit exceeded. Retry after {retry_after} seconds")]
    RateLimitExceeded { retry_after: u64 },

    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },

    #[error("Access forbidden: {message}")]
    Forbidden { message: String },

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

    pub fn suggestion(&self) -> Option<String> {
        match self {
            ApiError::AuthenticationFailed { .. } => {
                Some("Verify tokens with: atlassian-cli auth list\nTest auth with: atlassian-cli auth test [--bitbucket]".to_string())
            }
            ApiError::Forbidden { message } => {
                let base = "Verify tokens with: atlassian-cli auth list\nTest auth with: atlassian-cli auth test [--bitbucket]".to_string();
                let lower = message.to_lowercase();
                if lower.contains("scope") || lower.contains("privilege") || lower.contains("permission") {
                    Some(format!("{base}\nAdd missing scopes at: https://bitbucket.org/account/settings/app-passwords/"))
                } else {
                    Some(base)
                }
            }
            ApiError::RateLimitExceeded { .. } => {
                Some("Consider reducing request frequency or use bulk operations".to_string())
            }
            ApiError::NotFound { .. } => Some("Check if the resource ID is correct".to_string()),
            ApiError::BadRequest { message } => {
                if message.contains("Version number must be 1") {
                    Some("This is a draft page. Use 'confluence page publish' to publish for the first time".to_string())
                } else if message.to_lowercase().contains("version") {
                    Some("Version conflict detected. The content may have been modified. Fetch latest and retry".to_string())
                } else {
                    Some("Review the request parameters".to_string())
                }
            }
            ApiError::Timeout { .. } => Some("Check your network connection or try again later".to_string()),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbidden_has_suggestion() {
        let err = ApiError::Forbidden {
            message: "no access".to_string(),
        };
        assert!(err.suggestion().is_some());
        assert!(err.suggestion().unwrap().contains("auth test"));
    }

    #[test]
    fn forbidden_is_not_retryable() {
        let err = ApiError::Forbidden {
            message: "no access".to_string(),
        };
        assert!(!err.is_retryable());
    }

    #[test]
    fn authentication_failed_has_suggestion() {
        let err = ApiError::AuthenticationFailed {
            message: "expired".to_string(),
        };
        assert!(err.suggestion().is_some());
        assert!(err.suggestion().unwrap().contains("auth test"));
    }

    #[test]
    fn forbidden_with_scope_message_includes_app_passwords_link() {
        let err = ApiError::Forbidden {
            message: "Your credentials lack the required scope.".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("auth test"));
        assert!(hint.contains("app-passwords"));
    }

    #[test]
    fn forbidden_without_scope_omits_app_passwords_link() {
        let err = ApiError::Forbidden {
            message: "no access".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("auth test"));
        assert!(!hint.contains("app-passwords"));
    }

    #[test]
    fn forbidden_with_permission_message_includes_link() {
        let err = ApiError::Forbidden {
            message: "Insufficient Permission to access this resource".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("app-passwords"));
    }
}
