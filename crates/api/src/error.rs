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
                if let Some(hint) = scope_hint(message) {
                    return Some(hint);
                }
                let lower = message.to_lowercase();
                if lower.contains("scope") || lower.contains("privilege") || lower.contains("permission") {
                    Some(format!(
                        "{base}\nIf this is a scope problem, note that a token's scopes are fixed when it is created: \
                         make a replacement at https://id.atlassian.com/manage-profile/security/api-tokens"
                    ))
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

    /// The hint used to point at bitbucket.org/account/settings/app-passwords.
    /// App passwords are deprecated, and scopes cannot be edited after a token
    /// is created, so that link sent people somewhere that could not help.
    #[test]
    fn forbidden_with_scope_message_points_at_a_replacement_token() {
        let err = ApiError::Forbidden {
            message: "Your credentials lack the required scope.".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("auth test"));
        assert!(hint.contains("id.atlassian.com"), "{hint}");
        assert!(hint.contains("replacement"), "{hint}");
        assert!(!hint.contains("app-passwords"), "{hint}");
    }

    #[test]
    fn forbidden_without_scope_omits_the_token_advice() {
        let err = ApiError::Forbidden {
            message: "no access".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("auth test"));
        assert!(!hint.contains("replacement"), "{hint}");
    }

    #[test]
    fn forbidden_with_permission_message_includes_token_guidance() {
        let err = ApiError::Forbidden {
            message: "Insufficient Permission to access this resource".to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("id.atlassian.com"), "{hint}");
    }

    /// When the body carries the granted/required detail, the precise hint
    /// wins over the keyword-matched generic one.
    #[test]
    fn a_structured_403_gets_the_specific_hint() {
        let err = ApiError::Forbidden {
            message: serde_json::json!({
                "error": {"detail": {"granted": ["account"], "required": ["pullrequest"]}}
            })
            .to_string(),
        };
        let hint = err.suggestion().unwrap();
        assert!(hint.contains("missing the pullrequest scope"), "{hint}");
        assert!(
            !hint.contains("auth test"),
            "specific hint replaces the generic: {hint}"
        );
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

/// Turn Bitbucket's 403 body into a message that names the missing scope.
///
/// Bitbucket says which scopes the credential has and which the endpoint
/// wanted, and the CLI was throwing that away:
///
/// ```json
/// {"type": "error", "error": {"message": "Your credentials lack one or more required privilege scopes.",
///  "detail": {"granted": ["repository:write"], "required": ["pullrequest"]}}}
/// ```
///
/// Two details matter for what the hint should say. Scopes are fixed when a
/// token is created and cannot be widened afterwards, so telling someone to
/// "add the scope" sends them somewhere that cannot help; they need a
/// replacement token. And `granted` describes the *token*, not the account --
/// so when it already covers everything required, the cause is repository
/// permissions or an IP allowlist, and saying "missing scope" would send them
/// down the wrong path entirely.
fn scope_hint(message: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(message).ok()?;
    let detail = parsed.get("error")?.get("detail")?;

    let list = |key: &str| -> Vec<String> {
        detail
            .get(key)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    let granted = list("granted");
    let required = list("required");
    if required.is_empty() {
        return None;
    }

    let missing: Vec<&String> = required.iter().filter(|r| !granted.contains(r)).collect();

    if missing.is_empty() {
        return Some(format!(
            "The token already grants every scope this endpoint requires ({}).\n\
             The refusal is therefore not about scopes: check the account's access to this \
             resource, and whether an IP allowlist applies.",
            required.join(", ")
        ));
    }

    let missing: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
    Some(format!(
        "The token is missing the {} scope{}.\n\
         Granted: {}\n\
         A token's scopes are fixed when it is created and cannot be widened, so create a \
         replacement at https://id.atlassian.com/manage-profile/security/api-tokens \
         and re-run `atlassian-cli auth login --bitbucket`.",
        missing.join(", "),
        if missing.len() == 1 { "" } else { "s" },
        if granted.is_empty() {
            "(none reported)".to_string()
        } else {
            granted.join(", ")
        }
    ))
}

#[cfg(test)]
mod scope_hint_tests {
    use super::*;

    fn body(granted: &[&str], required: &[&str]) -> String {
        serde_json::json!({
            "type": "error",
            "error": {
                "message": "Your credentials lack one or more required privilege scopes.",
                "detail": {"granted": granted, "required": required}
            }
        })
        .to_string()
    }

    #[test]
    fn it_names_the_missing_scope() {
        let hint = scope_hint(&body(&["repository:write"], &["pullrequest"])).unwrap();
        assert!(hint.contains("missing the pullrequest scope"), "{hint}");
        assert!(hint.contains("repository:write"), "granted list: {hint}");
    }

    /// Scopes cannot be widened after creation, so "add the scope" would send
    /// the user somewhere that cannot help them.
    #[test]
    fn it_tells_the_user_to_replace_the_token_not_edit_it() {
        let hint = scope_hint(&body(&[], &["write:pipeline:bitbucket"])).unwrap();
        assert!(hint.contains("create a replacement"), "{hint}");
        assert!(!hint.to_lowercase().contains("add the scope"), "{hint}");
    }

    /// `granted` describes the token, not the account. When it already covers
    /// what was required, blaming scopes sends the user down the wrong path.
    #[test]
    fn a_sufficient_token_points_away_from_scopes() {
        let hint = scope_hint(&body(&["pullrequest", "account"], &["pullrequest"])).unwrap();
        assert!(hint.contains("not about scopes"), "{hint}");
        assert!(hint.contains("IP allowlist"), "{hint}");
    }

    #[test]
    fn it_handles_both_scope_vocabularies() {
        // Classic OAuth consumer scopes, and the newer scoped API-token form.
        assert!(scope_hint(&body(&["repository"], &["pullrequest"])).is_some());
        assert!(scope_hint(&body(
            &["read:repository:bitbucket"],
            &["write:pipeline:bitbucket"]
        ))
        .is_some());
    }

    #[test]
    fn a_body_without_the_detail_block_yields_nothing() {
        assert!(scope_hint("Access forbidden").is_none());
        assert!(scope_hint(r#"{"error":{"message":"nope"}}"#).is_none());
    }
}
