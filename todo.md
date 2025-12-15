# Changes Made

## 2025-12-15 (v7) - Add cargo-release for automated version bumping
- Added `cargo-release` configuration to workspace Cargo.toml
  - `shared-version = true` - all crates share the same version
  - `tag-name = "v{{version}}"` - creates tags like `v0.1.8`
  - `pre-release-commit-message = "chore: bump version to {{version}}"`
- Added `[package.metadata.release] release = false` to internal crates (api, auth, config, output, bulk)
- Added `[package.metadata.release] release = true` to CLI crate
- Usage: `cargo release patch --execute` (or `minor`, `major`)
- Files modified: `Cargo.toml`, `crates/*/Cargo.toml`

## 2025-12-15 (v6) - Code Review Fixes
- Fixed progress bar panic risk in `crates/bulk/src/lib.rs:267`
  - Changed `.unwrap()` to `.unwrap_or_else()` with fallback to `ProgressStyle::default_bar()`
  - Logs warning when template is invalid
- Added credential corruption warnings in `crates/auth/src/lib.rs`
  - `set_secret()` and `delete_secret()` now log warnings when JSON parsing fails
  - Previously silently returned empty HashMap on parse errors
- Added rate limiter timeout protection in `crates/api/src/ratelimit.rs`
  - All mutex lock calls now use 5-second timeout via `tokio::time::timeout()`
  - Prevents indefinite blocking if lock is held
  - Methods gracefully degrade: `update_from_response()` skips update, `check_limit()` returns None, `get_info()` returns empty info
- Fixed HTTP client connection pool loss in `crates/cli/src/commands/confluence/attachments.rs`
  - Added `http_client()` method to `ApiClient` to expose underlying reqwest client
  - `upload_attachment()` and `download_attachment()` now reuse connection pool instead of creating new clients
- Added user-visible pagination warning in `crates/cli/src/commands/confluence/bulk.rs`
  - `search_page_ids()` now shows `eprintln` warning when results hit 1000 limit
  - Previously only logged to tracing (not visible to users)
- Files modified: `crates/bulk/src/lib.rs`, `crates/auth/src/lib.rs`, `crates/api/src/lib.rs`, `crates/api/src/ratelimit.rs`, `crates/cli/src/commands/confluence/attachments.rs`, `crates/cli/src/commands/confluence/bulk.rs`

## 2025-12-15 (v5) - Fix Bitbucket-only profile auth flow
- Fixed: Bitbucket-only profiles were rejected because `resolve_active_profile()` required base_url + Jira token
- Root cause: `main.rs:164-220` called same resolution function for all commands
- Solution: Split profile resolution into two functions:
  - `resolve_profile_for_product()` - Jira/Confluence/JSM (requires base_url + token)
  - `resolve_profile_for_bitbucket()` - Bitbucket (requires only email + bitbucket token)
- Shared validation via `BaseProfile` struct and `resolve_base_profile()` helper to avoid duplication
- Changed structs: `ActiveProfile` replaced with `BaseProfile`, `ProductProfile`, `BitbucketProfile`
- Improved error messages: distinguishes "no Bitbucket token (only Jira token found)" vs "no token at all"
- Updated command dispatch to call appropriate resolution function per command type
- Bitbucket profile resolution falls back to general token if no bitbucket-specific token exists
- Added regression tests:
  - `test_bitbucket_only_profile_no_base_url_error` - verifies Bitbucket commands work without base_url
  - `test_jira_still_requires_base_url` - verifies Jira commands still require base_url
- Fixed missing `tracing` dependency in `crates/auth/Cargo.toml` (pre-existing issue)
- Files modified: `crates/cli/src/main.rs`, `crates/cli/tests/cli_integration.rs`, `crates/cli/Cargo.toml`, `crates/auth/Cargo.toml`

## 2025-12-15 (v4) - CLI UX Consistency Refactoring
- Phase 1: Quick fixes
  - Standardized `--limit` defaults to 25 across all products (was 50 in Jira/Confluence)
  - Fixed emoji inconsistency: all `✓` changed to `✅` in Bitbucket files (15 instances)
  - Verified snake_case flag was NOT an issue (clap auto-converts to kebab-case)
  - Files: `jira/mod.rs`, `confluence/mod.rs`, all `bitbucket/*.rs` files

- Phase 2: Output format compliance
  - Created `crates/cli/src/commands/common.rs` with `MutationResult` struct and `render_success()` helper
  - Updated 83+ mutation commands to respect `--output json/yaml/csv/quiet` flags
  - Success messages now render as JSON/YAML/etc when appropriate output format specified
  - Table format still shows emoji messages for human readability
  - Quiet format outputs just the ID when available
  - Files modified: all mutation commands across `jira/*.rs`, `bitbucket/*.rs`, `confluence/*.rs`

- Phase 3: Consistency fixes
  - Standardized confirmation messages to single-line format: `"⚠️  This will permanently delete {resource} {id}. Use --force to confirm."`
  - Standardized empty result messages to `"No {resources} found"` pattern
  - Added user-facing println for all empty results (previously only tracing)
  - Files modified: `jira/issues.rs`, `jira/projects.rs`, `jira/bulk.rs`, `bitbucket/repos.rs`, `bitbucket/pullrequests.rs`, `bitbucket/branches.rs`, `bitbucket/webhooks.rs`, `bitbucket/workspaces.rs`, `bitbucket/permissions.rs`, `bitbucket/pipelines.rs`, `bitbucket/commits.rs`, `confluence/bulk.rs`, `jsm.rs`

- Phase 4: Remove short flags from Jira (BREAKING)
  - Removed short flags `-a`, `-s`, `-y`, `-l`, `-t`, `-p` from Jira search command
  - Now consistent with Bitbucket/Confluence which use long flags only
  - File: `jira/mod.rs`

- Phase 5: Jira command restructure (BREAKING)
  - Moved flat issue commands under `jira issue` subcommand
  - Old: `jira search`, `jira get`, `jira create`, `jira update`, `jira delete`, `jira transition`, `jira assign`, `jira unassign`
  - New: `jira issue search`, `jira issue get`, `jira issue create`, etc.
  - Nested commands also moved: `jira watchers` → `jira issue watchers`, `jira links` → `jira issue links`, `jira comments` → `jira issue comments`
  - Now consistent with Bitbucket (`bb repo`, `bb pr`) and Confluence (`confluence page`, `confluence space`) patterns
  - File: `jira/mod.rs`

- Phase 6: Help text improvements
  - Added `long_about` with usage examples to key commands
  - Jira: `issue search`, `issue create` with JQL and filter flag examples
  - Bitbucket: `repo list`, `repo get`, `repo create` with workspace and flag examples
  - Confluence: `space list`, `space get`, `space create`, `search cql`, `search text`, `search in-space`, `search params` with CQL and filter examples
  - Improved argument descriptions with format examples (e.g., "Space key (e.g., TEAM)")
  - Removed short flags from Confluence search params for consistency with other products
  - Files: `jira/mod.rs`, `bitbucket/mod.rs`, `confluence/mod.rs`

## 2025-12-15 (v3)
- Code review fixes for Bitbucket auth flow
  - Added `BITBUCKET_API_URL` constant in `crates/auth/src/lib.rs:10`
  - Exported `get_token()` and `get_bitbucket_token()` as public functions
  - Fixed docstring: clarified no fallback to general token
  - Removed duplicate token lookup in `main.rs`, now uses `auth::get_bitbucket_token()`
  - Made logging levels consistent (all use `debug!` for non-existent token deletion)
  - Added `--all` flag to `auth list` to show all profiles including inactive
  - Improved error messages with env var hints
  - Added unit tests for `token_key()`, `bitbucket_token_key()`, `BITBUCKET_API_URL`
  - Files modified: `crates/auth/src/lib.rs`, `crates/cli/src/commands/auth.rs`, `crates/cli/src/main.rs`

## 2025-12-15 (v2)
- Enhanced Bitbucket pipeline step info in CLI
  - Added step UUID, started, completed, duration, logs_url to `StepInfo` struct
  - Added `started_on`, `completed_on`, `duration_in_seconds` fields to `PipelineStep` API struct
  - Added `format_duration_secs()` helper for human-readable duration formatting
  - Updated `fetch_steps()` to populate new fields with `include_details` parameter
  - Added `--steps` flag to `bb pipeline list` command to show step summary per pipeline
  - Added `bb pipeline steps <repo> <uuid>` command for dedicated step listing
  - Files modified: `crates/cli/src/commands/bitbucket/pipelines.rs`, `crates/cli/src/commands/bitbucket/mod.rs`

## 2025-12-15
- Improved Bitbucket auth flow with separate token storage
  - Added `bitbucket_token_key()` function in `crates/auth/src/lib.rs:15`
  - Bitbucket tokens stored with `{profile}_bitbucket` key in credentials file

- Added `--bitbucket` flag to `auth login`
  - File: `crates/cli/src/commands/auth.rs:110`
  - Stores Bitbucket token separately from Jira token
  - Added `--workspace` flag for Bitbucket workspace config
  - Shows Bitbucket app password URL when in Bitbucket mode

- Added `--bitbucket` flag to `auth test`
  - File: `crates/cli/src/commands/auth.rs:88`
  - Tests against Bitbucket API `/2.0/user` endpoint

- Added `--bitbucket` flag to `auth logout`
  - File: `crates/cli/src/commands/auth.rs:126`
  - Removes only Bitbucket token when flag is set

- Updated `auth list` to show Bitbucket status
  - File: `crates/cli/src/commands/auth.rs:286`
  - Now shows `has_jira_token`, `has_bitbucket_token`, `workspace` columns
  - Only shows profiles with at least one active token

- Updated Bitbucket token lookup to include credentials file
  - File: `crates/cli/src/main.rs:224`
  - Priority: env vars → credentials file (`{profile}_bitbucket` key)

## 2025-11-26
- Fixed `auth whoami` runtime panic (same nested runtime issue)
  - Made `whoami` async in `crates/cli/src/commands/auth.rs:222`
  - Removed nested tokio runtime, now uses existing runtime via `.await`

- Added API token URL hint to `auth login` flow
  - File: `crates/cli/src/commands/auth.rs:228`
  - Now shows "You can get the API token from: https://id.atlassian.com/manage-profile/security/api-tokens" before prompting for token

- Fixed `auth test` runtime panic ("Cannot start a runtime from within a runtime")
  - Made `auth::handle` async in `crates/cli/src/commands/auth.rs:88`
  - Made `test_auth` async in `crates/cli/src/commands/auth.rs:286`
  - Removed nested tokio runtime, now uses existing runtime via `.await`
  - Updated `main.rs:122` to await auth::handle

- Added `bitbucket whoami` command to verify Bitbucket authentication
  - Added `Whoami` variant to `BitbucketCommands` enum in `crates/cli/src/commands/bitbucket/mod.rs:77`
  - Added `whoami()` function calling `/2.0/user` endpoint in `crates/cli/src/commands/bitbucket/workspaces.rs:312`
  - Displays username, display name, account ID, UUID

- Added hidden password input + file-based credential storage (removed keychain)
  - Token input now hidden via `rpassword` crate
  - Removed `keyring` dependency entirely
  - Tokens stored only in `~/.atlassian-cli/credentials` with 600 permissions
  - Token lookup: env var → credentials file
  - Removed `CredentialStore` struct, simplified auth code
