# Pipeline UX Fixes Round 2 (2026-03-14)

5 fixes based on real-world usage feedback. 1 item skipped (no Bitbucket API for manual step triggers).

## Changes

### 1. `watch --timeout` (pipelines.rs, mod.rs)
- `--timeout <seconds>` exits with code 2 if pipeline doesn't finish in time
- Rejects `--timeout 0` via clap range validator
- Table/log mode: prints timeout message to stderr
- JSON/YAML mode: renders structured PipelineView with current state (no plain text leak)
- Added TIMEOUT to `status_to_exit_code()` mapping

### 2. Steps trigger/manual column (pipelines.rs)
- New `StepTrigger` struct deserializes `trigger.type` from Bitbucket API
- `trigger` field added to `StepInfo` output (skip_serializing_if None)
- Shows "manual"/"automatic" in steps table when API provides the data
- Backward compatible: `serde(default)` handles missing field

### 3. Forbidden error with scope hint (error.rs)
- `suggestion()` return type: `Option<&str>` → `Option<String>`
- For 403 with "scope"/"privilege"/"permission" in message body: appends link to app-passwords settings
- Case-insensitive keyword matching
- Generic fallback for non-scope 403s unchanged

### 4. Steps elapsed time for in-progress (pipelines.rs)
- In-progress steps now show `~2m 15s` (approximate elapsed since started_on)
- Uses `chrono::DateTime::parse_from_rfc3339` + `Utc::now()` subtraction
- Converts FixedOffset to Utc before subtraction
- Guards against negative elapsed (clock skew) with `.max(0)`
- Falls back to None silently if timestamp parse fails

### 5. Manual step trigger — SKIPPED
No Bitbucket Cloud API endpoint for triggering individual manual pipeline steps.

### 6. `watch --log` mode (pipelines.rs, mod.rs)
- `--log` flag forces log mode: one timestamped line per poll, no ANSI escape codes
- Auto-enabled when stdout is not a TTY (`is_table && !stdout.is_terminal()`)
- Three-way rendering: ANSI (interactive TTY) / log (non-TTY or --log) / structured (JSON/YAML)
- Log format: `[HH:MM:SS] #123 STATUS icon (branch) [elapsed] [steps_summary]`

## Files Changed
- `crates/api/src/error.rs` — suggestion() return type + scope-aware Forbidden hints
- `crates/cli/src/commands/bitbucket/pipelines.rs` — watch timeout/log, StepTrigger, elapsed time
- `crates/cli/src/commands/bitbucket/mod.rs` — Watch clap variant (--timeout, --log)

## Tests Added (5 new, 372 total)
- `test_timeout_exit_code` — TIMEOUT maps to exit code 2
- `test_step_trigger_deserialization` — trigger.type=manual parses correctly
- `test_step_without_trigger_deserialization` — missing trigger is None
- `forbidden_with_scope_message_includes_app_passwords_link`
- `forbidden_without_scope_omits_app_passwords_link`
- `forbidden_with_permission_message_includes_link`
