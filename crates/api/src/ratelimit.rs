use chrono::{DateTime, Utc};
use reqwest::Response;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<RateLimitState>>,
}

struct RateLimitState {
    remaining: Option<u32>,
    reset_at: Option<DateTime<Utc>>,
    limit: Option<u32>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RateLimitState {
                remaining: None,
                reset_at: None,
                limit: None,
            })),
        }
    }

    pub async fn update_from_response(&self, response: &Response) {
        let mut state = self.state.lock().await;

        if let Some(limit) = response.headers().get("x-ratelimit-limit") {
            if let Ok(s) = limit.to_str() {
                if let Ok(val) = s.parse() {
                    state.limit = Some(val);
                }
            }
        }

        if let Some(remaining) = response.headers().get("x-ratelimit-remaining") {
            if let Ok(s) = remaining.to_str() {
                if let Ok(val) = s.parse() {
                    state.remaining = Some(val);
                }
            }
        }

        if let Some(reset) = response.headers().get("x-ratelimit-reset") {
            if let Ok(s) = reset.to_str() {
                if let Ok(timestamp) = s.parse::<i64>() {
                    if let Some(dt) = DateTime::from_timestamp(timestamp, 0) {
                        state.reset_at = Some(dt);
                    }
                }
            }
        }

        if let (Some(remaining), Some(limit)) = (state.remaining, state.limit) {
            let usage_pct = ((limit - remaining) as f64 / limit as f64) * 100.0;
            if usage_pct > 80.0 {
                warn!(
                    remaining,
                    limit,
                    usage_pct = format!("{:.1}%", usage_pct),
                    "Rate limit usage high"
                );
            }
        }
    }

    pub async fn check_limit(&self) -> Option<u64> {
        let state = self.state.lock().await;

        if let Some(remaining) = state.remaining {
            if remaining == 0 {
                if let Some(reset_at) = state.reset_at {
                    let now = Utc::now();
                    if reset_at > now {
                        let wait_secs = (reset_at - now).num_seconds() as u64;
                        debug!(wait_secs, "Rate limit exceeded, need to wait");
                        return Some(wait_secs);
                    }
                }
            }
        }

        None
    }

    pub async fn get_info(&self) -> RateLimitInfo {
        let state = self.state.lock().await;
        RateLimitInfo {
            limit: state.limit,
            remaining: state.remaining,
            reset_at: state.reset_at,
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset_at: Option<DateTime<Utc>>,
}
