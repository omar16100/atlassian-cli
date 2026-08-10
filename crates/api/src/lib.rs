pub mod error;
pub mod pagination;
pub mod ratelimit;
pub mod retry;

use backoff::backoff::Backoff;
use error::{ApiError, Result};
use ratelimit::RateLimiter;
use reqwest::header::HeaderMap;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use retry::{retry_with_backoff, RetryConfig};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt;
use std::time::Duration;
use tracing::{debug, error, warn};
use url::Url;

#[derive(Clone)]
pub enum AuthMethod {
    Basic {
        username: String,
        token: SecretString,
    },
    Bearer {
        token: SecretString,
    },
    GenieKey {
        api_key: SecretString,
    },
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::Basic { username, .. } => f
                .debug_struct("Basic")
                .field("username", username)
                .field("token", &"[REDACTED]")
                .finish(),
            AuthMethod::Bearer { .. } => f
                .debug_struct("Bearer")
                .field("token", &"[REDACTED]")
                .finish(),
            AuthMethod::GenieKey { .. } => f
                .debug_struct("GenieKey")
                .field("api_key", &"[REDACTED]")
                .finish(),
        }
    }
}

/// Compare two URLs by full origin: scheme, host **and port**.
///
/// Port matters. Comparing scheme and host alone lets `https://site:8443/x`
/// through on a `https://site` profile, and on a localhost profile it lets any
/// other local port receive the profile's credentials.
fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host() == b.host()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// The `Retry-After` delay in seconds, when the server sent one. The HTTP-date
/// form is ignored; Atlassian sends seconds.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// An arbitrary request for [`ApiClient::request_raw`].
pub struct RawRequest<'a> {
    pub method: Method,
    /// Path (and optional query) relative to the client's base URL.
    pub path: &'a str,
    pub headers: HeaderMap,
    pub body: Option<&'a [u8]>,
    /// Overrides the client-wide 30s timeout for this request only.
    pub timeout: Option<Duration>,
}

/// A response with no status-to-error mapping applied.
#[derive(Debug, Clone)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl RawResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Case-insensitive header lookup. Returns the first match.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    /// Same-origin-only redirect policy; used by `request_raw`.
    raw_client: Client,
    base_url: Url,
    auth: Option<AuthMethod>,
    retry_config: RetryConfig,
    rate_limiter: RateLimiter,
}

impl ApiClient {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let url = Url::parse(base_url.as_ref()).map_err(ApiError::InvalidUrl)?;

        // Enforce HTTPS for security (prevent accidental credential leaks over HTTP)
        // Allow HTTP only for localhost/127.0.0.1 (for testing)
        if url.scheme() != "https" {
            let is_localhost = url
                .host_str()
                .map(|h| h == "localhost" || h == "127.0.0.1" || h.starts_with("127."))
                .unwrap_or(false);

            if !is_localhost {
                return Err(ApiError::InvalidUrl(
                    url::ParseError::InvalidDomainCharacter,
                ));
            }
        }

        let client = Client::builder()
            .user_agent(format!("atlassian-cli/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ApiError::RequestFailed)?;

        // `request_raw` sends user-chosen methods, bodies and headers, so it gets
        // a client that refuses to leave the profile's origin. The default
        // policy would follow a `Location` anywhere: reqwest strips
        // `Authorization` cross-host, but 307/308 replay the body and custom
        // `-H` headers are not stripped. A stopped redirect is returned to the
        // caller as the 3xx itself, which is the transparent answer for a
        // passthrough. The normal `client` keeps following redirects, because
        // attachment downloads depend on the cross-host hop to Atlassian's
        // media host.
        let origin = url.clone();
        let raw_client = Client::builder()
            .user_agent(format!("atlassian-cli/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 10 {
                    attempt.error("too many redirects")
                } else if same_origin(attempt.url(), &origin) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(ApiError::RequestFailed)?;

        Ok(Self {
            client,
            raw_client,
            base_url: url,
            auth: None,
            retry_config: RetryConfig::default(),
            rate_limiter: RateLimiter::new(),
        })
    }

    /// Safely join a path to the base URL, ensuring the origin remains unchanged
    /// to prevent SSRF attacks.
    fn safe_join(&self, path: &str) -> Result<Url> {
        let joined = self
            .base_url
            .join(path.strip_prefix('/').unwrap_or(path))
            .map_err(ApiError::InvalidUrl)?;

        if !same_origin(&joined, &self.base_url) {
            return Err(ApiError::InvalidUrl(
                url::ParseError::InvalidDomainCharacter,
            ));
        }

        Ok(joined)
    }

    pub fn with_basic_auth(
        mut self,
        username: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        self.auth = Some(AuthMethod::Basic {
            username: username.into(),
            token: SecretString::from(token.into()),
        });
        self
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(AuthMethod::Bearer {
            token: SecretString::from(token.into()),
        });
        self
    }

    pub fn with_genie_key(mut self, api_key: impl Into<String>) -> Self {
        self.auth = Some(AuthMethod::GenieKey {
            api_key: SecretString::from(api_key.into()),
        });
        self
    }

    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    pub fn base_url(&self) -> &str {
        self.base_url.as_str()
    }

    /// Returns a reference to the underlying HTTP client for raw requests (e.g., multipart uploads).
    pub fn http_client(&self) -> &Client {
        &self.client
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::GET, path, Option::<&()>::None).await
    }

    pub async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request(Method::POST, path, Some(body)).await
    }

    pub async fn put<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request(Method::PUT, path, Some(body)).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(Method::DELETE, path, Option::<&()>::None)
            .await
    }

    pub async fn delete_with_body<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        self.request(Method::DELETE, path, Some(body)).await
    }

    /// DELETE that expects 204 No Content (no response body).
    pub async fn delete_no_content(&self, path: &str) -> Result<()> {
        if let Some(wait_secs) = self.rate_limiter.check_limit().await {
            warn!(wait_secs, "Rate limit reached, waiting");
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }

        let joined = self.safe_join(path)?;

        debug!(method = "DELETE", url = %joined, "Sending delete (no content) request");

        retry_with_backoff(&self.retry_config, || async {
            let mut req = self.client.request(Method::DELETE, joined.clone());
            req = self.apply_auth(req);

            let response = req.send().await.map_err(ApiError::RequestFailed)?;

            self.rate_limiter.update_from_response(&response).await;

            let status = response.status();

            match status {
                StatusCode::UNAUTHORIZED => Err(ApiError::AuthenticationFailed {
                    message: "Invalid or expired credentials".to_string(),
                }),
                StatusCode::FORBIDDEN => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Access forbidden".to_string());
                    Err(ApiError::Forbidden { message })
                }
                StatusCode::NOT_FOUND => {
                    let resource = joined.path().to_string();
                    Err(ApiError::NotFound { resource })
                }
                StatusCode::BAD_REQUEST => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Bad request".to_string());
                    Err(ApiError::BadRequest { message })
                }
                StatusCode::GONE => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "API endpoint has been removed".to_string());
                    Err(ApiError::EndpointGone { message })
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    Err(ApiError::RateLimitExceeded { retry_after })
                }
                status if status.is_server_error() => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Server error".to_string());
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
                status if status.is_success() => Ok(()),
                _ => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| format!("Unexpected status: {}", status));
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
            }
        })
        .await
    }

    /// Get plain text content from an endpoint.
    /// Sets Accept: text/plain; charset=utf-8 header.
    /// Includes retry logic and rate limiting.
    pub async fn get_text(&self, path: &str) -> Result<String> {
        if let Some(wait_secs) = self.rate_limiter.check_limit().await {
            warn!(wait_secs, "Rate limit reached, waiting");
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }

        let joined = self.safe_join(path)?;

        debug!(method = "GET", url = %joined, "Sending text request");

        let result = retry_with_backoff(&self.retry_config, || async {
            let mut req = self.client.request(Method::GET, joined.clone());
            req = self.apply_auth(req);
            req = req.header("Accept", "text/plain, */*;q=0.1");

            let response = req.send().await.map_err(ApiError::RequestFailed)?;

            self.rate_limiter.update_from_response(&response).await;

            let status = response.status();

            match status {
                StatusCode::UNAUTHORIZED => Err(ApiError::AuthenticationFailed {
                    message: "Invalid or expired credentials".to_string(),
                }),
                StatusCode::FORBIDDEN => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Access forbidden".to_string());
                    Err(ApiError::Forbidden { message })
                }
                StatusCode::NOT_FOUND => {
                    let resource = joined.path().to_string();
                    Err(ApiError::NotFound { resource })
                }
                StatusCode::BAD_REQUEST => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Bad request".to_string());
                    Err(ApiError::BadRequest { message })
                }
                StatusCode::NOT_ACCEPTABLE => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Content not acceptable".to_string());
                    Err(ApiError::ServerError {
                        status: 406,
                        message,
                    })
                }
                StatusCode::GONE => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "API endpoint has been removed".to_string());
                    Err(ApiError::EndpointGone { message })
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    Err(ApiError::RateLimitExceeded { retry_after })
                }
                status if status.is_server_error() => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Server error".to_string());
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
                status if status.is_success() => response.text().await.map_err(|e| {
                    error!("Failed to read text response: {}", e);
                    ApiError::InvalidResponse(e.to_string())
                }),
                _ => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| format!("Unexpected status: {}", status));
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
            }
        })
        .await?;

        Ok(result)
    }

    /// Resolve `path` against the base URL, applying the same-origin (SSRF)
    /// check used by every request. Public so callers can validate or preview a
    /// path without sending anything.
    pub fn resolve_url(&self, path: &str) -> Result<Url> {
        self.safe_join(path)
    }

    /// Send an arbitrary request and return the status, headers and body bytes.
    ///
    /// Unlike [`ApiClient::request`], a non-2xx status is returned as
    /// `Ok(RawResponse)` rather than mapped to an [`ApiError`], so callers can
    /// surface the API's own error body. Only transport failures and URL
    /// validation produce `Err`. Same-origin validation, auth and rate limiting
    /// still apply.
    ///
    /// Retries on 429/5xx are limited to idempotent methods. `request` retries
    /// POSTs, which can double-create; a raw passthrough must not inherit that.
    pub async fn request_raw(&self, req: RawRequest<'_>) -> Result<RawResponse> {
        if let Some(wait_secs) = self.rate_limiter.check_limit().await {
            warn!(wait_secs, "Rate limit reached, waiting");
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }

        let joined = self.safe_join(req.path)?;
        debug!(method = %req.method, url = %joined, "Sending raw request");

        let idempotent = matches!(
            req.method,
            Method::GET | Method::HEAD | Method::PUT | Method::DELETE | Method::OPTIONS
        );
        // retry_with_backoff cannot be used here: its closure must signal a
        // retryable outcome as Err, which would discard the RawResponse we have
        // to return on the final attempt.
        let mut backoff = self.retry_config.backoff();
        let mut attempts = 0usize;

        loop {
            attempts += 1;

            let mut builder = self.raw_client.request(req.method.clone(), joined.clone());
            builder = self.apply_auth(builder);
            builder = builder.headers(req.headers.clone());
            if let Some(body) = req.body {
                builder = builder.body(body.to_vec());
            }
            if let Some(timeout) = req.timeout {
                builder = builder.timeout(timeout);
            }

            let response = builder.send().await.map_err(ApiError::RequestFailed)?;
            self.rate_limiter.update_from_response(&response).await;
            let status = response.status();

            let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
            if idempotent && retryable && attempts < self.retry_config.max_retries {
                if let Some(wait) = backoff.next_backoff() {
                    // A 429 says how long to wait; obey it rather than racing
                    // back in after a short exponential sleep.
                    let wait = retry_after(&response).unwrap_or(wait);
                    warn!(
                        status = status.as_u16(),
                        attempt = attempts,
                        wait_ms = wait.as_millis(),
                        "Raw request failed, retrying"
                    );
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }

            let headers = response
                .headers()
                .iter()
                .map(|(name, value)| {
                    (
                        name.as_str().to_string(),
                        value.to_str().unwrap_or_default().to_string(),
                    )
                })
                .collect();
            let body = response
                .bytes()
                .await
                .map_err(|err| ApiError::InvalidResponse(err.to_string()))?
                .to_vec();

            return Ok(RawResponse {
                status: status.as_u16(),
                headers,
                body,
            });
        }
    }

    /// Get binary content from an endpoint.
    /// Includes retry logic and rate limiting.
    pub async fn get_bytes(&self, path: &str) -> Result<Vec<u8>> {
        if let Some(wait_secs) = self.rate_limiter.check_limit().await {
            warn!(wait_secs, "Rate limit reached, waiting");
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }

        let joined = self.safe_join(path)?;

        debug!(method = "GET", url = %joined, "Sending bytes request");

        let result = retry_with_backoff(&self.retry_config, || async {
            let mut req = self.client.request(Method::GET, joined.clone());
            req = self.apply_auth(req);

            let response = req.send().await.map_err(ApiError::RequestFailed)?;

            self.rate_limiter.update_from_response(&response).await;

            let status = response.status();

            match status {
                StatusCode::UNAUTHORIZED => Err(ApiError::AuthenticationFailed {
                    message: "Invalid or expired credentials".to_string(),
                }),
                StatusCode::FORBIDDEN => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Access forbidden".to_string());
                    Err(ApiError::Forbidden { message })
                }
                StatusCode::NOT_FOUND => {
                    let resource = joined.path().to_string();
                    Err(ApiError::NotFound { resource })
                }
                StatusCode::GONE => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "API endpoint has been removed".to_string());
                    Err(ApiError::EndpointGone { message })
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    Err(ApiError::RateLimitExceeded { retry_after })
                }
                status if status.is_success() => {
                    response.bytes().await.map(|b| b.to_vec()).map_err(|e| {
                        error!("Failed to read bytes response: {}", e);
                        ApiError::InvalidResponse(e.to_string())
                    })
                }
                _ => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| format!("Unexpected status: {}", status));
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
            }
        })
        .await?;

        Ok(result)
    }

    pub async fn request<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        if let Some(wait_secs) = self.rate_limiter.check_limit().await {
            warn!(wait_secs, "Rate limit reached, waiting");
            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
        }

        let joined = self.safe_join(path)?;

        debug!(method = %method, url = %joined, "Sending request");

        let result = retry_with_backoff(&self.retry_config, || async {
            let mut req = self.client.request(method.clone(), joined.clone());
            req = self.apply_auth(req);

            if let Some(body) = body {
                req = req.json(body);
            }

            let response = req.send().await.map_err(ApiError::RequestFailed)?;

            self.rate_limiter.update_from_response(&response).await;

            let status = response.status();

            match status {
                StatusCode::UNAUTHORIZED => Err(ApiError::AuthenticationFailed {
                    message: "Invalid or expired credentials".to_string(),
                }),
                StatusCode::FORBIDDEN => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Access forbidden".to_string());
                    Err(ApiError::Forbidden { message })
                }
                StatusCode::NOT_FOUND => {
                    let resource = joined.path().to_string();
                    Err(ApiError::NotFound { resource })
                }
                StatusCode::BAD_REQUEST => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Bad request".to_string());
                    Err(ApiError::BadRequest { message })
                }
                StatusCode::GONE => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "API endpoint has been removed".to_string());
                    Err(ApiError::EndpointGone { message })
                }
                StatusCode::TOO_MANY_REQUESTS => {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    Err(ApiError::RateLimitExceeded { retry_after })
                }
                status if status.is_server_error() => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Server error".to_string());
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
                status if status.is_success() => {
                    let bytes = response
                        .bytes()
                        .await
                        .map_err(|e| ApiError::InvalidResponse(e.to_string()))?;
                    // Successful responses with an empty (or whitespace-only) body,
                    // e.g. HTTP 204 No Content from Jira update/transition/assign and
                    // most DELETEs, are treated as JSON `null`. Callers that discard
                    // the body (`let _: Value`) then succeed instead of failing to
                    // parse an empty body as JSON.
                    let slice: &[u8] = if bytes.iter().all(|b| b.is_ascii_whitespace()) {
                        b"null"
                    } else {
                        &bytes
                    };
                    serde_json::from_slice::<T>(slice).map_err(|e| {
                        error!("Failed to parse JSON response: {}", e);
                        ApiError::InvalidResponse(e.to_string())
                    })
                }
                _ => {
                    let message = response
                        .text()
                        .await
                        .unwrap_or_else(|_| format!("Unexpected status: {}", status));
                    Err(ApiError::ServerError {
                        status: status.as_u16(),
                        message,
                    })
                }
            }
        })
        .await?;

        Ok(result)
    }

    pub fn apply_auth(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.auth {
            Some(AuthMethod::Basic { username, token }) => {
                request.basic_auth(username, Some(token.expose_secret()))
            }
            Some(AuthMethod::Bearer { token }) => request.bearer_auth(token.expose_secret()),
            Some(AuthMethod::GenieKey { api_key }) => request.header(
                "Authorization",
                format!("GenieKey {}", api_key.expose_secret()),
            ),
            None => request,
        }
    }

    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_403_returns_forbidden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("test"))
            .respond_with(ResponseTemplate::new(403).set_body_string("You do not have access"))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result: error::Result<serde_json::Value> = client.get("/test").await;

        match result {
            Err(ApiError::Forbidden { message }) => {
                assert!(message.contains("You do not have access"));
            }
            other => panic!("Expected Forbidden, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_401_returns_authentication_failed() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("test"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result: error::Result<serde_json::Value> = client.get("/test").await;

        match result {
            Err(ApiError::AuthenticationFailed { .. }) => {}
            other => panic!("Expected AuthenticationFailed, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_403_get_text_returns_forbidden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("text-endpoint"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden resource"))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result = client.get_text("/text-endpoint").await;

        match result {
            Err(ApiError::Forbidden { message }) => {
                assert!(message.contains("Forbidden resource"));
            }
            other => panic!("Expected Forbidden, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_403_get_bytes_returns_forbidden() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("bytes-endpoint"))
            .respond_with(ResponseTemplate::new(403).set_body_string("Access denied"))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result = client.get_bytes("/bytes-endpoint").await;

        match result {
            Err(ApiError::Forbidden { message }) => {
                assert!(message.contains("Access denied"));
            }
            other => panic!("Expected Forbidden, got: {:?}", other),
        }
    }

    // Regression for #45: a successful PUT/POST returning HTTP 204 No Content (empty
    // body) must not fail JSON parsing. Callers discard the body as `Value`.
    #[tokio::test]
    async fn test_204_no_content_put_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("issue/AEA-1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result: error::Result<serde_json::Value> = client
            .put("/issue/AEA-1", &serde_json::json!({"fields": {}}))
            .await;

        match result {
            Ok(serde_json::Value::Null) => {}
            other => panic!("Expected Ok(Null) for 204, got: {:?}", other),
        }
    }

    // A 200 with an empty/whitespace-only body is also treated as null.
    #[tokio::test]
    async fn test_200_empty_body_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("transitions"))
            .respond_with(ResponseTemplate::new(200).set_body_string("  \n"))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result: error::Result<serde_json::Value> =
            client.post("/transitions", &serde_json::json!({})).await;

        match result {
            Ok(serde_json::Value::Null) => {}
            other => panic!("Expected Ok(Null) for empty 200, got: {:?}", other),
        }
    }

    // A non-empty JSON body on success still parses normally.
    #[tokio::test]
    async fn test_200_json_body_still_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("issue/AEA-1"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"key": "AEA-1"})),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let result: serde_json::Value = client.get("/issue/AEA-1").await.unwrap();
        assert_eq!(result["key"], "AEA-1");
    }

    // -----------------------------------------------------------------------
    // request_raw
    // -----------------------------------------------------------------------

    /// The point of the raw path: a non-2xx status is data, not an error, so the
    /// API's own error body survives instead of being replaced by ApiError.
    #[tokio::test]
    async fn test_request_raw_surfaces_non_2xx_without_erroring() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/rest/api/3/issue/NOPE-1"))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"errorMessages": ["Issue does not exist"]})),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let response = client
            .request_raw(RawRequest {
                method: Method::GET,
                path: "/rest/api/3/issue/NOPE-1",
                headers: HeaderMap::new(),
                body: None,
                timeout: None,
            })
            .await
            .unwrap();

        assert_eq!(response.status, 404);
        assert!(!response.is_success());
        assert!(response
            .header("Content-Type")
            .unwrap()
            .contains("application/json"));
        assert!(String::from_utf8_lossy(&response.body).contains("Issue does not exist"));
    }

    #[tokio::test]
    async fn test_request_raw_applies_headers_and_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/issue"))
            .and(header("X-Atlassian-Token", "no-check"))
            .and(body_string("{\"fields\":{}}"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(serde_json::json!({"key": "A-1"})),
            )
            .mount(&server)
            .await;

        let mut headers = HeaderMap::new();
        headers.insert("X-Atlassian-Token", "no-check".parse().unwrap());

        let client = ApiClient::new(server.uri()).unwrap();
        let response = client
            .request_raw(RawRequest {
                method: Method::POST,
                path: "/rest/api/3/issue",
                headers,
                body: Some(b"{\"fields\":{}}"),
                timeout: None,
            })
            .await
            .unwrap();

        assert_eq!(response.status, 201);
    }

    #[tokio::test]
    async fn test_request_raw_retries_5xx_for_get() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(ResponseTemplate::new(500))
            .expect(3)
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri())
            .unwrap()
            .with_retry_config(RetryConfig {
                initial_interval: Duration::from_millis(1),
                ..RetryConfig::default()
            });
        let response = client
            .request_raw(RawRequest {
                method: Method::GET,
                path: "/flaky",
                headers: HeaderMap::new(),
                body: None,
                timeout: None,
            })
            .await
            .unwrap();

        assert_eq!(response.status, 500);
    }

    /// Replaying a POST can double-create. `request` does retry POSTs; the raw
    /// path deliberately does not inherit that.
    #[tokio::test]
    async fn test_request_raw_never_retries_post() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/create"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri())
            .unwrap()
            .with_retry_config(RetryConfig {
                initial_interval: Duration::from_millis(1),
                ..RetryConfig::default()
            });
        let response = client
            .request_raw(RawRequest {
                method: Method::POST,
                path: "/create",
                headers: HeaderMap::new(),
                body: Some(b"{}"),
                timeout: None,
            })
            .await
            .unwrap();

        assert_eq!(response.status, 503);
    }

    #[tokio::test]
    async fn test_request_raw_rejects_cross_host_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let err = client
            .request_raw(RawRequest {
                method: Method::GET,
                path: "https://evil.example.com/steal",
                headers: HeaderMap::new(),
                body: None,
                timeout: None,
            })
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::InvalidUrl(_)), "got {err:?}");
    }

    #[test]
    fn test_resolve_url_enforces_same_origin() {
        let client = ApiClient::new("https://site.atlassian.net").unwrap();

        assert_eq!(
            client.resolve_url("/rest/api/3/myself").unwrap().as_str(),
            "https://site.atlassian.net/rest/api/3/myself"
        );
        // Relative paths work with or without the leading slash.
        assert_eq!(
            client.resolve_url("rest/api/3/myself").unwrap().as_str(),
            "https://site.atlassian.net/rest/api/3/myself"
        );
        // Other hosts, scheme downgrades and userinfo tricks are all rejected.
        for bad in [
            "https://evil.example.com/x",
            "http://site.atlassian.net/x",
            "https://site.atlassian.net@evil.example.com/",
            "//evil.example.com/x",
        ] {
            let resolved = client.resolve_url(bad);
            match resolved {
                Err(_) => {}
                // A protocol-relative path is not treated as a host by `join`;
                // pin the behaviour so a future change cannot silently open it up.
                Ok(url) => assert_eq!(url.host_str(), Some("site.atlassian.net"), "{bad}"),
            }
        }
    }

    /// Regression: comparing scheme and host but not port let any other port on
    /// the same host receive the profile's credentials.
    #[tokio::test]
    async fn test_request_raw_rejects_a_different_port_on_the_same_host() {
        let victim = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("secrets"))
            .expect(0)
            .mount(&victim)
            .await;

        let server = MockServer::start().await;
        let client = ApiClient::new(server.uri()).unwrap();
        let err = client
            .request_raw(RawRequest {
                method: Method::GET,
                path: &format!("{}/steal", victim.uri()),
                headers: HeaderMap::new(),
                body: None,
                timeout: None,
            })
            .await
            .unwrap_err();

        assert!(matches!(err, ApiError::InvalidUrl(_)), "got {err:?}");
    }

    /// `safe_join` only sees the first URL, so a same-origin endpoint could
    /// otherwise bounce a write-capable request anywhere. The raw client stops
    /// at the redirect and hands the 3xx back instead of following it.
    #[tokio::test]
    async fn test_request_raw_does_not_follow_a_cross_origin_redirect() {
        let evil = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("pwned"))
            .expect(0)
            .mount(&evil)
            .await;

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/api/3/bounce"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/steal", evil.uri()).as_str()),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri())
            .unwrap()
            .with_basic_auth("dev@example.com", "token");
        let response = client
            .request_raw(RawRequest {
                method: Method::POST,
                path: "/rest/api/3/bounce",
                headers: HeaderMap::new(),
                body: Some(b"{}"),
                timeout: None,
            })
            .await
            .unwrap();

        assert_eq!(response.status, 307);
        assert!(response.header("location").unwrap().contains("/steal"));
        assert_ne!(response.body, b"pwned".to_vec());
    }

    /// Same-origin redirects are still followed, so ordinary endpoints work.
    #[tokio::test]
    async fn test_request_raw_follows_a_same_origin_redirect() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/from"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/to"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/to"))
            .respond_with(ResponseTemplate::new(200).set_body_string("arrived"))
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        let response = client
            .request_raw(RawRequest {
                method: Method::GET,
                path: "/from",
                headers: HeaderMap::new(),
                body: None,
                timeout: None,
            })
            .await
            .unwrap();

        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"arrived".to_vec());
    }

    /// Attachment downloads depend on the cross-host hop to the media host, so
    /// the ordinary client must keep following redirects.
    #[tokio::test]
    async fn test_get_bytes_still_follows_cross_host_redirects() {
        let media = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file/binary"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"BYTES".to_vec()))
            .mount(&media)
            .await;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/content/1"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/file/binary", media.uri()).as_str()),
            )
            .mount(&server)
            .await;

        let client = ApiClient::new(server.uri()).unwrap();
        assert_eq!(client.get_bytes("/content/1").await.unwrap(), b"BYTES");
    }

    #[test]
    fn test_same_origin_compares_scheme_host_and_port() {
        let base = Url::parse("https://site.atlassian.net").unwrap();
        assert!(same_origin(
            &Url::parse("https://site.atlassian.net/x").unwrap(),
            &base
        ));
        // 443 is the known default for https, so an explicit port still matches.
        assert!(same_origin(
            &Url::parse("https://site.atlassian.net:443/x").unwrap(),
            &base
        ));
        for other in [
            "https://site.atlassian.net:8443/x",
            "http://site.atlassian.net/x",
            "https://evil.example.com/x",
        ] {
            assert!(
                !same_origin(&Url::parse(other).unwrap(), &base),
                "{other} must not match"
            );
        }
    }
}
