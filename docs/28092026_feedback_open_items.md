# Paused pipelines, plain JSON, single-object tables, positional repo, `--debug`, `pr update --reviewers`

Status: in progress on `fix/feedback-open-items`.

Plan and triage: [28092026_feedback_open_items_plan.md](28092026_feedback_open_items_plan.md).

## Problem

A user re-tested 14 reported items on v0.9.3. Eight were already fixed (they had
been running the 0.2.8 Homebrew build, which Homebrew 7 would not upgrade from an
untrusted tap). Six were still open:

| # | Reported | Cause |
| --- | --- | --- |
| 2 | A build waiting only on manual steps shows `IN_PROGRESS` indefinitely | `state.stage` (`PAUSED`) was never read |
| 3 | `-f json` gives `"state": "IN_PROGRESS 🔄"` and `"completed": ""` | icons appended in every format; missing values rendered as `""` |
| 4 | `pr get` prints JSON with no `-f` | the table renderer fell back to JSON for any single object |
| 5 | `pipeline list <repo>` rejected; needs `--repo` | pipeline commands had no repository argument |
| 9 | `--debug` prints nothing extra | filter target misspelt, flag not global, no status or error body logged |
| 12 | `pr update` has no `--reviewers` | not implemented |

## What changed for users

- **Paused builds.** `pipeline list`, `get`, `status` and `watch` report `PAUSED`
  for a build waiting on a manual step. `pipeline get` on a paused build (and
  `list --steps`) adds `pending_manual_steps`. `watch` and `status --wait` stop
  there instead of polling until `--timeout`, and exit **3**. An unrecognised
  status now exits 1, not 0. `watch --on-complete` runs at a pause too, with
  `PIPELINE_STATUS=PAUSED`.
- **Plain machine output.** In JSON, YAML, CSV and quiet, pipeline `state` is the
  bare status, `build_number` is a number, and `completed`, `created`, `commit`,
  `ref_name`, `target_type` are `null` when absent. `trigger` and `rerun` output
  `build_number` as a number too. Step triggers read `MANUAL` / `AUTOMATIC`
  instead of `pipeline_step_trigger_manual`. Tables and markdown keep their
  icons; markdown loses the ANSI colour it used to get.
- **Tables for single objects.** Every `get`, `create` and `update` that returns
  one object prints a `field | value` table by default, in declaration order, with
  lists of objects (a pull request's reviewers) as a titled table below. `pr get`
  therefore shows reviewers in table mode. `jira workflow export` without
  `--output` and `confluence folder get` still print JSON, since the document is
  the output.
- **Repository as an argument.** `pipeline list|latest|trigger|status [REPO]`,
  `pipeline get|stop|watch|steps|rerun [REPO] [PIPELINE]`,
  `pipeline logs [REPO] [PIPELINE] [STEP_UUID]`. `--repo` and the git remote
  still work.
- **`--debug`** is accepted anywhere on the command line and logs each request
  (method, URL) and response (status, elapsed time, and for an error the body) on
  stderr.
- **`pr update --reviewers`** replaces the reviewer set. Every pull request edit
  now sends title, description and reviewers together.

## Design notes

**Status derivation.** `result.name` when present; for `IN_PROGRESS`, the stage
unless it is `RUNNING`; else `state.name`. The stage is surfaced generically
rather than matching `PAUSED` alone, so another stage value is shown instead of
being hidden as `IN_PROGRESS`. Only `PAUSED` and `HALTED` map to exit code 3 and
stop a wait. The reporter confirmed `state.stage.name = PAUSED` on a real paused
build; `HALTED` is not confirmed against the API.

**Step outcome.** A finished step's `state.name` is always `COMPLETED`, with the
outcome in `result`. `logs --failed-only`, the skipped-step check, the log header
and `pipeline_has_failed_steps` read `state.name`, so `logs --failed-only` matched
nothing and `rerun --pr --failed-only` never reran. They now share
`get_step_status`.

**Rows per format** (`pipelines/rows.rs`). One row type serves every format, so
the decoration is decided when the row is built, by `OutputFormat::is_human()`.

**Record tables** (`crates/output`). The object is serialised to a string and read
back into an `IndexMap` to keep field order; `serde_json::Value` sorts keys.
`serde_json`'s `preserve_order` feature was not used: it would change key order in
every JSON document the CLI prints. Table-mode output is built as a string, so it
is tested directly.

**Positional repository** (`bitbucket/positional.rs`). The identifiers were
already optional positionals, so a lone value has to be classified. Count first: a
value beyond the identifier slots that no flag filled is the repository. Then
shape: a first value that is neither all digits nor a UUID is the repository. A
value that looks like an identifier while `--pipeline` also supplies one is an
error rather than a repository, matching the clap conflict it replaces. A
positional repository that disagrees with `--repo` is an error. The argument's
clap id is `repo_arg` / `targets`, not `repo`, which would shadow the global flag
(the reason `bb pr get --repo x 54` fails today).

**Logging** (`crates/api/src/response.rs`). The filter directive was
`atlassian-cli`, but targets are module paths with underscores; `atlassian_cli`
matches every workspace crate by prefix. The five copies of the status-to-error
match in `ApiClient` became one `error_for_status`, which reads the body once for
both the log and the error. Error variants per status are unchanged except that
a 400 from `get_bytes` is now `BadRequest` rather than `ServerError(400)`; neither
retries. Request bodies are never logged: secured pipeline variables travel in
them. In a logged error body, any JSON field whose name contains `token`,
`secret`, `password`, `credential`, `authorization`, `signature` or `api_key` is
redacted at any depth, as is the `value` of an object marked `"secured": true`;
query parameters with such names are redacted in logged URLs. Plain `key` is
not treated as secret, so `issueKey` and a variable's name stay readable.
`--debug`'s directives come after `RUST_LOG`'s: a later directive for the same
target replaces an earlier one, so `RUST_LOG=atlassian_cli=off` cannot silence
the flag, while a more specific target still refines it.

**PR edits.** `build_pr_put_body` takes each of title, description and reviewers
from the change or the current pull request. Reviewers the API returns without a
UUID are left out of the body rather than dropped; an edit that must rebuild the
list refuses. Typed reviewers must be UUIDs (a name used to be wrapped in braces
and sent), and the author is refused as a reviewer.

## Limitations

- `pr` subcommands still take `repo` as a required positional whose clap id
  shadows the global `--repo`, so `bb pr get --repo x 54` is rejected. Unchanged
  here; `pr get <repo> <id>` works.
- `pipeline var` and `pipeline env` take the repository from `--repo` or the git
  remote only.
- A repository whose slug is all digits or UUID-shaped needs `--repo` unless every
  identifier is given positionally.
- A pipeline state this CLI does not recognise exits 1 from `pipeline status`,
  but `watch` and `status --wait` keep polling through it (it may be
  transitional) after one warning on stderr.
- Log redaction is by field and parameter name plus the credential-shape
  scrub; a secret under an unremarkable name in a non-JSON error body could
  still be logged under `--debug`.
- CSV of a single object still falls back to JSON; quiet mode still prints only
  string `id`s.
- Whether a PUT that omits `reviewers` or `description` clears them has not been
  checked against live Bitbucket; the change sends both regardless.
- `bb api` has no `--paginate`.
- Jira `--assignee <email>` on `issue create` and `issue assign` sends the value
  as an account ID with no user lookup; not changed here.
- `bitbucket/mod.rs` (1941 lines) and `pullrequests.rs` (1882) are close to the
  2000-line limit.

## Tests

- `pipelines/state.rs`: paused, running, halted and finished status derivation;
  recognised versus unknown states;
  exit codes including 3 and unknown as 1; step outcome from `result`; trigger
  normalisation; pending manual step count.
- `pipelines/rows.rs`: JSON view with no icons, numeric `build_number`, `null`
  `completed`; YAML/CSV/quiet plain; markdown keeps icons; steps fetched only for
  the count are not listed.
- `pipelines/mock_tests.rs` (wiremock): `get` fetches steps once for a paused
  build and not for a running one; `watch` stops on a paused build with status
  `PAUSED`; a `COMPLETED`/`FAILED` step counts as a failure; `logs --failed-only`
  fetches only the failed step's log.
- `crates/output`: single object renders as a `field | value` table in
  declaration order; nested list of objects as a titled table; scalars, lists and
  nulls; nested objects as compact JSON; long values wrap; lists and bare values
  unchanged; `is_human`.
- `bitbucket/positional.rs`: identifier shape; repo plus identifiers; numeric repo
  by count; `logs` with three values; flag-filled slots; identifier given twice;
  repository precedence and conflict; clap parse of the new and existing forms.
- `crates/api/src/response.rs`: URL redaction by secret-like name; JSON error
  bodies with secret-named and secured fields redacted, names kept; bodies
  scrubbed and bounded on a char boundary. The existing 401/403/text/bytes/raw tests pass unchanged
  against the shared mapping.
- `main.rs`: real `EnvFilter`s built from the directives, checked by which debug
  events they emit: `--debug` logs the CLI and API client and not dependencies,
  the old `atlassian-cli=debug` filter logs neither, `RUST_LOG=atlassian_cli=off`
  does not beat the flag, a more specific `RUST_LOG` target still applies;
  `--debug` accepted after a subcommand.
- `tests/debug_logging_e2e.rs` (built binary, mock Jira): a failing request under
  `--debug` logs method, path, `status=404`, timing and the 404 body, and not the
  token, with stdout empty; no trace without `--debug`; `-f json --debug` leaves
  stdout parseable.
- `pullrequests.rs`: PUT body keeps description and reviewers on a title edit;
  reviewers without UUIDs left alone; UUID-only reviewers; author refused; blanks
  and duplicates dropped; wiremock: `update --reviewers` replaces, `update --title`
  resends, `reviewers --add` keeps the description, a bad value sends no PUT, an
  empty update is refused.

Live verification: see the plan document.
