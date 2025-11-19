use crate::error::{ApiError, Result};
use backoff::{backoff::Backoff, ExponentialBackoff};
use std::time::Duration;
use tracing::{debug, warn};

#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_interval: Duration,
    pub max_interval: Duration,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_interval: Duration::from_millis(500),
            max_interval: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    pub fn backoff(&self) -> ExponentialBackoff {
        ExponentialBackoff {
            current_interval: self.initial_interval,
            initial_interval: self.initial_interval,
            randomization_factor: 0.1,
            multiplier: self.multiplier,
            max_interval: self.max_interval,
            max_elapsed_time: None,
            ..Default::default()
        }
    }
}

pub async fn retry_with_backoff<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut backoff = config.backoff();
    let mut attempts = 0;

    loop {
        attempts += 1;
        debug!(attempt = attempts, "Executing request");

        match operation().await {
            Ok(result) => {
                if attempts > 1 {
                    debug!(attempts, "Request succeeded after retries");
                }
                return Ok(result);
            }
            Err(err) if err.is_retryable() && attempts < config.max_retries => {
                if let Some(wait) = backoff.next_backoff() {
                    warn!(
                        error = %err,
                        attempt = attempts,
                        wait_ms = wait.as_millis(),
                        "Request failed, retrying"
                    );
                    tokio::time::sleep(wait).await;
                } else {
                    return Err(ApiError::Timeout { attempts });
                }
            }
            Err(err) => {
                if attempts >= config.max_retries {
                    warn!(attempts, "Max retries exceeded");
                }
                return Err(err);
            }
        }
    }
}
