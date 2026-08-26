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

    #[error("API endpoint removed: {message}")]
    EndpointGone { message: String },

    #[error("Invalid response format: {0}")]
    InvalidResponse(String),
}

impl ApiError {
    pub fn is_retryable(&self) -> bool {
        match self {
            ApiError::RateLimitExceeded { .. } => true,
            ApiError::ServerError { status, .. } if *status >= 500 => true,
            ApiError::Timeout { .. } => true,
            ApiError::EndpointGone { .. } => false,
            _ => false,
        }
    }

    pub fn suggestion(&self) -> Option<String> {
        match self {
            ApiError::AuthenticationFailed { message } => {
                let base = "Verify tokens with: atlassian-cli auth list\nTest auth with: atlassian-cli auth test [--bitbucket]".to_string();
                // A scope mismatch is not a bad token: re-issuing the same
                // token changes nothing, so point at the scopes instead.
                if message.to_lowercase().contains("scope") {
                    Some(format!(
                        "{base}\nThis looks like a missing scope, not a bad token. Re-create the token with the scopes the command needs at:\nhttps://id.atlassian.com/manage-profile/security/api-tokens"
                    ))
                } else {
                    Some(base)
                }
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
            ApiError::EndpointGone { .. } => {
                Some("This API endpoint has been removed by Atlassian. Update atlassian-cli to the latest version.".to_string())
            }
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

    #[test]
    fn authentication_failed_with_scope_message_points_at_scopes() {
        let err = ApiError::AuthenticationFailed {
            message: "Invalid or expired credentials (Unauthorized; scope does not match)"
                .to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("missing scope"));
        assert!(hint.contains("api-tokens"));
    }

    #[test]
    fn authentication_failed_without_scope_message_omits_scope_hint() {
        let err = ApiError::AuthenticationFailed {
            message: "Invalid or expired credentials".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("auth test"));
        assert!(!hint.contains("missing scope"));
    }

    #[test]
    fn endpoint_gone_has_suggestion() {
        let err = ApiError::EndpointGone {
            message: "The requested API has been removed".to_string(),
        };
        assert!(err.suggestion().is_some());
        assert!(err.suggestion().unwrap().contains("Update atlassian-cli"));
    }

    #[test]
    fn endpoint_gone_is_not_retryable() {
        let err = ApiError::EndpointGone {
            message: "removed".to_string(),
        };
        assert!(!err.is_retryable());
    }
}
