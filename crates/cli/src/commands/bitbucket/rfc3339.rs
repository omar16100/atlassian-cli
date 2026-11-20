//! RFC 3339 timestamps for the Bitbucket commands.
//!
//! The parser is strict and written by hand. `jiff` accepts more forms (basic
//! ISO 8601, a time with no seconds, a time zone annotation), and the commands
//! must accept and reject the same inputs as before. `jiff` supplies the
//! calendar: the date check, the count of seconds from the Unix epoch and the
//! conversion of an instant to a date and a time.
//!
//! The output form is fixed: a fraction of zero, three, six or nine digits and
//! a numeric offset (`+00:00`, never `Z`). The string goes into Bitbucket
//! queries, so it must not change.

use jiff::civil;
use jiff::tz::Offset;
use jiff::Timestamp;

const NANOS_PER_SECOND: i128 = 1_000_000_000;
const SECONDS_PER_DAY: i64 = 86_400;

/// A date and a time with an offset, as written in an RFC 3339 string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rfc3339 {
    date: civil::Date,
    hour: u8,
    minute: u8,
    /// 0 to 60. The value 60 is a leap second.
    second: u8,
    nanosecond: u32,
    offset_seconds: i32,
}

impl Rfc3339 {
    /// Parse a strict RFC 3339 date-time.
    ///
    /// The grammar is `YYYY-MM-DD`, then `T`, `t` or a space, then `HH:MM:SS`,
    /// then an optional `.` with one or more digits, then `Z`, `z` or
    /// `+HH:MM` / `-HH:MM`. The Unicode minus sign (U+2212) is accepted for a
    /// negative offset. Digits after the ninth fraction digit are dropped.
    /// Second 60 is accepted. Text before or after the value is not accepted.
    pub(crate) fn parse(input: &str) -> Option<Self> {
        let bytes = input.as_bytes();
        if bytes.len() < 19 {
            return None;
        }
        let digit = |index: usize| -> Option<u32> {
            let byte = bytes[index];
            byte.is_ascii_digit().then(|| u32::from(byte - b'0'))
        };
        let two_digits =
            |index: usize| -> Option<u32> { Some(digit(index)? * 10 + digit(index + 1)?) };

        let year = two_digits(0)? * 100 + two_digits(2)?;
        let month = two_digits(5)?;
        let day = two_digits(8)?;
        let hour = two_digits(11)?;
        let minute = two_digits(14)?;
        let second = two_digits(17)?;
        if bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
            return None;
        }
        if !matches!(bytes[10], b'T' | b't' | b' ') {
            return None;
        }
        if hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        let date = civil::Date::new(
            i16::try_from(year).ok()?,
            i8::try_from(month).ok()?,
            i8::try_from(day).ok()?,
        )
        .ok()?;

        // The first 19 bytes are ASCII, so index 19 and index 20 are character
        // boundaries.
        let (nanosecond, rest) = if bytes.get(19) == Some(&b'.') {
            parse_fraction(&input[20..])?
        } else {
            (0, &input[19..])
        };
        let offset_seconds = parse_offset(rest)?;

        Some(Self {
            date,
            hour: hour as u8,
            minute: minute as u8,
            second: second as u8,
            nanosecond,
            offset_seconds,
        })
    }

    /// The fixed output form, with the date, the time and the offset as
    /// written. A leap second stays as second 60.
    pub(crate) fn to_output_string(self) -> String {
        format_rfc3339(
            i32::from(self.date.year()),
            self.date.month() as u32,
            self.date.day() as u32,
            u32::from(self.hour),
            u32::from(self.minute),
            u32::from(self.second),
            self.nanosecond,
            self.offset_seconds,
        )
    }

    /// The count of seconds from the Unix epoch to this value, with a leap
    /// second read as second 59.
    fn unix_second(self) -> i64 {
        let second = self.second.min(59);
        let local = self
            .date
            .at(self.hour as i8, self.minute as i8, second as i8, 0);
        let epoch = civil::date(1970, 1, 1).at(0, 0, 0, 0);
        local.duration_since(epoch).as_secs() - i64::from(self.offset_seconds)
    }

    /// The time from this value to `now` in nanoseconds. The result is
    /// negative when this value is after `now`.
    ///
    /// A leap second has no place in Unix time. It is read as second 59 when
    /// the time of day of `now` (UTC) is later than that second 59. If not, it
    /// is read as the first second of the next minute. This is the rule that
    /// the commands used before, so results do not change.
    pub(crate) fn elapsed_nanos(self, now: Timestamp) -> i128 {
        let now_nanos = now.as_nanosecond();
        let mut then_second = self.unix_second();
        if self.second == 60 {
            let now_second = now_nanos.div_euclid(NANOS_PER_SECOND);
            let now_time_of_day = now_second.rem_euclid(i128::from(SECONDS_PER_DAY));
            let then_time_of_day = i128::from(then_second.rem_euclid(SECONDS_PER_DAY));
            if now_time_of_day <= then_time_of_day {
                then_second += 1;
            }
        }
        let then_nanos = i128::from(then_second) * NANOS_PER_SECOND + i128::from(self.nanosecond);
        now_nanos - then_nanos
    }
}

/// Read the digits after the `.` of a fraction. Give the nanoseconds and the
/// text after the digits.
fn parse_fraction(input: &str) -> Option<(u32, &str)> {
    let digit_count = input.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 {
        return None;
    }
    let mut nanosecond = 0u32;
    for index in 0..9 {
        let digit = match input.as_bytes().get(index) {
            Some(byte) if index < digit_count => u32::from(byte - b'0'),
            _ => 0,
        };
        nanosecond = nanosecond * 10 + digit;
    }
    Some((nanosecond, &input[digit_count..]))
}

/// Read an offset (`Z`, `z`, `+HH:MM` or `-HH:MM`) that must end the input.
/// Give the offset in seconds east of UTC.
fn parse_offset(input: &str) -> Option<i32> {
    if input == "Z" || input == "z" {
        return Some(0);
    }
    let mut chars = input.chars();
    let negative = match chars.next()? {
        '+' => false,
        '-' | '\u{2212}' => true,
        _ => return None,
    };
    let bytes = chars.as_str().as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return None;
    }
    if !(bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4].is_ascii_digit())
    {
        return None;
    }
    let hours = i32::from(bytes[0] - b'0') * 10 + i32::from(bytes[1] - b'0');
    let minutes = i32::from(bytes[3] - b'0') * 10 + i32::from(bytes[4] - b'0');
    if hours > 23 || minutes > 59 {
        return None;
    }
    let seconds = hours * 3600 + minutes * 60;
    Some(if negative { -seconds } else { seconds })
}

/// The fixed output form of an instant, in UTC.
pub(crate) fn format_utc(instant: Timestamp) -> String {
    let utc = Offset::UTC.to_datetime(instant);
    format_rfc3339(
        i32::from(utc.year()),
        utc.month() as u32,
        utc.day() as u32,
        utc.hour() as u32,
        utc.minute() as u32,
        utc.second() as u32,
        utc.subsec_nanosecond() as u32,
        0,
    )
}

/// Write the fixed output form.
///
/// A year from 0 to 9999 has four digits. A different year has a sign and a
/// minimum of four digits. The fraction is absent for a whole second. If not,
/// it has three, six or nine digits: the shortest that keeps the value. The
/// offset is `+HH:MM` or `-HH:MM`, and UTC is `+00:00`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn format_rfc3339(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanosecond: u32,
    offset_seconds: i32,
) -> String {
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+05}")
    };
    let fraction = if nanosecond == 0 {
        String::new()
    } else if nanosecond.is_multiple_of(1_000_000) {
        format!(".{:03}", nanosecond / 1_000_000)
    } else if nanosecond.is_multiple_of(1_000) {
        format!(".{:06}", nanosecond / 1_000)
    } else {
        format!(".{nanosecond:09}")
    };
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let offset_minutes = offset_seconds.unsigned_abs() / 60;
    format!(
        "{year}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{fraction}{sign}{:02}:{:02}",
        offset_minutes / 60,
        offset_minutes % 60,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    #[test]
    fn the_output_form_keeps_the_fields_as_written() {
        for (input, expected) in [
            ("2024-01-01T10:00:00Z", "2024-01-01T10:00:00+00:00"),
            ("2024-01-01t10:00:00.5z", "2024-01-01T10:00:00.500+00:00"),
            (
                "2024-01-01 10:00:00.000001-05:30",
                "2024-01-01T10:00:00.000001-05:30",
            ),
            (
                "2016-12-31T23:59:60.000000001+08:00",
                "2016-12-31T23:59:60.000000001+08:00",
            ),
        ] {
            let parsed = Rfc3339::parse(input).unwrap();
            assert_eq!(parsed.to_output_string(), expected, "input {input:?}");
        }
    }

    #[test]
    fn text_that_is_not_ascii_does_not_cause_a_panic() {
        for input in [
            "2024-01-01T10:00:00\u{e9}",
            "2024-01-01T10:00:0\u{e9}Z",
            "\u{e9}024-01-01T10:00:00Z",
            "2024-01-01T10:00:00.\u{e9}Z",
            "2024-01-01T10:00:00.5\u{e9}",
            "2024-01-01T10:00:00\u{2212}",
            "2024-01-01T10:00:00\u{2212}05:3\u{e9}",
            "2024-01-01T10:00:00+\u{e9}5:30",
            "\u{1f600}\u{1f600}\u{1f600}\u{1f600}\u{1f600}",
        ] {
            assert_eq!(Rfc3339::parse(input), None, "input {input:?}");
        }
    }

    #[test]
    fn the_elapsed_time_is_exact_to_the_nanosecond() {
        let then = Rfc3339::parse("2026-11-20T20:00:00.25+08:00").unwrap();
        assert_eq!(then.elapsed_nanos(now("2026-11-20T12:00:01Z")), 750_000_000);
        assert_eq!(
            then.elapsed_nanos(now("2026-11-20T12:00:00Z")),
            -250_000_000
        );
    }

    #[test]
    fn the_elapsed_time_works_at_the_limits_of_the_year() {
        let first = Rfc3339::parse("0000-01-01T00:00:00+23:59").unwrap();
        let last = Rfc3339::parse("9999-12-31T23:59:59.999999999-23:59").unwrap();
        let current = now("2026-11-20T12:00:00Z");
        assert!(first.elapsed_nanos(current) > 0);
        assert!(last.elapsed_nanos(current) < 0);
        // 1970-01-01T00:00:00Z is the Unix epoch.
        let epoch = Rfc3339::parse("1970-01-01T00:00:00Z").unwrap();
        assert_eq!(epoch.elapsed_nanos(now("1970-01-01T00:00:00Z")), 0);
        assert_eq!(first.unix_second(), -62_167_219_200 - 86_340);
        assert_eq!(last.unix_second(), 253_402_300_799 + 86_340);
    }

    #[test]
    fn an_instant_is_written_in_utc() {
        assert_eq!(
            format_utc(now("2026-11-20T20:00:00.120+08:00")),
            "2026-11-20T12:00:00.120+00:00"
        );
        assert_eq!(
            format_utc(now("-000164-07-24T12:34:56.123456789Z")),
            "-0164-07-24T12:34:56.123456789+00:00"
        );
    }
}
