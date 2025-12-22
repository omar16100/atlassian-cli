pub mod error;
pub mod pagination;
pub mod ratelimit;
pub mod retry;

use error::{ApiError, Result};
use ratelimit::RateLimiter;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use retry::{retry_with_backoff, RetryConfig};
use secrecy::{ExposeSecret, Secret};
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
        token: Secret<String>,
    },
    Bearer {
        token: Secret<String>,
    },
}

impl fmt::Debug for AuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthMethod::Basic { username, .. } => {
                f.debug_struct("Basic")
                    .field("username", username)
                    .field("token", &"[REDACTED]")
                    .finish()
            }
            AuthMethod::Bearer { .. } => {
                f.debug_struct("Bearer")
                    .field("token", &"[REDACTED]")
                    .finish()
            }
        }
    }
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
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
            let is_localhost = url.host_str()
                .map(|h| h == "localhost" || h == "127.0.0.1" || h.starts_with("127."))
                .unwrap_or(false);

            if !is_localhost {
                return Err(ApiError::InvalidUrl(url::ParseError::InvalidDomainCharacter));
            }
        }

        let client = Client::builder()
            .user_agent(format!("atlassian-cli/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ApiError::RequestFailed)?;

        Ok(Self {
            client,
            base_url: url,
            auth: None,
            retry_config: RetryConfig::default(),
            rate_limiter: RateLimiter::new(),
        })
    }

    /// Safely join a path to the base URL, ensuring scheme and host remain unchanged
    /// to prevent SSRF attacks.
    fn safe_join(&self, path: &str) -> Result<Url> {
        let joined = self.base_url
            .join(path.strip_prefix('/').unwrap_or(path))
            .map_err(ApiError::InvalidUrl)?;

        // Validate that scheme and host haven't changed (SSRF protection)
        if joined.scheme() != self.base_url.scheme() || joined.host() != self.base_url.host() {
            return Err(ApiError::InvalidUrl(url::ParseError::InvalidDomainCharacter));
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
            token: Secret::new(token.into()),
        });
        self
    }

    pub fn with_bearer_token(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(AuthMethod::Bearer {
            token: Secret::new(token.into()),
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
            req = req.header("Accept", "text/plain; charset=utf-8");

            let response = req.send().await.map_err(ApiError::RequestFailed)?;

            self.rate_limiter.update_from_response(&response).await;

            let status = response.status();

            match status {
                StatusCode::UNAUTHORIZED => Err(ApiError::AuthenticationFailed {
                    message: "Invalid or expired credentials".to_string(),
                }),
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
                status if status.is_success() => response.json::<T>().await.map_err(|e| {
                    error!("Failed to parse JSON response: {}", e);
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

    pub fn apply_auth(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.auth {
            Some(AuthMethod::Basic { username, token }) => {
                request.basic_auth(username, Some(token.expose_secret()))
            }
            Some(AuthMethod::Bearer { token }) => request.bearer_auth(token.expose_secret()),
            None => request,
        }
    }

    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }
}
