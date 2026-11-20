use reqwest::header::HeaderMap;
use reqwest::Response;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, info, warn};

#[derive(Clone)]
pub struct RateLimiter {
    state: Arc<Mutex<RateLimitState>>,
}

/// The largest and the smallest `x-ratelimit-reset` values that are accepted,
/// in seconds from the Unix epoch: the last second of the year 262142 and the
/// first second of the year -262143. These are the limits that the client had
/// before, so the accepted values do not change.
const MAX_RESET_EPOCH_SECS: i64 = 8_210_266_876_799;
const MIN_RESET_EPOCH_SECS: i64 = -8_334_601_228_800;

struct RateLimitState {
    remaining: Option<u32>,
    /// The reset time in seconds from the Unix epoch.
    reset_at: Option<i64>,
    limit: Option<u32>,
}

/// The current time as whole seconds from the Unix epoch (rounded down) and
/// the nanoseconds after that second.
fn unix_now() -> (i64, u32) {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since_epoch) => (
            i64::try_from(since_epoch.as_secs()).unwrap_or(i64::MAX),
            since_epoch.subsec_nanos(),
        ),
        // The system clock is before 1970.
        Err(error) => {
            let before_epoch = error.duration();
            let secs = i64::try_from(before_epoch.as_secs()).unwrap_or(i64::MAX);
            match before_epoch.subsec_nanos() {
                0 => (-secs, 0),
                nanos => (-secs - 1, 1_000_000_000 - nanos),
            }
        }
    }
}

/// The `SystemTime` of a count of seconds from the Unix epoch, or `None` when
/// the platform cannot represent that time.
fn system_time_from_epoch_secs(secs: i64) -> Option<SystemTime> {
    let distance = Duration::from_secs(secs.unsigned_abs());
    if secs >= 0 {
        UNIX_EPOCH.checked_add(distance)
    } else {
        UNIX_EPOCH.checked_sub(distance)
    }
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
        self.update_from_headers(response.headers()).await;
    }

    /// Read the rate limit headers. A test can call this with no HTTP response.
    async fn update_from_headers(&self, headers: &HeaderMap) {
        let Ok(mut state) = timeout(Duration::from_secs(5), self.state.lock()).await else {
            warn!("Rate limiter lock timeout, skipping update");
            return;
        };

        if let Some(limit) = headers.get("x-ratelimit-limit") {
            if let Ok(s) = limit.to_str() {
                if let Ok(val) = s.parse() {
                    state.limit = Some(val);
                }
            }
        }

        if let Some(remaining) = headers.get("x-ratelimit-remaining") {
            if let Ok(s) = remaining.to_str() {
                if let Ok(val) = s.parse() {
                    state.remaining = Some(val);
                }
            }
        }

        if let Some(reset) = headers.get("x-ratelimit-reset") {
            if let Ok(s) = reset.to_str() {
                if let Ok(timestamp) = s.parse::<i64>() {
                    if (MIN_RESET_EPOCH_SECS..=MAX_RESET_EPOCH_SECS).contains(&timestamp) {
                        state.reset_at = Some(timestamp);
                    } else {
                        debug!(timestamp, "Rate limit reset time is out of range, ignored");
                    }
                }
            }
        }

        if let (Some(remaining), Some(limit)) = (state.remaining, state.limit) {
            let usage_pct = ((limit - remaining) as f64 / limit as f64) * 100.0;
            if usage_pct > 80.0 {
                // Not a warning: nothing is wrong yet, and this fires for
                // each response of a successful command. The wait itself has
                // its own warning in the client.
                info!(
                    remaining,
                    limit,
                    usage_pct = format!("{:.1}%", usage_pct),
                    "Rate limit usage high"
                );
            }
        }
    }

    pub async fn check_limit(&self) -> Option<u64> {
        let (now_secs, now_nanos) = unix_now();
        self.check_limit_at(now_secs, now_nanos).await
    }

    /// The wait for a given current time: whole seconds from the Unix epoch
    /// (rounded down) and the nanoseconds after that second. A test can call
    /// this with a fixed time.
    async fn check_limit_at(&self, now_secs: i64, now_nanos: u32) -> Option<u64> {
        let Ok(state) = timeout(Duration::from_secs(5), self.state.lock()).await else {
            warn!("Rate limiter lock timeout, skipping check");
            return None;
        };

        if let Some(remaining) = state.remaining {
            if remaining == 0 {
                if let Some(reset_at) = state.reset_at {
                    // The reset time is a whole second. It is in the future
                    // only when it is after the current whole second.
                    if reset_at > now_secs {
                        // A part of a second is dropped, not rounded.
                        let started_second = i64::from(now_nanos > 0);
                        let wait_secs = (reset_at.saturating_sub(now_secs) - started_second) as u64;
                        debug!(wait_secs, "Rate limit exceeded, need to wait");
                        return Some(wait_secs);
                    }
                }
            }
        }

        None
    }

    pub async fn get_info(&self) -> RateLimitInfo {
        let Ok(state) = timeout(Duration::from_secs(5), self.state.lock()).await else {
            warn!("Rate limiter lock timeout, returning empty info");
            return RateLimitInfo {
                limit: None,
                remaining: None,
                reset_at: None,
            };
        };
        RateLimitInfo {
            limit: state.limit,
            remaining: state.remaining,
            reset_at: state.reset_at.and_then(system_time_from_epoch_secs),
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
    /// The time at which the limit resets. It is `None` when no reset time is
    /// known, or when the platform cannot represent the time as a
    /// `SystemTime`.
    pub reset_at: Option<SystemTime>,
}

#[cfg(test)]
mod tests {
    //! These tests pin the reset time handling. The expected values were
    //! recorded with `chrono` and did not change when `chrono` was removed.
    //! Only the helpers `reset_epoch_secs` and `wait_at` know the time types.

    use super::*;
    use reqwest::header::HeaderValue;

    /// The largest and the smallest `x-ratelimit-reset` values that are
    /// accepted (the last second of the year 262142 and the first second of
    /// the year -262143).
    const MAX_RESET: i64 = 8_210_266_876_799;
    const MIN_RESET: i64 = -8_334_601_228_800;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    /// The stored reset time, in seconds from the Unix epoch.
    fn reset_epoch_secs(info: &RateLimitInfo) -> Option<i64> {
        info.reset_at
            .map(|time| match time.duration_since(UNIX_EPOCH) {
                Ok(since_epoch) => since_epoch.as_secs() as i64,
                Err(error) => -(error.duration().as_secs() as i64),
            })
    }

    /// The wait that `check_limit` gives at a fixed current time.
    async fn wait_at(limiter: &RateLimiter, now_secs: i64, now_nanos: u32) -> Option<u64> {
        limiter.check_limit_at(now_secs, now_nanos).await
    }

    /// A limiter with no requests left and the given reset header.
    async fn exhausted(reset: &str) -> RateLimiter {
        let limiter = RateLimiter::new();
        limiter
            .update_from_headers(&headers(&[
                ("x-ratelimit-limit", "100"),
                ("x-ratelimit-remaining", "0"),
                ("x-ratelimit-reset", reset),
            ]))
            .await;
        limiter
    }

    #[tokio::test]
    async fn a_valid_reset_time_gives_the_wait_in_whole_seconds() {
        let limiter = exhausted("1700000100").await;
        let info = limiter.get_info().await;
        assert_eq!(info.limit, Some(100));
        assert_eq!(info.remaining, Some(0));
        assert_eq!(reset_epoch_secs(&info), Some(1_700_000_100));

        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, Some(100));
        // A part of a second is dropped, not rounded.
        assert_eq!(wait_at(&limiter, 1_700_000_000, 1).await, Some(99));
        assert_eq!(
            wait_at(&limiter, 1_700_000_000, 500_000_000).await,
            Some(99)
        );
        assert_eq!(wait_at(&limiter, 1_700_000_099, 0).await, Some(1));
        assert_eq!(wait_at(&limiter, 1_700_000_099, 999_999_999).await, Some(0));
    }

    #[tokio::test]
    async fn a_reset_time_that_is_not_in_the_future_gives_no_wait() {
        let limiter = exhausted("1700000100").await;
        assert_eq!(wait_at(&limiter, 1_700_000_100, 0).await, None);
        assert_eq!(wait_at(&limiter, 1_700_000_100, 1).await, None);
        assert_eq!(wait_at(&limiter, 1_800_000_000, 0).await, None);
    }

    #[tokio::test]
    async fn a_zero_reset_time_is_stored_and_gives_no_wait() {
        let limiter = exhausted("0").await;
        assert_eq!(reset_epoch_secs(&limiter.get_info().await), Some(0));
        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);
        // The epoch is in the future for a clock before 1970.
        assert_eq!(wait_at(&limiter, -10, 0).await, Some(10));
        assert_eq!(wait_at(&limiter, -10, 250_000_000).await, Some(9));
    }

    #[tokio::test]
    async fn a_negative_reset_time_is_stored_and_gives_no_wait() {
        let limiter = exhausted("-5").await;
        assert_eq!(reset_epoch_secs(&limiter.get_info().await), Some(-5));
        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);
        assert_eq!(wait_at(&limiter, 0, 0).await, None);
    }

    #[tokio::test]
    async fn a_very_large_reset_time_is_stored_and_gives_a_long_wait() {
        // A value in milliseconds is read as seconds.
        let limiter = exhausted("1700000000000").await;
        assert_eq!(
            reset_epoch_secs(&limiter.get_info().await),
            Some(1_700_000_000_000)
        );
        assert_eq!(
            wait_at(&limiter, 1_700_000_000, 0).await,
            Some(1_698_300_000_000)
        );
    }

    #[tokio::test]
    async fn the_accepted_range_has_fixed_limits() {
        let limiter = exhausted(&MAX_RESET.to_string()).await;
        assert_eq!(reset_epoch_secs(&limiter.get_info().await), Some(MAX_RESET));
        assert_eq!(
            wait_at(&limiter, 1_700_000_000, 0).await,
            Some(8_208_566_876_799)
        );

        let limiter = exhausted(&MIN_RESET.to_string()).await;
        assert_eq!(reset_epoch_secs(&limiter.get_info().await), Some(MIN_RESET));
        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);
    }

    #[tokio::test]
    async fn a_reset_time_out_of_range_keeps_the_last_value() {
        for bad in [
            (MAX_RESET + 1).to_string(),
            (MIN_RESET - 1).to_string(),
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            "9223372036854775808".to_string(),
        ] {
            let limiter = exhausted("1700000100").await;
            limiter
                .update_from_headers(&headers(&[("x-ratelimit-reset", &bad)]))
                .await;
            assert_eq!(
                reset_epoch_secs(&limiter.get_info().await),
                Some(1_700_000_100),
                "reset header {bad:?}"
            );

            let limiter = exhausted(&bad).await;
            assert_eq!(
                reset_epoch_secs(&limiter.get_info().await),
                None,
                "reset header {bad:?}"
            );
            assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);
        }
    }

    #[tokio::test]
    async fn a_reset_header_that_is_not_an_integer_keeps_the_last_value() {
        for bad in [
            "",
            "abc",
            "1.5",
            " 1700000200",
            "1700000200 ",
            "1700000200s",
        ] {
            let limiter = exhausted("1700000100").await;
            limiter
                .update_from_headers(&headers(&[("x-ratelimit-reset", bad)]))
                .await;
            assert_eq!(
                reset_epoch_secs(&limiter.get_info().await),
                Some(1_700_000_100),
                "reset header {bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_reset_header_with_a_plus_sign_is_an_integer() {
        let limiter = exhausted("+1700000200").await;
        assert_eq!(
            reset_epoch_secs(&limiter.get_info().await),
            Some(1_700_000_200)
        );
    }

    #[tokio::test]
    async fn there_is_no_wait_when_requests_remain_or_no_reset_time_is_known() {
        let limiter = RateLimiter::new();
        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);
        assert_eq!(reset_epoch_secs(&limiter.get_info().await), None);

        limiter
            .update_from_headers(&headers(&[("x-ratelimit-remaining", "0")]))
            .await;
        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);

        limiter
            .update_from_headers(&headers(&[
                ("x-ratelimit-remaining", "1"),
                ("x-ratelimit-reset", "1700000100"),
            ]))
            .await;
        assert_eq!(wait_at(&limiter, 1_700_000_000, 0).await, None);
    }
}
