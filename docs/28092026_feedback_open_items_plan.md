# Feedback: the six items still open on 0.9.3 (plan)

Status: shipped in v0.10.0 (PR #153).

## Context

A user re-tested their 14 reported items against v0.9.3. Until then they had been
running the 0.2.8 Homebrew build: Homebrew 7 refuses to load a formula from an
untrusted tap ("Refusing to load formula omar16100/atlassian-cli/atlassian-cli
from untrusted tap omar16100/atlassian-cli", seen on Homebrew 7.0.4), so
`brew upgrade` never moved them. On 0.9.3, items 1, 6, 7, 8, 10, 11, 13 and 14 are
confirmed fixed. Six remain, each verified against the source:

| # | Symptom | Root cause |
| --- | --- | --- |
| 2 | A build waiting on manual steps shows `IN_PROGRESS` indefinitely | `PipelineState` has no `stage`. The API reports `state.name = IN_PROGRESS`, `state.stage.name = PAUSED` (confirmed by the reporter on build 592). `is_terminal_state` and `status_to_exit_code` are blind to it, and the latter maps any unknown state to 0, success |
| 3 | `-f json` gives `"state": "IN_PROGRESS 🔄"` and `"completed": ""` | `format_status_for_display` appends the icon even without colour; the list and get rows are all `String` filled with `unwrap_or_default()`, one row type for every format |
| 4 | `pr get` prints JSON with no `-f` | `OutputRenderer::render` falls back to JSON in table mode for anything that is not a non-empty array, so every single-object command (about 20) prints JSON by default |
| 5 | `pipeline list <repo>` is rejected | pipeline commands take the repo only from `--repo` or the git remote |
| 9 | `--debug` prints nothing extra | the filter directive `atlassian-cli=debug` never matches the underscore targets (`atlassian_cli`, `atlassian_cli_api`); `--debug` is not global, so it is rejected after a subcommand; `RUST_LOG` overrides it; the client never logs a response status or error body |
| 12 | `pr update` has no `--reviewers` | `update_pull_request` PUTs `{title?, description?}` only |

Related defects found while checking, fixed here because they sit in the same code:

- A completed step keeps its outcome in `state.result.name`, but `logs --failed-only`,
  the `NOT_RUN` skip, the log header and `pipeline_has_failed_steps` read
  `state.name` (always `COMPLETED`). So `logs --failed-only` never matches and
  `rerun --pr --failed-only` never reruns.
- `pr reviewers --add` PUTs `{title, reviewers}` and drops `description`.

## Approach

1. **Split `pipelines.rs`** (2052 lines, over the 2000-line limit) into
   `bitbucket/pipelines/{mod,model,state,list,steps,logs,trigger,watch,status}.rs`.
   No behaviour change; test count unchanged.
2. **Paused state (2).** `PipelineState.stage`. Derived status: `result.name` when
   present; for `IN_PROGRESS` with a stage other than `RUNNING`, the stage name
   (`PAUSED`, and whatever else Bitbucket reports there, without hardcoding
   `HALTED`); otherwise `state.name`. `watch` and `status --wait` stop on a
   paused build; exit code **3** means "waiting on manual action"; unknown states
   exit **1** instead of 0. `pipeline get` on a paused build, and `list --steps`,
   report `pending_manual_steps`. Step triggers normalised
   (`pipeline_step_trigger_manual` becomes `MANUAL`, unknown values pass through).
   One `step_outcome` helper for the failed-step readers.
3. **Clean machine output (3).** `OutputFormat::is_human()` (`Table`, `Markdown`).
   Pipeline rows are built by pure functions: icons and colour only for human
   formats (colour only for `Table`); `created`, `completed`, `commit`,
   `ref_name` are `Option` (`null` in JSON); `build_number` is a number.
4. **Single objects render as a table (4).** In table mode a single object
   renders as a two-column `FIELD | VALUE` table in declaration order; an array
   of objects inside it renders below as a titled sub-table; arrays of scalars
   are comma-joined; nested objects are compact JSON. The renderer builds
   strings and prints once, so it is testable. `pr get` shows reviewers in the
   table too.
5. **Positional repo on pipeline commands (5).** `list`, `latest`, `trigger`,
   `status` take `[REPO]`; `get`, `stop`, `watch`, `steps`, `rerun` take
   `[REPO] [PIPELINE]`; `logs` takes `[REPO] [PIPELINE] [STEP_UUID]`. Rule:
   values beyond the id slots that flags have not filled make the first value
   the repo; otherwise the first value is the repo only if it cannot be a
   pipeline id (not all digits and not a UUID). A numeric repo slug therefore
   needs `--repo`. A positional repo that disagrees with `--repo` is an error.
   The new argument does not reuse the global `repo` id.
6. **`--debug` (9).** Global flag; directives `info,atlassian_cli=debug`
   (`EnvFilter` matches targets by prefix, so this covers every workspace
   crate); `--debug` wins over `RUST_LOG`. The API client logs method, path,
   status and elapsed time for each response, and on a non-2xx the response
   body, truncated and credential-scrubbed. Request bodies are never logged
   (secured pipeline variables travel in them), only their size; token-like
   query values are redacted.
7. **`pr update --reviewers` (12).** Replaces the reviewer set; values must be
   UUIDs (the error points at `pr reviewers` for finding them). Every PR mutation
   GETs the PR and PUTs `title`, `description` and `reviewers` together, so an
   update never drops a field it did not mean to change. Existing reviewers are
   read from the PR's `reviewers` field.
8. **Docs.** CHANGELOG `[Unreleased]`, `docs/c4model.md`, a feature document,
   README (positional repo, exit code 3, `brew trust` in the Homebrew section),
   root `todo.md`.

Out of scope, recorded for later: `pr` positional `repo` shadowing the global
`--repo`; `jira issue create/assign --assignee <email>` sending the raw value as
the account id; `bb workspace list` requiring a workspace; `auth whoami` on a
Bitbucket-only profile needing `--bitbucket`; quiet mode with numeric ids; CSV of a
single object; `bb api --paginate`; positional repo on `pipeline var`/`env`.

## Verification

- `make pre-commit`, `make test`.
- Unit and wiremock tests per item (listed in the feature document when done).
- Live, read-only, on the reporter's repository with a working Bitbucket token:
  `pipeline list <repo>`, `pipeline get <repo> <build>` on the paused build
  (table and `-f json`), `pipeline steps <repo> <build>`, `pipeline status
  --wait` exit code, `pr get <repo> <id>`, `--debug` on a failing request.
- The PR write path is live-tested only on a PR the user names; otherwise it is
  reported as mock-verified only.

## Progress

- [x] 1 split (`e1a95d1`)
- [x] 2 paused state and 3 machine output (`a3c6747`)
- [x] 4 single-object table (`4df0fe1`)
- [x] 5 positional repo (`d251723`)
- [x] 6 `--debug` (`6c985b7`)
- [x] 7 `pr update --reviewers` (`2ac9d08`)
- [x] codex review round 1 (`e535c87`): secret-named fields redacted in logs,
  `--debug` after `RUST_LOG`, warning on unrecognised states during waits
- [x] codex review round 2: secret names matched across case and separators, a
  finished build with an unknown result ends a wait, `pipeline logs` no-match
  output structured in machine formats
- [x] 8 docs: CHANGELOG, c4model, feature doc, README, root `todo.md`
- [ ] live verification (blocked: both stored tokens on the build machine return
  401; needs a working Bitbucket token and the reporter's workspace)

## Deviations

- Plan said HALTED would be hardcoded; the status derivation surfaces any
  non-RUNNING stage instead, and only PAUSED/HALTED map to exit code 3.
- `pending_manual_steps` on `pipeline list` needs `--steps` (no extra requests
  otherwise); `pipeline get` fetches steps for a paused build on its own.
- Unknown states exit 1 (was 0). Waits stop on any finished build, and keep
  polling through an unrecognised in-progress state with a warning.
- The API client's five status-to-error matches became one `error_for_status`
  in `crates/api/src/response.rs`, to log error bodies in one place; this kept
  `lib.rs` well under 2000 lines.
- `jira workflow export` (no `--output`) and `confluence folder get` keep JSON in
  table mode through a new `render_document`.
