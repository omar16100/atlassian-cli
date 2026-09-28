//! What every request path does with a response: trace it, and turn a non-2xx
//! status into an [`ApiError`].
//!
//! `--debug` used to print nothing a failing request could be traced with.
//! Requests were logged with their URL, but never the status that came back,
//! how long it took, or the body of an error (a 404's body was dropped
//! entirely). Each path also carried its own copy of the status-to-error
//! match. Both now live here, so every path logs the same way.
//!
//! What is logged, at `debug`: method, URL with secret-named query values
//! redacted, status, elapsed time, and for a non-2xx the response body with
//! secret-named JSON fields redacted, credential-shaped values scrubbed, and
//! the whole truncated. Never logged: request headers, tokens, or request
//! bodies, which can carry secrets (secured pipeline variables travel in them).

use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use tracing::debug;
use url::Url;

use crate::error::ApiError;
use crate::{scrub_credentials, unauthorized_message};

/// Longest error body logged, in characters.
const MAX_LOGGED_BODY: usize = 2048;

/// Fragments that mark a query parameter or JSON field as a secret, matched
/// case-insensitively anywhere in the name: `access_token`, `client_secret`,
/// `apikey`, `oauth_signature`. Plain `key` is not on the list: Jira's
/// `issueKey` and a pipeline variable's `key` are names, and hiding them would
/// make the trace useless for the requests it exists for.
const SECRET_NAME_PARTS: [&str; 10] = [
    "token",
    "secret",
    "password",
    "passwd",
    "credential",
    "authorization",
    "signature",
    "private_key",
    "api_key",
    "apikey",
];

/// Whether a parameter or field name marks its value as a secret.
fn is_secret_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "jwt" || SECRET_NAME_PARTS.iter().any(|part| name.contains(part))
}

/// The URL as it may appear in a log: query values that look like credentials
/// replaced. `bb api` passes paths through as typed, so a token in a query
/// string is the user's to send but not ours to print.
pub(crate) fn redact_url(url: &Url) -> String {
    if url.query().is_none() {
        return url.to_string();
    }
    let mut redacted = url.clone();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let secret = is_secret_name(&key);
            let value = if secret {
                "<redacted>".to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    redacted.query_pairs_mut().clear().extend_pairs(pairs);
    redacted.to_string()
}

/// Replace the values of secret-named fields, at any depth, and the `value` of
/// anything marked `"secured": true` (a pipeline variable echoed back in a
/// validation error carries its secret there under a plain name).
fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let secured = map.get("secured").and_then(serde_json::Value::as_bool) == Some(true);
            for (name, field) in map.iter_mut() {
                let hide = is_secret_name(name) || (secured && name == "value");
                if hide && !field.is_null() {
                    *field = serde_json::Value::String("<redacted>".to_string());
                } else {
                    redact_json(field);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(redact_json),
        _ => {}
    }
}

/// An error body as it may appear in a log: secret-named JSON fields redacted,
/// credential-shaped values scrubbed, cut to [`MAX_LOGGED_BODY`] characters on a
/// char boundary.
pub(crate) fn body_for_log(body: &str) -> String {
    let body = body.trim();
    let text = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(mut value) => {
            redact_json(&mut value);
            value.to_string()
        }
        Err(_) => body.to_string(),
    };
    let scrubbed = scrub_credentials(&text);
    if scrubbed.chars().count() <= MAX_LOGGED_BODY {
        return scrubbed;
    }
    let short: String = scrubbed.chars().take(MAX_LOGGED_BODY).collect();
    format!("{short}... (truncated)")
}

/// Trace one exchange: what was asked, what came back, and how long it took.
pub(crate) fn log_response(method: &Method, url: &Url, status: StatusCode, elapsed: Duration) {
    debug!(
        method = %method,
        url = %redact_url(url),
        status = status.as_u16(),
        elapsed_ms = elapsed.as_millis() as u64,
        "Response received"
    );
}

/// Log a non-2xx body for `--debug`.
pub(crate) fn log_error_body(status: StatusCode, url: &Url, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    debug!(
        status = status.as_u16(),
        url = %redact_url(url),
        body = %body_for_log(body),
        "Error response body"
    );
}

/// Map a non-2xx response to the error the callers expect, logging its body.
///
/// The body is read once, here, so the log and the error see the same text.
/// A body that cannot be read falls back to the fixed wording each status had.
pub(crate) async fn error_for_status(response: Response, url: &Url) -> ApiError {
    let status = response.status();
    // Read before the body consumes the response.
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let body = response.text().await.ok();
    if let Some(text) = &body {
        log_error_body(status, url, text);
    }
    let message = |fallback: &str| body.clone().unwrap_or_else(|| fallback.to_string());

    match status {
        StatusCode::UNAUTHORIZED => ApiError::AuthenticationFailed {
            message: unauthorized_message(body.as_deref().unwrap_or_default()),
        },
        StatusCode::FORBIDDEN => ApiError::Forbidden {
            message: message("Access forbidden"),
        },
        StatusCode::NOT_FOUND => ApiError::NotFound {
            resource: url.path().to_string(),
        },
        StatusCode::BAD_REQUEST => ApiError::BadRequest {
            message: message("Bad request"),
        },
        StatusCode::GONE => ApiError::EndpointGone {
            message: message("API endpoint has been removed"),
        },
        StatusCode::TOO_MANY_REQUESTS => ApiError::RateLimitExceeded { retry_after },
        StatusCode::NOT_ACCEPTABLE => ApiError::ServerError {
            status: 406,
            message: message("Content not acceptable"),
        },
        status if status.is_server_error() => ApiError::ServerError {
            status: status.as_u16(),
            message: message("Server error"),
        },
        status => ApiError::ServerError {
            status: status.as_u16(),
            message: message(&format!("Unexpected status: {status}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_query_values_are_redacted() {
        let url = Url::parse("https://x.atlassian.net/rest/api/3/myself?access_token=abc123&expand=groups&JWT=eyJ.x.y").unwrap();
        let shown = redact_url(&url);
        assert!(!shown.contains("abc123"), "{shown}");
        assert!(!shown.contains("eyJ.x.y"), "{shown}");
        assert!(shown.contains("expand=groups"), "{shown}");
        assert!(shown.contains("access_token=%3Credacted%3E"), "{shown}");
    }

    /// Names beyond an exact list: `bb api --query` passes whatever it is given.
    #[test]
    fn any_secret_like_query_name_is_redacted() {
        let url = Url::parse(
            "https://api.bitbucket.org/2.0/x?client_secret=s1&oauth_token=s2&private_token=s3&issueKey=PROJ-1",
        )
        .unwrap();
        let shown = redact_url(&url);
        for secret in ["s1", "s2", "s3"] {
            assert!(
                !shown.contains(&format!("={secret}")),
                "{secret} in {shown}"
            );
        }
        assert!(shown.contains("issueKey=PROJ-1"), "{shown}");
    }

    #[test]
    fn secret_fields_in_a_json_error_body_are_redacted() {
        let body = r#"{
            "error": {"message": "invalid", "detail": {"access_token": "t0k3n", "password": "hunter2"}},
            "client_secret": "cs-123",
            "variables": [
                {"key": "DEPLOY_KEY", "value": "super-secret", "secured": true},
                {"key": "REGION", "value": "eu-west-1", "secured": false}
            ]
        }"#;
        let shown = body_for_log(body);
        for secret in ["t0k3n", "hunter2", "cs-123", "super-secret"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
        for kept in ["invalid", "DEPLOY_KEY", "REGION", "eu-west-1"] {
            assert!(shown.contains(kept), "{kept} missing from {shown}");
        }
    }

    #[test]
    fn a_url_without_a_query_is_unchanged() {
        let url =
            Url::parse("https://api.bitbucket.org/2.0/repositories/ws/web_app/pipelines/").unwrap();
        assert_eq!(redact_url(&url), url.to_string());
    }

    #[test]
    fn logged_bodies_are_scrubbed_and_bounded() {
        let echoed =
            r#"{"error": "bad", "echo": "Authorization: Bearer abcdefghijklmnopqrstuvwxyz0123"}"#;
        let shown = body_for_log(echoed);
        assert!(!shown.contains("abcdefghijklmnopqrstuvwxyz0123"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");

        let long = "é".repeat(MAX_LOGGED_BODY + 10);
        let shown = body_for_log(&long);
        assert!(shown.ends_with("... (truncated)"));
        assert_eq!(shown.chars().filter(|c| *c == 'é').count(), MAX_LOGGED_BODY);
    }
}
