use crate::error::Result;
use std::time::Duration;
use tracing::{debug, warn};

use backoff::ExponentialBackoff;

/// Retry policy shared by the typed request helpers and the raw passthrough.
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Total attempts, the first try included. Values of 0 and 1 both mean a
    /// single attempt: the first try always runs.
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
    /// The delay generator the client uses between attempts.
    pub fn delays(&self) -> RetryDelays {
        RetryDelays::new(self)
    }

    /// The same policy as a `backoff::ExponentialBackoff`.
    ///
    /// Kept for callers that still hold one. The client no longer uses it, and
    /// the `backoff` crate is unmaintained (RUSTSEC-2025-0012), so this method
    /// and the `backoff` dependency go away in the next breaking release.
    #[deprecated(
        since = "0.10.1",
        note = "use RetryConfig::delays(); backoff() is removed in the next breaking release"
    )]
    pub fn backoff(&self) -> ExponentialBackoff {
        ExponentialBackoff {
            current_interval: self.initial_interval,
            initial_interval: self.initial_interval,
            randomization_factor: RetryDelays::JITTER,
            multiplier: self.multiplier,
            max_interval: self.max_interval,
            max_elapsed_time: None,
            ..Default::default()
        }
    }
}

/// Exponential delays with jitter.
///
/// Each call returns the current interval spread uniformly over plus or minus
/// [`RetryDelays::JITTER`], then multiplies the interval and caps it at
/// `max_interval`. The cap applies before the jitter, so a 30 s cap can yield
/// up to 33 s. There is no elapsed-time cutoff: the caller decides when to stop.
/// The policy matches the `backoff::ExponentialBackoff` settings the client used
/// before (same first interval, growth, cap and jitter range). The arithmetic is
/// not bit-identical: this generator works in seconds as `f64` and rounds to the
/// nearest nanosecond, where `backoff` worked in nanoseconds and truncated, so
/// individual delays can differ by a nanosecond.
#[derive(Clone, Debug)]
pub struct RetryDelays {
    current: Duration,
    multiplier: f64,
    max_interval: Duration,
}

impl RetryDelays {
    /// Spread applied to every delay, as a fraction of the interval.
    pub const JITTER: f64 = 0.1;

    fn new(config: &RetryConfig) -> Self {
        Self {
            current: config.initial_interval,
            multiplier: config.multiplier,
            max_interval: config.max_interval,
        }
    }

    /// The next delay to sleep before another attempt.
    pub fn next_delay(&mut self) -> Duration {
        let delay = jittered(self.current, Self::JITTER, fastrand::f64());
        self.current = grow(self.current, self.multiplier, self.max_interval);
        delay
    }
}

/// `base` spread over `[base * (1 - factor), base * (1 + factor)]` at position
/// `unit` in `[0, 1)`.
fn jittered(base: Duration, factor: f64, unit: f64) -> Duration {
    let secs = base.as_secs_f64();
    let low = secs * (1.0 - factor);
    let high = secs * (1.0 + factor);
    Duration::try_from_secs_f64(low + unit * (high - low)).unwrap_or(base)
}

/// `current * multiplier`, capped at `max`. Overflow and non-finite values
/// land on `max`.
fn grow(current: Duration, multiplier: f64, max: Duration) -> Duration {
    let next = current.as_secs_f64() * multiplier;
    if !next.is_finite() {
        return max;
    }
    Duration::try_from_secs_f64(next)
        .map(|next| next.min(max))
        .unwrap_or(max)
}

/// Runs `operation` until it succeeds, returns a non-retryable error, or
/// `config.max_retries` attempts are used up. The last error is returned as is.
pub async fn retry_with_backoff<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut delays = config.delays();
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
                let wait = delays.next_delay();
                warn!(
                    error = %err,
                    attempt = attempts,
                    wait_ms = wait.as_millis(),
                    "Request failed, retrying"
                );
                tokio::time::sleep(wait).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ApiError;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn config(initial_ms: u64, max_ms: u64, multiplier: f64) -> RetryConfig {
        RetryConfig {
            max_retries: 3,
            initial_interval: Duration::from_millis(initial_ms),
            max_interval: Duration::from_millis(max_ms),
            multiplier,
        }
    }

    #[test]
    fn jittered_spans_plus_minus_the_factor() {
        let base = Duration::from_millis(1000);
        assert_eq!(jittered(base, 0.1, 0.0), Duration::from_millis(900));
        assert_eq!(jittered(base, 0.1, 0.5), Duration::from_millis(1000));
        let top = jittered(base, 0.1, 0.999_999);
        assert!(top > Duration::from_millis(1099) && top < Duration::from_millis(1100));
    }

    #[test]
    fn grow_multiplies_then_caps() {
        let max = Duration::from_millis(30_000);
        assert_eq!(
            grow(Duration::from_millis(500), 2.0, max),
            Duration::from_millis(1000)
        );
        assert_eq!(grow(Duration::from_millis(16_000), 2.0, max), max);
        assert_eq!(grow(max, 2.0, max), max);
    }

    #[test]
    fn grow_lands_on_max_when_the_product_is_not_representable() {
        let max = Duration::from_secs(30);
        assert_eq!(grow(Duration::from_secs(1), f64::INFINITY, max), max);
        assert_eq!(grow(Duration::from_secs(1), f64::NAN, max), max);
        assert_eq!(grow(Duration::MAX, 2.0, max), max);
    }

    #[test]
    fn first_delay_is_the_initial_interval_within_jitter() {
        let mut delays = config(500, 30_000, 2.0).delays();
        let first = delays.next_delay();
        assert!(first >= Duration::from_millis(450), "{first:?}");
        assert!(first <= Duration::from_millis(550), "{first:?}");
    }

    #[test]
    fn delays_grow_by_the_multiplier_and_stay_under_the_jittered_cap() {
        let mut delays = config(500, 2_000, 2.0).delays();
        let expected_bases = [500u64, 1000, 2000, 2000, 2000];
        for base in expected_bases {
            let delay = delays.next_delay();
            let low = Duration::from_millis(base * 9 / 10);
            let high = Duration::from_millis(base * 11 / 10);
            assert!(
                (low..=high).contains(&delay),
                "delay {delay:?}, base {base} ms"
            );
        }
    }

    #[tokio::test]
    async fn retry_stops_after_max_retries_attempts() {
        let calls = AtomicUsize::new(0);
        let cfg = config(0, 0, 2.0);
        let result: Result<()> = retry_with_backoff(&cfg, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(ApiError::ServerError {
                status: 503,
                message: "down".into(),
            })
        })
        .await;
        assert!(matches!(
            result,
            Err(ApiError::ServerError { status: 503, .. })
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_returns_the_first_success() {
        let calls = AtomicUsize::new(0);
        let cfg = config(0, 0, 2.0);
        let result = retry_with_backoff(&cfg, || async {
            let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n < 2 {
                Err(ApiError::RateLimitExceeded { retry_after: 0 })
            } else {
                Ok(n)
            }
        })
        .await;
        assert_eq!(result.unwrap(), 2);
    }

    #[tokio::test]
    async fn retry_does_not_repeat_a_non_retryable_error() {
        let calls = AtomicUsize::new(0);
        let cfg = config(0, 0, 2.0);
        let result: Result<()> = retry_with_backoff(&cfg, || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(ApiError::AuthenticationFailed {
                message: "no".into(),
            })
        })
        .await;
        assert!(matches!(result, Err(ApiError::AuthenticationFailed { .. })));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[allow(deprecated)]
    fn deprecated_backoff_keeps_the_old_settings() {
        let b = config(250, 12_000, 3.0).backoff();
        assert_eq!(b.initial_interval, Duration::from_millis(250));
        assert_eq!(b.current_interval, Duration::from_millis(250));
        assert_eq!(b.max_interval, Duration::from_millis(12_000));
        assert_eq!(b.multiplier, 3.0);
        assert_eq!(b.randomization_factor, RetryDelays::JITTER);
        assert_eq!(b.max_elapsed_time, None);
    }

    #[test]
    fn jitter_upper_endpoint_is_reached_by_rounding_only() {
        // The ideal range is [0.9 * base, 1.1 * base). `1.0 - f64::EPSILON` is
        // the largest value fastrand::f64() can return (not the largest f64
        // below 1), and it rounds to the endpoint in nanoseconds.
        let base = Duration::from_millis(500);
        let top = jittered(base, 0.1, 1.0 - f64::EPSILON);
        assert_eq!(top, Duration::from_millis(550));
        assert_eq!(jittered(base, 0.1, 0.0), Duration::from_millis(450));
    }

    #[test]
    fn an_initial_interval_above_the_cap_is_used_once_then_capped() {
        let mut delays = config(5_000, 1_000, 2.0).delays();
        let first = delays.next_delay();
        assert!(first >= Duration::from_millis(4_500) && first <= Duration::from_millis(5_500));
        let second = delays.next_delay();
        assert!(second >= Duration::from_millis(900) && second <= Duration::from_millis(1_100));
    }

    #[test]
    fn fractional_growth_rounds_to_the_nearest_nanosecond() {
        let max = Duration::from_secs(1);
        assert_eq!(
            grow(Duration::from_nanos(1), 1.5, max),
            Duration::from_nanos(2)
        );
        assert_eq!(
            grow(Duration::from_nanos(10), 1.5, max),
            Duration::from_nanos(15)
        );
    }

    #[tokio::test]
    async fn max_retries_zero_and_one_both_allow_a_single_attempt() {
        for max_retries in [0usize, 1] {
            let calls = AtomicUsize::new(0);
            let cfg = RetryConfig {
                max_retries,
                ..config(0, 0, 2.0)
            };
            let result: Result<()> = retry_with_backoff(&cfg, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(ApiError::ServerError {
                    status: 502,
                    message: "bad gateway".into(),
                })
            })
            .await;
            assert!(result.is_err());
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "max_retries = {max_retries}"
            );
        }
    }
}
