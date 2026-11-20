# Quiet stderr: the default log level is `warn`

Status: shipped in v0.14.0 (PR #184).

## Problem

The default log level was `info`, written to stderr
(`crates/cli/src/main.rs`, `tracing_directives`). Almost every command logs an
`info` line that repeats its stdout message, for example
`INFO Issue created successfully key=PROJ-1` or `INFO No issues found`.

Callers that capture a command with `2>&1` got that line in the same stream as
the JSON, and the JSON parse failed. Coding agents use `2>&1` as a habit, so
they met this failure often.

## What changed for users

- A successful command writes no log lines to stderr at the default level.
  Warnings stay: a truncated list, a rate limit wait, a retry, a skipped item.
- `RUST_LOG=info` shows the `info` lines again. `--debug` shows them too, with
  the request trace.
- `auth login` and `auth logout` print their result on stdout. The log line was
  their only output. Table and markdown formats print one line. The machine
  formats print the usual result object (`success`, `message`, `id`, where `id`
  is the profile name). The token is never printed.
- `bb project delete` prints "Cancelled." on stdout when you answer no.
- `auth login -f json` (and the other machine formats) prints the result object
  only. The note about a `/wiki` base URL is printed for the table and markdown
  formats only, and the token prompt goes to stderr, as the other prompts do.
  A user name or password in the base URL is not printed back.
- `auth logout` fails when it cannot read or write a credentials file. Before,
  it logged the error at debug level and reported success, so a token could
  stay on disk. It now also removes the token from a legacy plaintext file when
  the encrypted file is in good condition.
- Empty lists that showed their hint only in a log line print it on stdout in
  the table and markdown formats, as the other list commands do: `auth list`,
  `bamboo project list`, the four `confluence search` commands, and
  `opsgenie alert list`, `recipients`, `logs` and `notes`. Before, these printed
  `[]` in a table. The machine formats are unchanged (`[]`).
- The "Rate limit usage high" message is an `info` line now. It was a warning
  that appeared for each response when the usage was more than 80%, also on a
  successful command. The warning for a rate limit wait stays.

## Design notes

One line changed the level: `tracing_directives(false, None)` returns `warn`.
The `--debug` directives stay `info,atlassian_cli=debug`, and a `RUST_LOG` value
is used as it is. An unparseable `RUST_LOG` falls back to `warn`.

The `info!` calls stay in the code. They are useful in a trace, and to remove
342 calls gives no benefit to the user.

Each `info!` call was checked for a stdout message in the same function. The
calls that had none are the ones above (login, logout, cancelled delete). The
`product.rs` stub has one more, but no command uses that module.

The `warn!` calls report retries, rate limit waits, incomplete pagination and
skipped items. The user must read those, so they stay. One was not a warning
and is `info` now: "Rate limit usage high".

## Limitations

- Scripts that read the `INFO` lines from stderr get nothing now. They must set
  `RUST_LOG=info`.
- Bulk commands (`jira bulk import`, `jira bulk assign`, `jira bulk label`,
  `confluence bulk delete`, `confluence bulk add-labels`) logged one `info`
  line for each item, for example the key of each imported issue. Those lines
  need `RUST_LOG=info` now. Stdout still has only the summary line. A per-item
  result on stdout is a possible follow-up.
- A command that has a warning still writes to stderr, so `2>&1` can still mix
  streams in that case. Use `2>/dev/null` or read the two streams separately.
- Some commands write messages to stderr directly, not through the log, and
  those are unchanged. The bulk commands (`jira bulk`, `confluence bulk`,
  `bb bulk`) print "Found N ..." and "Listing only ..." on stderr by design, so
  their `2>&1` output is still not one JSON document.
- `bb project delete` prints its question and the "Cancelled." line on stdout
  in each format.
- The message of a migrated plaintext credentials file during `auth login` is
  an `info` line and is hidden. The login confirmation says that the token is
  stored encrypted.

## Tests

- `crates/cli/src/main.rs`: `the_default_level_hides_info_and_keeps_warnings`,
  `rust_log_and_debug_still_show_info` (a real `EnvFilter` built from the
  directives), and the updated blank `RUST_LOG` fallback.
- `crates/cli/tests/quiet_stderr_e2e.rs` (built binary against a mock Jira):
  `jira issue create -f json` has an empty stderr and the two streams together
  are one JSON document; an empty `jira issue search -f json` has an empty
  stderr and prints `[]`; `RUST_LOG=info` and `--debug` still show the success
  line; `auth login` and `auth logout` confirm on stdout and leave stderr empty;
  `auth login -f json` with a `/wiki` URL that has a password prints one JSON
  document without the password; `auth logout` fails on an unreadable
  credentials file; an empty `auth list` keeps its hint; a cancelled
  `bb project delete` says so on stdout.
- No test covers the fallback for a `RUST_LOG` value that cannot be parsed.
- Not verified against a live Atlassian site. The change is in local logging
  only, and no request changed.
