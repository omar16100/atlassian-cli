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
    parse_time_expression_at(expr, Utc::now())
}

/// `parse_time_expression` for a given current time. A test can call this with
/// a fixed time.
fn parse_time_expression_at(expr: &str, now: DateTime<Utc>) -> Result<String> {
    // Try parsing as relative duration first
    if let Some(result) = try_parse_relative(expr, now)? {
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
fn try_parse_relative(expr: &str, now: DateTime<Utc>) -> Result<Option<String>> {
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
        anyhow!(
            "Invalid relative duration: '{}' - numeric part must be an integer",
            expr
        )
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid time expression"));
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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("numeric part must be an integer"));
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

#[cfg(test)]
mod pinned_tests {
    //! These tests pin the accepted inputs, the rejected inputs and the exact
    //! output strings. The output goes into a Bitbucket query, so its precision
    //! and its offset form (`+00:00`, never `Z`) must not change.
    //!
    //! The expected values were recorded with `chrono` and did not change when
    //! `chrono` was removed. Only the helper `at` knows the time types.

    use super::*;

    const NOW: &str = "2026-11-20T12:34:56.123456789Z";

    const INVALID_EXPRESSION_HELP: &str = "Supported formats:\n\
        - Relative: 24h, 7d, 1w, 30d\n\
        - ISO8601 with timezone: 2024-01-01T10:00:00Z\n\
        - ISO8601 date only: 2024-01-01 (assumes 00:00:00 UTC)";

    /// Parse `expr` with the current time fixed at the RFC 3339 time `now`.
    fn at(expr: &str, now: &str) -> Result<String> {
        let now = DateTime::parse_from_rfc3339(now)
            .unwrap()
            .with_timezone(&Utc);
        parse_time_expression_at(expr, now)
    }

    fn ok(expr: &str) -> String {
        at(expr, NOW).unwrap_or_else(|e| panic!("{expr:?} must be accepted: {e}"))
    }

    fn err(expr: &str) -> String {
        match at(expr, NOW) {
            Ok(value) => panic!("{expr:?} must be rejected, but gave {value:?}"),
            Err(e) => e.to_string(),
        }
    }

    fn assert_invalid_expression(expr: &str) {
        assert_eq!(
            err(expr),
            format!("Invalid time expression: '{expr}'\n{INVALID_EXPRESSION_HELP}"),
        );
    }

    #[test]
    fn relative_expressions_subtract_from_the_current_time() {
        for (expr, expected) in [
            ("1h", "2026-11-20T11:34:56.123456789+00:00"),
            ("24h", "2026-11-19T12:34:56.123456789+00:00"),
            ("13h", "2026-11-19T23:34:56.123456789+00:00"),
            ("1d", "2026-11-19T12:34:56.123456789+00:00"),
            ("7d", "2026-11-13T12:34:56.123456789+00:00"),
            ("30d", "2026-10-21T12:34:56.123456789+00:00"),
            ("365d", "2025-11-20T12:34:56.123456789+00:00"),
            ("1w", "2026-11-13T12:34:56.123456789+00:00"),
            ("2w", "2026-11-06T12:34:56.123456789+00:00"),
            ("100000d", "1753-02-04T12:34:56.123456789+00:00"),
            // The numeric part can have a plus sign and leading zeros.
            ("+5d", "2026-11-15T12:34:56.123456789+00:00"),
            ("007d", "2026-11-13T12:34:56.123456789+00:00"),
            // White space around the expression is ignored.
            (" 24h ", "2026-11-19T12:34:56.123456789+00:00"),
            ("\t1w\n", "2026-11-13T12:34:56.123456789+00:00"),
        ] {
            assert_eq!(ok(expr), expected, "expression {expr:?}");
        }
    }

    #[test]
    fn a_relative_expression_can_give_a_year_before_year_zero() {
        // 2026-11-20 minus 800000 days.
        assert_eq!(ok("800000d"), "-0164-07-24T12:34:56.123456789+00:00");
    }

    /// The fraction has zero, three, six or nine digits. It is the shortest of
    /// these that keeps the value of the clock.
    #[test]
    fn the_fraction_of_a_relative_result_follows_the_clock_precision() {
        for (now, expected) in [
            ("2026-11-20T12:00:00Z", "2026-11-19T12:00:00+00:00"),
            ("2026-11-20T12:00:00.5Z", "2026-11-19T12:00:00.500+00:00"),
            ("2026-11-20T12:00:00.120Z", "2026-11-19T12:00:00.120+00:00"),
            (
                "2026-11-20T12:00:00.1234Z",
                "2026-11-19T12:00:00.123400+00:00",
            ),
            (
                "2026-11-20T12:00:00.123456Z",
                "2026-11-19T12:00:00.123456+00:00",
            ),
            (
                "2026-11-20T12:00:00.1234567Z",
                "2026-11-19T12:00:00.123456700+00:00",
            ),
            (
                "2026-11-20T12:00:00.000000001Z",
                "2026-11-19T12:00:00.000000001+00:00",
            ),
        ] {
            assert_eq!(at("1d", now).unwrap(), expected, "current time {now}");
        }
    }

    #[test]
    fn a_relative_expression_crosses_month_and_year_limits() {
        assert_eq!(
            at("1d", "2024-03-01T00:00:00Z").unwrap(),
            "2024-02-29T00:00:00+00:00"
        );
        assert_eq!(
            at("1d", "2023-03-01T00:00:00Z").unwrap(),
            "2023-02-28T00:00:00+00:00"
        );
        assert_eq!(
            at("1h", "2026-01-01T00:30:00Z").unwrap(),
            "2025-12-31T23:30:00+00:00"
        );
        assert_eq!(
            at("52w", "2026-01-01T00:00:00Z").unwrap(),
            "2025-01-02T00:00:00+00:00"
        );
    }

    #[test]
    fn a_relative_expression_with_a_bad_number_is_rejected() {
        for expr in [
            "abch",
            "1.5h",
            "1 0d",
            "--1d",
            "d1d",
            "9223372036854775808d",
            "\u{ff11}d",
        ] {
            assert_eq!(
                err(expr),
                format!("Invalid relative duration: '{expr}' - numeric part must be an integer"),
            );
        }
        // The message shows the expression with no white space around it.
        assert_eq!(
            err(" xh "),
            "Invalid relative duration: 'xh' - numeric part must be an integer"
        );
    }

    #[test]
    fn a_relative_expression_that_is_not_positive_is_rejected() {
        for expr in ["0h", "0d", "0w", "-1d", "-24h", "+0w", "000d"] {
            assert_eq!(
                err(expr),
                format!(
                    "Invalid relative duration: '{expr}' - duration must be positive (greater than 0)"
                ),
            );
        }
    }

    #[test]
    fn other_units_are_not_relative_expressions() {
        for expr in [
            "5m", "5s", "5y", "24H", "7D", "1W", "5", "h", "d", "", " ", "5 days",
        ] {
            assert_invalid_expression(expr);
        }
    }

    #[test]
    fn an_rfc_3339_time_keeps_its_offset_and_never_gives_z() {
        for (expr, expected) in [
            ("2024-01-01T10:00:00Z", "2024-01-01T10:00:00+00:00"),
            ("2024-01-01T10:00:00+00:00", "2024-01-01T10:00:00+00:00"),
            ("2024-01-01T10:00:00-00:00", "2024-01-01T10:00:00+00:00"),
            ("2024-01-01T10:00:00+08:00", "2024-01-01T10:00:00+08:00"),
            ("2024-01-01T10:00:00-05:30", "2024-01-01T10:00:00-05:30"),
            ("2024-01-01T10:00:00+23:59", "2024-01-01T10:00:00+23:59"),
            ("2024-01-01T10:00:00-23:59", "2024-01-01T10:00:00-23:59"),
            // The Unicode minus sign (U+2212) is accepted.
            (
                "2024-01-01T10:00:00\u{2212}05:30",
                "2024-01-01T10:00:00-05:30",
            ),
            // Lower case letters and a space separator are accepted.
            ("2024-01-01t10:00:00z", "2024-01-01T10:00:00+00:00"),
            ("2024-01-01 10:00:00Z", "2024-01-01T10:00:00+00:00"),
            ("2024-01-01 10:00:00+08:00", "2024-01-01T10:00:00+08:00"),
            // The limits of the date and the time.
            ("0000-01-01T00:00:00Z", "0000-01-01T00:00:00+00:00"),
            ("0000-01-01T00:00:00+23:59", "0000-01-01T00:00:00+23:59"),
            ("9999-12-31T23:59:59Z", "9999-12-31T23:59:59+00:00"),
            ("9999-12-31T23:59:59-23:59", "9999-12-31T23:59:59-23:59"),
            ("2024-02-29T00:00:00Z", "2024-02-29T00:00:00+00:00"),
            ("2000-02-29T00:00:00Z", "2000-02-29T00:00:00+00:00"),
        ] {
            assert_eq!(ok(expr), expected, "expression {expr:?}");
        }
    }

    #[test]
    fn the_fraction_of_an_rfc_3339_time_has_zero_three_six_or_nine_digits() {
        for (expr, expected) in [
            ("2024-01-01T10:00:00.0Z", "2024-01-01T10:00:00+00:00"),
            (
                "2024-01-01T10:00:00.000000000Z",
                "2024-01-01T10:00:00+00:00",
            ),
            ("2024-01-01T10:00:00.5Z", "2024-01-01T10:00:00.500+00:00"),
            (
                "2024-01-01T10:00:00.25+08:00",
                "2024-01-01T10:00:00.250+08:00",
            ),
            ("2024-01-01T10:00:00.123Z", "2024-01-01T10:00:00.123+00:00"),
            (
                "2024-01-01T10:00:00.1234Z",
                "2024-01-01T10:00:00.123400+00:00",
            ),
            (
                "2024-01-01T10:00:00.123456Z",
                "2024-01-01T10:00:00.123456+00:00",
            ),
            (
                "2024-01-01T10:00:00.1000000Z",
                "2024-01-01T10:00:00.100+00:00",
            ),
            (
                "2024-01-01T10:00:00.1234567Z",
                "2024-01-01T10:00:00.123456700+00:00",
            ),
            (
                "2024-01-01T10:00:00.123456789Z",
                "2024-01-01T10:00:00.123456789+00:00",
            ),
            // Digits after the ninth are dropped, not rounded.
            (
                "2024-01-01T10:00:00.1234567899999Z",
                "2024-01-01T10:00:00.123456789+00:00",
            ),
            (
                "2024-01-01T10:00:00.0000000009Z",
                "2024-01-01T10:00:00+00:00",
            ),
        ] {
            assert_eq!(ok(expr), expected, "expression {expr:?}");
        }
    }

    #[test]
    fn a_leap_second_is_kept_in_the_output() {
        for (expr, expected) in [
            ("2016-12-31T23:59:60Z", "2016-12-31T23:59:60+00:00"),
            (
                "2024-01-01T10:00:60.25+08:00",
                "2024-01-01T10:00:60.250+08:00",
            ),
            (
                "2024-01-01T10:00:60.999999999Z",
                "2024-01-01T10:00:60.999999999+00:00",
            ),
        ] {
            assert_eq!(ok(expr), expected, "expression {expr:?}");
        }
    }

    #[test]
    fn a_time_that_is_not_strict_rfc_3339_is_rejected() {
        for expr in [
            // No offset, or an offset in a different form.
            "2024-01-01T10:00:00",
            "2024-01-01T10:00:00+0800",
            "2024-01-01T10:00:00+08",
            "2024-01-01T10:00:00+8:00",
            "2024-01-01T10:00:00 +08:00",
            "2024-01-01T10:00:00UTC",
            "2024-01-01T10:00:00+08:00[Asia/Singapore]",
            "2024-01-01T10:00:00+08:00:00",
            // Values out of range.
            "2024-01-01T10:00:00+24:00",
            "2024-01-01T10:00:00+08:60",
            "2024-01-01T24:00:00Z",
            "2024-01-01T10:60:00Z",
            "2024-01-01T10:00:61Z",
            "2023-02-29T10:00:00Z",
            "1900-02-29T10:00:00Z",
            "2024-13-01T10:00:00Z",
            "2024-00-01T10:00:00Z",
            "2024-01-00T10:00:00Z",
            "2024-04-31T10:00:00Z",
            // Parts that are absent or have a different width.
            "2024-01-01T10:00Z",
            "2024-01-01T10Z",
            "2024-1-1T10:00:00Z",
            "2024-01-01T1:00:00Z",
            "24-01-01T10:00:00Z",
            "+2024-01-01T10:00:00Z",
            "12024-01-01T10:00:00Z",
            "20240101T100000Z",
            "2024-01-01T10:00:00.Z",
            "2024-01-01T10:00:00,5Z",
            // Different separators, and text before or after.
            "2024-01-01_10:00:00Z",
            "2024-01-01  10:00:00Z",
            "2024-01-01T10:00:00Z ",
            " 2024-01-01T10:00:00Z",
            "2024-01-01T10:00:00ZZ",
            "2024/01/01T10:00:00Z",
        ] {
            assert_invalid_expression(expr);
        }
    }

    #[test]
    fn a_date_gives_midnight_utc() {
        for (expr, expected) in [
            ("2024-01-01", "2024-01-01T00:00:00+00:00"),
            ("2024-12-31", "2024-12-31T00:00:00+00:00"),
            ("2024-02-29", "2024-02-29T00:00:00+00:00"),
            ("2000-02-29", "2000-02-29T00:00:00+00:00"),
            ("0000-02-29", "0000-02-29T00:00:00+00:00"),
            ("9999-12-31", "9999-12-31T00:00:00+00:00"),
            // One digit is accepted for the month and for the day.
            ("2024-1-5", "2024-01-05T00:00:00+00:00"),
            ("2024-01-5", "2024-01-05T00:00:00+00:00"),
            // A year with less than four digits is accepted.
            ("24-01-01", "0024-01-01T00:00:00+00:00"),
            ("7-1-1", "0007-01-01T00:00:00+00:00"),
            // White space is accepted before each number, not after the date.
            (" 2024-01-01", "2024-01-01T00:00:00+00:00"),
            ("\t2024-01-01", "2024-01-01T00:00:00+00:00"),
            ("2024- 01- 01", "2024-01-01T00:00:00+00:00"),
            // A year with a sign can have more than four digits.
            ("+2024-01-01", "2024-01-01T00:00:00+00:00"),
            ("+12345-06-07", "+12345-06-07T00:00:00+00:00"),
            ("+002024-01-01", "2024-01-01T00:00:00+00:00"),
            ("-0001-01-01", "-0001-01-01T00:00:00+00:00"),
            ("-1-1-1", "-0001-01-01T00:00:00+00:00"),
            ("-0-01-01", "0000-01-01T00:00:00+00:00"),
            ("-12345-06-07", "-12345-06-07T00:00:00+00:00"),
            // The limits of the year.
            ("+262142-12-31", "+262142-12-31T00:00:00+00:00"),
            ("-262143-01-01", "-262143-01-01T00:00:00+00:00"),
        ] {
            assert_eq!(ok(expr), expected, "expression {expr:?}");
        }
    }

    #[test]
    fn a_date_that_is_not_valid_is_rejected() {
        for expr in [
            "2023-02-29",
            "1900-02-29",
            "2024-13-01",
            "2024-00-10",
            "2024-01-00",
            "2024-01-32",
            "2024-04-31",
            "2024-01-01 ",
            "2024-01-01\n",
            "2024 -01-01",
            "2024/01/01",
            "2024-01",
            "2024",
            "20240101",
            "2024-001-01",
            "2024-01-011",
            "12345-01-01",
            "2024--01-01",
            "2024-+1-01",
            "+-2024-01-01",
            "+ 2024-01-01",
            "+262143-01-01",
            "-262144-12-31",
            "+99999999999-01-01",
            "-9223372036854775808-01-01",
            "+9223372036854775808-01-01",
            "01-01-2024",
            "not-a-time",
            "2024-01-01T",
            "today",
        ] {
            assert_invalid_expression(expr);
        }
    }

    /// `chrono` behaviour for durations that go out of its range. These are
    /// the only inputs whose result changes when `chrono` is removed.
    #[test]
    fn a_relative_expression_out_of_range() {
        // 2026-11-20 minus 5000000 days is in the year -11663.
        assert_eq!(ok("5000000d"), "-11663-05-09T12:34:56.123456789+00:00");
        for expr in ["9223372036854775807h", "9223372036854775807d", "200000000d"] {
            let result = std::panic::catch_unwind(|| at(expr, NOW));
            assert!(result.is_err(), "{expr:?} must panic with chrono");
        }
    }
}
