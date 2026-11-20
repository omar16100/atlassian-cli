# `chrono` is replaced by `jiff` and the standard library

Status: in progress on chore/replace-chrono (PR #PR)

The branch starts from `fix/security-advisories` (PR #190), because PR #190 was
not merged when this work started. PR #190 added the temporary advisory
exception that this change removes.

## Problem

`chrono` has the advisory RUSTSEC-2026-0543: the crate is unmaintained and has
no patched version. It was a direct dependency of the `cli` crate and the `api`
crate, and no other crate in the dependency tree used it
(`cargo tree -i chrono` showed only the two workspace crates). `deny.toml`
ignored the advisory as a temporary exception.

`chrono` had five call sites:

| File | Use |
| --- | --- |
| `crates/api/src/ratelimit.rs` | Store the `x-ratelimit-reset` time and calculate the wait |
| `crates/cli/src/commands/bitbucket/time_parser.rs` | Change `24h`, `7d`, `1w`, an RFC 3339 time or a date into the string of a Bitbucket query |
| `crates/cli/src/commands/bitbucket/bulk.rs` | Decide if a repository is stale (`--days`) |
| `crates/cli/src/commands/bitbucket/pipelines/steps.rs` | Show the elapsed time of a step that runs |
| `crates/cli/src/commands/bitbucket/pipelines/watch.rs` | Show the local wall clock (`HH:MM:SS`) in the log mode |

No type used the `serde` feature of `chrono`.

The acceptance rule was: no change in output.

## Design notes

### Crate choice: `jiff`

| Criterion | `jiff` 1.0.1 | `time` 0.3.55 |
| --- | --- | --- |
| In `Cargo.lock` before this change | No | No |
| Licence | `Unlicense OR MIT` | `MIT OR Apache-2.0` |
| Accepted by `deny.toml` | Yes (MIT) | Yes |
| Declared minimum Rust version | 1.85 | 1.88 |
| Crates added to `Cargo.lock` | 6 | 7 |
| Crates removed from `Cargo.lock` | 7 | 7 |
| Local time | Reads `TZ`, then `/etc/localtime`, with its own time zone code | Calls `localtime_r` of the C library |

The workspace declares no minimum Rust version, and CI uses the stable
toolchain. The crate counts come from a comparison of `Cargo.lock` for each
candidate.

The local time was the deciding criterion. `pipeline watch` runs on the
multi-thread Tokio runtime.

- The documentation of `time` 0.3.55 (`time::util::refresh_tz_unchecked`) says
  that an update of the time zone data is sound only if the operating system
  has a thread-safe environment, or the process has one thread, or no other
  thread changes the environment. It also says: "it is not possible for a
  library to soundly call this method". The safe function `refresh_tz` gives
  `None` when the call is not known to be sound. Thus, in this program, `time`
  cannot update the time zone data in a sound way. On a target without
  `tm_gmtoff`, the source of `time` says that the offset "can return an
  incorrect value".
- The documentation of `jiff` 1.0.1 (`TimeZone::system`) says that `TZ`
  overrides other sources, and that on Unix `/etc/localtime` is used when `TZ`
  is not set. `jiff` does not call the C library for this. If `jiff` finds no
  time zone, it uses a zone that behaves as UTC.

`jiff` is used with its default features.

### The `api` crate uses no date crate

`ratelimit.rs` needs only a count of seconds. The reset time is now an `i64`
count of seconds from the Unix epoch, and the current time comes from
`std::time::SystemTime`. The accepted range of the header value is the range
that `chrono` accepted (from the year -262143 to the year 262142), kept as two
constants.

The public field `RateLimitInfo::reset_at` changes its type from
`Option<chrono::DateTime<chrono::Utc>>` to `Option<std::time::SystemTime>`. A
type of the standard library does not put a third-party crate into the public
interface of the `api` crate. This is a change that the users of the library
can see. No code in this repository reads the field.

### A strict RFC 3339 parser (`commands/bitbucket/rfc3339.rs`)

`jiff` accepts more forms than `chrono` did: basic ISO 8601 (`20240101T100000Z`),
a time with no seconds, and a time zone annotation (`[Asia/Singapore]`). To
accept and reject the same inputs as before, the new module `rfc3339.rs` has a
strict parser written by hand. It follows the grammar that `chrono` used:

- `YYYY-MM-DD`, then `T`, `t` or a space, then `HH:MM:SS`;
- an optional `.` and one or more digits (digits after the ninth are dropped);
- `Z`, `z`, `+HH:MM` or `-HH:MM` (the Unicode minus sign U+2212 is accepted);
- second 60 (a leap second) is accepted;
- no text before or after the value.

`jiff` supplies the calendar: the date check, the count of seconds from the
Unix epoch, and the conversion of an instant to a date and a time.

The module also writes the output form that `chrono::DateTime::to_rfc3339`
wrote: a fraction of zero, three, six or nine digits, and a numeric offset
(`+00:00`, never `Z`).

A leap second has no place in Unix time. `chrono` read `:60` as second 59 when
the time of day of the current time (UTC) was later than that second 59. If
not, it read `:60` as the first second of the next minute. `elapsed_nanos`
keeps this rule, so the staleness decision and the step duration do not change
for such a timestamp.

### Date-only expressions

`chrono::NaiveDate::parse_from_str(expr, "%Y-%m-%d")` accepted more than
`YYYY-MM-DD`: one digit for the month or the day, a year with less than four
digits, a year with a sign and more than four digits, and white space before
each number. `parse_date_only` in `time_parser.rs` keeps these rules and the
same year limits. It does not use `jiff`, because `jiff` dates stop at the
years -9999 and 9999.

### Fixed-time seams

Each call site got a function that takes the current time as a parameter
(`parse_time_expression_at`, `running_step_duration`, `check_limit_at`,
`clock_text`; `is_stale` had one). The commands call them with the real clock.
The tests call them with a fixed time.

## Limitations

- A relative expression that gives a time before the year -9999 (more than
  about 12000 years before the current time, for example `5000000d`) is now an
  error:
  "duration is too large (the result is before the year -9999)". With `chrono`
  it gave a string with a negative year, as far as the year -262143.
- A relative expression that went out of the range of `chrono` (for example
  `200000000d`, or `9223372036854775807h`) caused a panic. It is now the same
  error.
- `RateLimitInfo::reset_at` is `None` on a platform whose `SystemTime` cannot
  represent the stored time. The wait calculation does not use `SystemTime`
  for the reset time, so it is not affected.
- The local clock of `pipeline watch --log` comes from `jiff`. On macOS, the
  test compares it with the `date` command. It was not run on Linux or on
  Windows in this work. The behaviour for an invalid `TZ` value was not
  compared with `chrono`.
- `rfc3339.rs` uses `u32::is_multiple_of`, which needs Rust 1.87 or later.

## Tests

The work was test first, in two commits.

1. The first commit added the fixed-time seams and the characterisation tests,
   with `chrono` still in use. All of them passed with `chrono`.
2. The second commit replaced `chrono`. The same tests passed with no edit to
   their expected values. Only the test helpers that build a time changed
   (`at`, `stale`, `running`, `clock_at`, `reset_epoch_secs`, `wait_at`).

The one test whose expected values changed is
`time_parser::pinned_tests::a_relative_expression_out_of_range`. With `chrono`
it pinned a negative-year string and three panics. It now pins the error (see
Limitations).

| File | Tests | What they pin |
| --- | --- | --- |
| `time_parser.rs` | module `pinned_tests` (15 tests, 14 of them from the first commit) | Each accepted and rejected form of relative, RFC 3339 and date-only expressions, the exact output string, and the three error messages |
| `bulk.rs` | 7 tests with the prefix `pinned_` | Exactly N days, N days plus one second or one nanosecond, offsets, invalid timestamps, the limits of the year, zero and negative `--days`, an out-of-range `--days`, a leap second |
| `pipelines/steps.rs` | 5 tests with the prefix `pinned_` | The duration text for fixed times, a dropped part of a second, a start time in the future, invalid start times, a leap second |
| `pipelines/watch.rs` | 3 tests with the prefix `pinned_` | The `HH:MM:SS` text for fixed times, the shape of the local clock, and agreement with the `date` command on a multi-thread runtime (Unix) |
| `crates/api/src/ratelimit.rs` | module `tests` (10 tests) | Valid, zero, negative and very large reset times, the limits of the accepted range, header values that are not integers |
| `rfc3339.rs` | module `tests` (5 tests) | The output form, input that is not ASCII, the elapsed time, the limits of the year |

Two tests in `pipelines/mock_tests.rs` run the command functions against a
`wiremock` server. `list_sends_the_parsed_time_expressions_in_the_query` reads
the `q` filter of the request. `steps_show_a_calculated_duration_only_for_a_running_step`
reads the duration of each step. These two tests were added after the
replacement, so they were not run with `chrono`.

The built binary cannot use a mock server for Bitbucket commands, because the
Bitbucket URL is a constant. Thus there is no built-binary test for these
commands. The binary was run by hand with a proxy variable that points to a
closed local port, so that no request left the machine: rejected expressions
gave the pinned error text, and the debug log of an accepted expression showed
the pinned strings in the query.

`cargo tree -i chrono` finds no package after the change. `cargo deny check`
and `cargo audit` pass with the `RUSTSEC-2026-0543` exception removed from
`deny.toml`.
