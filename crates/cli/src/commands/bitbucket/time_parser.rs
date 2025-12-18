use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};

/// Parse time expressions into ISO8601/RFC3339 format for Bitbucket API
///
/// Supports:
/// - Relative: "24h", "7d", "1w", "30d" (calculated from current UTC time)
/// - ISO8601 with timezone: "2024-01-01T10:00:00Z"
/// - ISO8601 date only: "2024-01-01" (assumes 00:00:00 UTC)
///
/// All timestamps are in UTC. Relative durations calculate from Utc::now().
pub fn parse_time_expression(expr: &str) -> Result<String> {
    // Try parsing as relative duration first
    if let Some(result) = try_parse_relative(expr)? {
        return Ok(result);
    }

    // Try parsing as ISO8601 datetime with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(expr) {
        return Ok(dt.to_rfc3339());
    }

    // Try parsing as ISO8601 date only (assume UTC 00:00:00)
    if let Ok(date) = chrono::NaiveDate::parse_from_str(expr, "%Y-%m-%d") {
        let datetime = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow!("Invalid date: {}", expr))?;
        let utc_datetime = DateTime::<Utc>::from_naive_utc_and_offset(datetime, Utc);
        return Ok(utc_datetime.to_rfc3339());
    }

    Err(anyhow!(
        "Invalid time expression: '{}'\n\
        Supported formats:\n\
        - Relative: 24h, 7d, 1w, 30d\n\
        - ISO8601 with timezone: 2024-01-01T10:00:00Z\n\
        - ISO8601 date only: 2024-01-01 (assumes 00:00:00 UTC)",
        expr
    ))
}

/// Try parsing as relative duration (e.g., "24h", "7d", "1w")
/// Returns None if not a relative duration format
/// Returns Some(Err) if format matches but value is invalid
/// Returns Some(Ok(timestamp)) if successfully parsed
fn try_parse_relative(expr: &str) -> Result<Option<String>> {
    let expr = expr.trim();

    // Must have at least 2 chars (e.g., "1h")
    if expr.len() < 2 {
        return Ok(None);
    }

    let unit = expr.chars().last().unwrap();
    let value_str = &expr[..expr.len() - 1];

    // If it doesn't match the pattern, it's not a relative duration
    if !matches!(unit, 'h' | 'd' | 'w') {
        return Ok(None);
    }

    // Parse the numeric value
    let value: i64 = value_str.parse().map_err(|_| {
        anyhow!("Invalid relative duration: '{}' - numeric part must be an integer", expr)
    })?;

    // Reject zero or negative durations
    if value <= 0 {
        return Err(anyhow!(
            "Invalid relative duration: '{}' - duration must be positive (greater than 0)",
            expr
        ));
    }

    let duration = match unit {
        'h' => Duration::hours(value),
        'd' => Duration::days(value),
        'w' => Duration::weeks(value),
        _ => unreachable!(), // Already validated above
    };

    let now = Utc::now();
    let target = now - duration;
    Ok(Some(target.to_rfc3339()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_relative_hours() {
        let result = parse_time_expression("24h");
        assert!(result.is_ok());
        // Just verify it's a valid RFC3339 timestamp
        assert!(DateTime::parse_from_rfc3339(&result.unwrap()).is_ok());
    }

    #[test]
    fn test_parse_relative_days() {
        let result = parse_time_expression("7d");
        assert!(result.is_ok());
        assert!(DateTime::parse_from_rfc3339(&result.unwrap()).is_ok());
    }

    #[test]
    fn test_parse_relative_weeks() {
        let result = parse_time_expression("2w");
        assert!(result.is_ok());
        assert!(DateTime::parse_from_rfc3339(&result.unwrap()).is_ok());
    }

    #[test]
    fn test_parse_iso8601_with_timezone() {
        let result = parse_time_expression("2024-01-01T10:00:00Z").unwrap();
        assert_eq!(result, "2024-01-01T10:00:00+00:00");
    }

    #[test]
    fn test_parse_iso8601_with_offset() {
        let result = parse_time_expression("2024-01-01T10:00:00+08:00").unwrap();
        // Should preserve the timezone offset
        assert!(result.contains("2024-01-01T10:00:00+08:00"));
    }

    #[test]
    fn test_parse_date_only_assumes_utc_midnight() {
        let result = parse_time_expression("2024-01-01").unwrap();
        assert_eq!(result, "2024-01-01T00:00:00+00:00");
    }

    #[test]
    fn test_reject_zero_duration() {
        let result = parse_time_expression("0h");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));
    }

    #[test]
    fn test_reject_negative_duration() {
        let result = parse_time_expression("-1d");
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_invalid_format() {
        let result = parse_time_expression("not-a-time");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid time expression"));
    }

    #[test]
    fn test_reject_invalid_unit() {
        let result = parse_time_expression("5m");
        assert!(result.is_err());
    }

    #[test]
    fn test_reject_non_numeric_value() {
        let result = parse_time_expression("abch");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("numeric part must be an integer"));
    }

    #[test]
    fn test_empty_string() {
        let result = parse_time_expression("");
        assert!(result.is_err());
    }

    #[test]
    fn test_whitespace_handling() {
        let result = parse_time_expression(" 24h ");
        assert!(result.is_ok());
    }

    #[test]
    fn test_large_duration() {
        let result = parse_time_expression("365d");
        assert!(result.is_ok());
        assert!(DateTime::parse_from_rfc3339(&result.unwrap()).is_ok());
    }

    #[test]
    fn test_relative_duration_calculation() {
        // Mock time for reproducible test
        // We can't easily mock Utc::now(), but we can verify the result is in the past
        let result = parse_time_expression("1h").unwrap();
        let parsed = DateTime::parse_from_rfc3339(&result).unwrap();
        let now = Utc::now();

        // The parsed time should be approximately 1 hour ago (allowing small test execution time)
        let diff = now.signed_duration_since(parsed);
        assert!(diff.num_hours() >= 0);
        assert!(diff.num_hours() <= 1);
        // Should be close to 1 hour (within 1 second tolerance for test execution)
        assert!(diff.num_seconds() >= 3599);
        assert!(diff.num_seconds() <= 3601);
    }
}
