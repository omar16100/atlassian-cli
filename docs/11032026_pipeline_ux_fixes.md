# Pipeline UX Fixes (2026-03-11)

7 issues fixed across git remote detection, pipeline commands, output formatting, and error messages.

## Changes

### 1. Git remote auto-detection (git.rs)
- `detect_git_context()` now iterates all remotes instead of hardcoding "origin"
- Priority: configured `bitbucket_remote` > origin > first Bitbucket remote
- New `bitbucket_remote` field in Profile config for preferred remote name
- Uses `git remote get-url <name>` per remote (fetch URL, not push)

### 2. `--pipeline` flag (mod.rs)
- All pipeline commands now accept `--pipeline <ID>` in addition to positional arg
- `pipeline steps 13` and `pipeline steps --pipeline 13` both work
- Logs: `step_uuid` kept as positional + added `--step-uuid` flag (both work)
- Helper: `resolve_pipeline_arg()` deduplicates resolution logic

### 3. `--envelope` flag for JSON output (output/lib.rs)
- New `--envelope` global flag wraps list JSON/YAML in `{"data": [...], "count": N}`
- Opt-in to avoid breaking existing scripts
- `render_list()` method on OutputRenderer handles envelope logic
- Table/CSV/Markdown unaffected

### 4. `--on-complete` hook for watch (pipelines.rs)
- `pipeline watch --on-complete "notify.sh"` runs command on completion
- Env vars: `PIPELINE_STATUS`, `PIPELINE_BUILD_NUMBER`, `PIPELINE_UUID`, `PIPELINE_REF_NAME`
- Hook failure prints warning but does not override pipeline exit code

### 5. `pipeline status --wait` + watch exit codes
- `pipeline status --wait` polls until terminal state, pin on UUID at start
- `pipeline watch` now exits with meaningful code (0=success, 1=failed, 2=in-progress)
- Shared `status_to_exit_code()` helper
- Watch returns final status to caller, exit code handled in dispatch

### 6. Better `--repo` missing error (mod.rs)
- `require_repo()` now shows detected git remotes with redacted credentials
- Suggests `--workspace <slug> --repo <slug>` with example command
- No signature change, all 14 call sites unaffected

### 7. `--step` respects `-i` flag (pipelines.rs)
- `--step` name pattern now honors `--ignore-case` / `-i` flag
- Previously `-i` only applied to `--grep`

## Files Changed
- `crates/config/src/lib.rs` — `bitbucket_remote` field on Profile
- `crates/cli/src/commands/bitbucket/git.rs` — remote detection refactor
- `crates/cli/src/commands/bitbucket/mod.rs` — clap enum, dispatch, error messages
- `crates/cli/src/commands/bitbucket/pipelines.rs` — watch/status/logs logic
- `crates/cli/src/main.rs` — `--envelope` flag, `bitbucket_remote` threading
- `crates/output/src/lib.rs` — `render_list()`, `ListEnvelope`, `with_envelope()`

## Tests Added
- `test_status_to_exit_code` — exit code mapping
- `test_resolve_pipeline_arg_*` (4 tests) — positional/flag resolution
- `test_render_list_*` (3 tests) — envelope rendering
- `test_with_envelope_setter` — builder pattern
