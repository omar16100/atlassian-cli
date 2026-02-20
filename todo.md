# Changes Made

## 2026-02-20 - Bitbucket Bearer Auth Support & App Password Deprecation

### Context
Bitbucket app passwords deprecated (creation disabled Sep 2025, all disabled Jun 2026).
atlassian-cli only supported Basic auth in CLI paths despite API layer having Bearer support.

### Changes

**Priority 1: Help text & error message updates** (`crates/cli/src/commands/auth.rs`)
- Updated `read_token_from_stdin()` — replaced app password URL with Bitbucket API token instructions
- Updated `test_bitbucket_auth()` failure message — added deprecation hint and bearer suggestion
- Updated `LoginArgs` after_help — added bearer example and deprecation notice
- Updated `list_profiles()` — added `bitbucket_auth` column showing basic/bearer

**Priority 2: Bearer auth support**
- `crates/config/src/lib.rs` — Added `bitbucket_token_type: Option<String>` to `Profile` struct
- `crates/cli/src/commands/auth.rs`:
  - Added `--bearer` flag to `LoginArgs` (requires `--bitbucket`)
  - Made `--email` optional (not required for `--bearer`)
  - Added `is_bitbucket_bearer()` helper
  - Updated `login_bitbucket()` to store token_type in profile
  - Updated `test_bitbucket_auth()` to use `/2.0/workspaces` for bearer tokens
  - Updated `auth_status()` to build bearer client and use correct endpoint
- `crates/cli/src/main.rs`:
  - Added `is_bearer` field to `BitbucketProfile`
  - Updated `resolve_profile_for_bitbucket()` — email optional for bearer
  - Updated `build_bitbucket_client()` — uses `with_bearer_token()` for bearer
  - Passes `is_bearer` through to `bitbucket::execute()`
- `crates/cli/src/commands/bitbucket/utils.rs`:
  - Added `is_bearer` field to `BitbucketContext`
  - Updated `verify_auth()` — uses `/2.0/workspaces` for bearer tokens
- `crates/cli/src/commands/bitbucket/mod.rs` — passes `is_bearer` to context
- `crates/cli/src/commands/bitbucket/workspaces.rs` — `whoami()` shows workspaces for bearer

**Doc fixes**
- `crates/config/src/lib.rs` — fixed "keyring" reference in Profile doc comment to "encrypted credential files"

### Tests Added
- Config: 6 tests for `bitbucket_token_type` (default none, bearer, skip serialization, backwards compat, roundtrip)
- Auth: 4 tests for `is_bitbucket_bearer()` (default, bearer, basic, nonexistent)
- CLI integration: 2 tests (bearer flag in help, bearer profile no email required)

### Key Design Decisions
- Bearer tokens use `/2.0/workspaces` instead of `/2.0/user` (access tokens are not user-scoped)
- `--email` is optional when `--bearer` is set
- Token type stored in profile config (not credential store) for simplicity
- Env vars (`BITBUCKET_TOKEN`) default to whatever `bitbucket_token_type` the profile specifies

---

## 2026-01-31 - Add Pipeline Variable/Secret Management

### Feature
Full CRUD for Bitbucket pipeline variables at 3 scopes: repository, workspace, and deployment environment.

### CLI Commands
```
bb pipeline var list [--workspace-level | --deployment <env>]
bb pipeline var get --key <KEY> [--workspace-level | --deployment <env>]
bb pipeline var create --key <KEY> --value <VAL> [--secured] [--workspace-level | --deployment <env>]
bb pipeline var update --key <KEY> --value <VAL> [--secured | --unsecured] [--workspace-level | --deployment <env>]
bb pipeline var delete --key <KEY> --force [--workspace-level | --deployment <env>]
bb pipeline env list
```

### Key Design Decisions
- Tri-state `--secured`/`--unsecured` on update (neither = preserve current state)
- Key-based operations with transparent UUID resolution
- Environment name-to-UUID resolution (case-insensitive)
- `delete_no_content()` in ApiClient for 204 responses
- `type: "pipeline_variable"` in create/update payloads

### Files Created
- `crates/cli/src/commands/bitbucket/variables.rs` — data structures, CRUD functions, resolution helpers, 13 unit tests

### Files Modified
- `crates/api/src/lib.rs` — added `delete_no_content()` method
- `crates/cli/src/commands/bitbucket/mod.rs` — added VarCommands, EnvCommands, dispatch logic

### Tests
- 13 new unit tests (URL building, deserialization, payload format, scope labels, UUID cleaning, secured display)
- All 310 tests pass, zero clippy warnings

## 2026-01-31 - Fix `--repo` flag for pipeline commands

### Problem
`bb pipeline list --repo genai_images` failed — `repo` was a positional arg on each pipeline subcommand, shadowing the global `--repo` flag from `BitbucketArgs`. Also, 5 of 10 pipeline subcommands panicked on `--help` due to invalid clap positional ordering (optional `repo` before required `pipeline_id`).

### Changes
- Removed positional `repo: Option<String>` from all 10 `PipelineCommands` variants
- Updated all 10 handler match arms to use global `--repo` via `require_repo(None, global_repo.as_deref(), ...)`
- Updated `require_repo()` error message to reference `--repo` flag only

### Result
- `bb pipeline list --repo genai_images` now works
- `bb --repo genai_images pipeline list` works
- `bb pipeline list` auto-detects from git remote
- `bb pipeline get 42` works (pipeline_id as sole positional)
- No more `--help` panics on get/stop/logs/watch/steps

### File Modified
- `crates/cli/src/commands/bitbucket/mod.rs`

## 2026-01-31 - Fix Silent Auth Failure on Search + Related Issues

### Problem
`jira issue search --assignee @me` returns "No issues found" when auth is expired. Jira's API returns HTTP 200 with empty results when permissions-based filtering removes everything. The code printed "No issues found" without validating credentials.

### Changes

1. **Fix ANSI in tracing output** (`crates/cli/src/main.rs`)
   - Added `.with_ansi(std::io::stderr().is_terminal())` to tracing fmt builder
   - Non-TTY output no longer contains ANSI escape codes

2. **Add `Forbidden` variant to ApiError** (`crates/api/src/error.rs`)
   - New `Forbidden { message: String }` variant for 403 responses
   - Shares suggestion with `AuthenticationFailed` (points to `auth test`)
   - Not retryable
   - Added 3 unit tests

3. **Handle 403 in API client** (`crates/api/src/lib.rs`)
   - Added `StatusCode::FORBIDDEN` arm in `request()`, `get_text()`, `get_bytes()`
   - Returns `ApiError::Forbidden` with response body as message
   - Added 4 wiremock tests (403 JSON, 403 text, 403 bytes, 401 auth)

4. **Display error suggestions in main** (`crates/cli/src/main.rs`)
   - Restructured: `main()` → `run()` pattern
   - `main()` catches errors, downcasts to `ApiError`, shows `.suggestion()` on stderr
   - Exits with code 1 on error

5. **Add `verify_auth()` to context types**
   - `JiraContext` → `/rest/api/3/myself` (`crates/cli/src/commands/jira/utils.rs`)
   - `JsmContext` → `/rest/api/3/myself` (`crates/cli/src/commands/jsm/utils.rs`)
   - `ConfluenceContext` → `/wiki/rest/api/user/current` (`crates/cli/src/commands/confluence/utils.rs`)
   - `BitbucketContext` → `/2.0/user` (`crates/cli/src/commands/bitbucket/utils.rs`)
   - `BambooContext` → `/rest/api/latest/info` (`crates/cli/src/commands/bamboo/utils.rs`)

6. **Auth check on empty results** (high-impact commands)
   - `jira issue search` (`crates/cli/src/commands/jira/issues.rs:95`)
   - `confluence search` (`crates/cli/src/commands/confluence/search.rs`)
   - `bitbucket repo list` (`crates/cli/src/commands/bitbucket/repos.rs:73`)
   - `bitbucket pr list` (`crates/cli/src/commands/bitbucket/pullrequests.rs:147`)
   - `bamboo build list` (`crates/cli/src/commands/bamboo/builds.rs:48`)
   - `jsm request list` (`crates/cli/src/commands/jsm/requests.rs:87`)
   - `jsm servicedesk list` (`crates/cli/src/commands/jsm/servicedesk.rs:66`)

### Files Modified
| File | Change |
|------|--------|
| `crates/api/src/error.rs` | Forbidden variant + tests |
| `crates/api/src/lib.rs` | 403 handling in 3 methods + wiremock tests |
| `crates/api/Cargo.toml` | Added wiremock dev-dependency |
| `crates/cli/src/main.rs` | ANSI fix, run() pattern, suggestion display |
| `crates/cli/src/commands/jira/utils.rs` | verify_auth() |
| `crates/cli/src/commands/jira/issues.rs` | Auth check on empty search |
| `crates/cli/src/commands/confluence/utils.rs` | verify_auth() |
| `crates/cli/src/commands/confluence/search.rs` | Auth check + empty handling |
| `crates/cli/src/commands/bitbucket/utils.rs` | verify_auth() |
| `crates/cli/src/commands/bitbucket/repos.rs` | Auth check on empty list |
| `crates/cli/src/commands/bitbucket/pullrequests.rs` | Auth check on empty list |
| `crates/cli/src/commands/bamboo/utils.rs` | verify_auth() |
| `crates/cli/src/commands/bamboo/builds.rs` | Auth check on empty list |
| `crates/cli/src/commands/jsm/utils.rs` | verify_auth() |
| `crates/cli/src/commands/jsm/requests.rs` | Auth check on empty list |
| `crates/cli/src/commands/jsm/servicedesk.rs` | Auth check on empty list |

### Tests
- 7 new tests (3 error.rs unit tests + 4 wiremock integration tests)
- All 226 tests pass

## 2026-01-31 - Merge Dependabot Cargo Deps Bump (PR #29)

### Problem
PR #29 bumped 10 cargo dependencies (clap, tokio, reqwest, serde_json, colored, thiserror, chrono, url, whoami, indexmap). Security Audit failed because `reqwest 0.12→0.13` switched to rustls by default, pulling in `aws-lc-rs` with ISC/OpenSSL licenses and `webpki-root-certs` with CDLA-Permissive-2.0 — none in deny.toml allowlist.

### Changes
- Added `ISC`, `OpenSSL`, `CDLA-Permissive-2.0` to `deny.toml` license allowlist (all permissive, OSI-approved)
- Removed stale `RUSTSEC-2025-0119` ignore (number_prefix no longer in dep tree after updates)

### File Modified
- `deny.toml`

## 2026-01-27 - Website Footer Credit
- Added "Built by Omar Shabab" credit with link to omarshabab.com
- Added `.footer-credit` styling to match existing footer aesthetic
- Files modified: `docs/index.html`, `docs/styles.css`

## 2026-01-25 - CLI UX Improvements from User Feedback

### Issue 1: Rename `--output` to `--format` (global flag)
- Renamed `--output` to `--format` to avoid conflict with subcommand `--output` file path flags
- Made flag global with `global = true` so it works on all subcommands
- Added short flag `-f` for convenience
- File: `crates/cli/src/main.rs:27-29, 81`

### Issue 2: Build number support for `pipeline logs`
- Changed `pipeline_uuid` to `pipeline_id` in Logs command enum
- Added `resolve_pipeline_id()` call before `get_pipeline_logs()`
- Now accepts both build numbers (e.g., `335`) and UUIDs
- File: `crates/cli/src/commands/bitbucket/mod.rs:481-485, 1131-1158`

### Issue 3: Improve 406 error handling for log fetching
- Changed Accept header from `text/plain; charset=utf-8` to `text/plain, */*;q=0.1`
- Added explicit `StatusCode::NOT_ACCEPTABLE` (406) handling
- File: `crates/api/src/lib.rs:202, 224-232`

### Issue 4: Add browser URL to log fetch errors
- Error messages now include browser URL for manual viewing
- URL format: `https://bitbucket.org/{workspace}/{repo}/pipelines/results/{uuid}/steps/{step_uuid}`
- File: `crates/cli/src/commands/bitbucket/pipelines.rs:890-913`

### Files Modified
| File | Change |
|------|--------|
| `crates/cli/src/main.rs` | Rename output→format, add global=true |
| `crates/cli/src/commands/bitbucket/mod.rs` | Logs command uses pipeline_id + resolve |
| `crates/cli/src/commands/bitbucket/pipelines.rs` | Browser URL in error messages |
| `crates/api/src/lib.rs` | Broader Accept header, 406 handling |
| `crates/cli/tests/cli_integration.rs` | Updated test to use --format |

## 2026-01-25 - Fix Homebrew Formula Release Workflow

- Release workflow failing since v0.2.1 due to cargo-dist version mismatch
- Dependabot bumped `actions/checkout@v4` to `@v6`, but cargo-dist expects v4
- Regenerated `.github/workflows/release.yml` via `dist init`
- Re-tagged v0.2.4 to trigger new release build with homebrew formula update
- File: `.github/workflows/release.yml`

## 2026-01-25 - Fix Homebrew Installation Command

- Fixed incorrect Homebrew install command on website
- Changed `omar16100/tap/atlassian-cli` → `omar16100/atlassian-cli/atlassian-cli`
- File: `docs/index.html` (lines 86-87, 350-351)

## 2026-01-19 - UX Improvements

### Issue 1: Auth UX - Hint for `--bitbucket-token`
- Added clap error hook in `main.rs` to detect `--bitbucket-token` typo and suggest correct syntax
- Added `after_help` to `LoginArgs` in `auth.rs` showing examples for Jira and Bitbucket auth
- Files: `crates/cli/src/main.rs`, `crates/cli/src/commands/auth.rs`

### Issue 2: Add `auth status` Command
- New command: `atlassian-cli auth status [--profile <name>] [--configured-only]`
- Shows authentication status for all services: Jira/Confluence, Bitbucket, OpsGenie, Bamboo
- Tests actual API connectivity for Jira and Bitbucket (shows OK/FAILED/N/A)
- OpsGenie and Bamboo show CONFIGURED status (API key present)
- `--configured-only` flag hides N/A entries
- File: `crates/cli/src/commands/auth.rs`

### Issue 3: Pipeline Repo Auto-Detection Error Messages
- Created `require_repo()` helper function for better error messages
- Used across all pipeline commands (list, get, trigger, stop, logs, watch, steps, status, rerun)
- Error now shows: "Repository required for '<cmd>'. Not in a git directory with Bitbucket remote."
- File: `crates/cli/src/commands/bitbucket/mod.rs`

### Issue 4: Build Number vs UUID - Renamed `uuid` to `pipeline_id`
- Renamed `uuid` field to `pipeline_id` in Get, Stop, Watch, Steps commands
- Clarifies that both build numbers and UUIDs are accepted
- Made `resolve_pipeline_id()` public for use in Stop command
- Files: `crates/cli/src/commands/bitbucket/mod.rs`, `crates/cli/src/commands/bitbucket/pipelines.rs`

### Issue 5: Add Commit Hash to Pipeline Output
- Added `commit` field to `PipelineRow`, `PipelineView`, `PipelineStatusOutput` structs
- Shows short hash (7 chars) of the commit that triggered the pipeline
- Added `get_commit_hash()` helper function
- Visible in: `pipeline list`, `pipeline get`, `pipeline status`, `pipeline watch`
- File: `crates/cli/src/commands/bitbucket/pipelines.rs`

### Issue 6: Improve Auth Failure Error Messages
- Improved Bitbucket auth error message to mention `auth list` command
- Shows: "Check token status: atlassian-cli auth list"
- Updated `AuthenticationFailed` suggestion in error.rs to mention both `auth list` and `auth test`
- Files: `crates/cli/src/main.rs`, `crates/api/src/error.rs`

### Files Modified
| File | Changes |
|------|---------|
| `crates/cli/src/main.rs` | Clap error hook, improve Bitbucket auth errors |
| `crates/cli/src/commands/auth.rs` | New Status command, LoginArgs after_help |
| `crates/cli/src/commands/bitbucket/mod.rs` | Rename uuid→pipeline_id, require_repo helper |
| `crates/cli/src/commands/bitbucket/pipelines.rs` | Add commit hash, resolve_pipeline_id public |
| `crates/api/src/error.rs` | Update auth failure suggestion |

## 2026-01-15 - Bug Fixes for JSM, OpsGenie & Bamboo Modules

### Double Rendering Bug Fix
- Fixed 6 locations where `println!()` + early return broke JSON/YAML output
- Files fixed:
  - `crates/cli/src/commands/bamboo/projects.rs:40-45` - list_projects
  - `crates/cli/src/commands/jsm/requests.rs:86-91` - list_requests
  - `crates/cli/src/commands/opsgenie/alerts.rs:57-62` - list_alerts
  - `crates/cli/src/commands/opsgenie/alerts.rs:407-412` - list_recipients
  - `crates/cli/src/commands/opsgenie/alerts.rs:441-446` - list_logs
  - `crates/cli/src/commands/opsgenie/alerts.rs:473-478` - list_notes
- Now always renders via ctx.renderer.render() for consistent output format support

### OpsGenie Query Parameter Bug Fix
- Fixed `crates/cli/src/commands/opsgenie/alerts.rs:14-27`
- When both `--query` and `--status` provided, they were overwriting each other
- Now combines them with " AND " for proper OpsGenie query syntax

## 2026-01-14 - JSM, OpsGenie & Bamboo CLI Implementation

### Phase 1: JSM Enhancement (Refactored to Module)
- Refactored `jsm.rs` → `jsm/` module directory structure
- Created files:
  - `mod.rs` - Main routing with full command enum definitions
  - `utils.rs` - JsmContext, ServiceDesk, RequestType, Customer, Organization types
  - `requesttypes.rs` - list_request_types, get_request_type
  - `customers.rs` - list_customers, get_customer, create_customer, delete_customer
  - `feedback.rs` - list_feedback, get_feedback
  - `knowledgebase.rs` - list_articles, get_article
- Full CRUD for service desk requests with transitions, comments, attachments, participants
- Added `delete_with_body()` method to ApiClient for JSM endpoints requiring body in DELETE

### Phase 2: OpsGenie Implementation (NEW)
- Created `opsgenie/` module with EU/US region support
- Added `GenieKey` authentication to ApiClient
  - New `AuthMethod::GenieKey { api_key }` variant
  - `with_genie_key()` builder method
  - Sets `Authorization: GenieKey {key}` header
- Created files:
  - `utils.rs` - OpsgenieContext, Alert, Incident, Schedule, Team, Escalation, Service, Heartbeat types
  - `alerts.rs` - list, get, create, close, acknowledge, snooze, escalate, assign, add_note, delete, list_recipients, list_logs, list_notes
  - `incidents.rs` - list, get, create, close, resolve, reopen, add_responder, add_note, delete, list_timeline
  - `schedules.rs` - list, get, create, delete, enable, disable, get_on_call, get_timeline, export_ical
  - `teams.rs` - list, get, create, delete, list_members, add_member, remove_member, get_on_call
  - `escalations.rs` - list, get, create, delete
  - `services.rs` - list, get, create, delete
  - `heartbeats.rs` - list, get, create, delete, enable, disable, ping
  - `mod.rs` - Full command enum definitions and routing
- Added config profile fields:
  - `opsgenie_api_key: Option<String>`
  - `opsgenie_region: Option<String>` ("us" or "eu")
- Added `OPSGENIE_API_KEY` and `OPSGENIE_REGION` env var support with fallback to profile

### Phase 3: Bamboo Implementation (NEW)
- Created `bamboo/` module with full CI/CD operations
- Created files:
  - `utils.rs` - BambooContext, Project, Plan, Branch, BuildResult, Agent, DeploymentProject, Environment, Artifact types
  - `projects.rs` - list_projects, get_project
  - `plans.rs` - list, get, enable, disable, favorite, unfavorite
  - `branches.rs` - list, get, create, delete, enable, disable
  - `builds.rs` - list, get, latest, run, stop, logs, comment, add_label, remove_label
  - `deployments.rs` - list_projects, get_project, list_environments, get_environment, list_results, trigger, list_versions
  - `agents.rs` - list, get, enable, disable, capabilities, server_info, queue
  - `artifacts.rs` - list, download
  - `mod.rs` - Full command enum definitions and routing
- Added config profile field:
  - `bamboo_base_url: Option<String>` (falls back to base_url if not set)
- Added `get_bytes()` method to ApiClient for binary artifact downloads

### Phase 4: Cross-Cutting Concerns
- Updated `main.rs`:
  - `resolve_profile_for_opsgenie()` - env var first, then profile config
  - `build_opsgenie_client()` - uses GenieKey auth
  - `resolve_profile_for_bamboo()` - uses bamboo_base_url or base_url
  - `build_bamboo_client()` - uses Basic auth
- Updated `crates/config/src/lib.rs` with new profile fields
- Updated `crates/api/src/lib.rs` with delete_with_body and GenieKey auth

### Files Created
- `crates/cli/src/commands/jsm/*.rs` (12 files)
- `crates/cli/src/commands/opsgenie/*.rs` (9 files)
- `crates/cli/src/commands/bamboo/*.rs` (9 files including artifacts.rs)

### Files Modified
- `crates/api/src/lib.rs` - delete_with_body, GenieKey auth
- `crates/config/src/lib.rs` - opsgenie_api_key, opsgenie_region, bamboo_base_url
- `crates/cli/src/main.rs` - profile resolution and client building for OpsGenie/Bamboo

### Phase 5: Integration Tests
- Created `crates/cli/tests/jsm_integration.rs` (11 tests)
  - Service desk list/get, requests CRUD, request types, customers, comments, transitions, queues
- Created `crates/cli/tests/opsgenie_integration.rs` (14 tests)
  - Alerts CRUD + acknowledge/close, incidents, schedules, teams, services, heartbeats, escalations
  - Tests GenieKey authentication header
- Created `crates/cli/tests/bamboo_integration.rs` (22 tests)
  - Projects, plans, branches, builds, deployments, agents, queue, server info, artifacts
  - Full CRUD coverage for all major endpoints including artifact list/download

### Test Results
- Total: 47 new integration tests
- JSM: 11 tests passed
- OpsGenie: 14 tests passed
- Bamboo: 22 tests passed

## 2026-01-14 - C4 Model Architecture Documentation
- Created `docs/c4model.md` with comprehensive C4 model documentation
- Level 1: System Context diagram showing user, atlassian-cli, and external Atlassian APIs
- Level 2: Container diagram showing 6 Rust crates (cli, api, auth, config, output, bulk)
- Level 3: Component diagrams for each crate detailing internal structure
- Level 4: Code-level class diagram showing key structs
- Added data flow diagrams: authentication flow, bulk operation flow, request lifecycle
- Converted all diagrams from Mermaid to ASCII art for terminal compatibility
- Files created: `docs/c4model.md`

## 2025-12-26 - Confluence Draft Publishing Fix

### Bug Fixed
- **Critical**: Fixed version handling for draft page/blog post publishing
  - Root cause: `update_page()` and `update_blogpost()` always incremented version by 1
  - When publishing drafts (status: "draft" → "current"), Confluence API requires version 1
  - CLI was sending version 2, causing 400 error: "Version number must be 1 when publishing a page for the first time"

### New Commands
- `confluence page publish <PAGE_ID> --body <FILE> [--title <TITLE>] [--message <MSG>]`
  - Publishes a draft page for the first time
  - Requires `--body` flag with content file
  - Validates page is actually a draft before publishing
  - Sends version 1 as required by Confluence API

- `confluence blog publish <BLOGPOST_ID> --body <FILE> [--title <TITLE>] [--message <MSG>]`
  - Same functionality for blog posts

### Enhanced Commands
- `confluence page update` - Added new flags:
  - `--status <current|draft>` - Target status for the page
  - `--message <MSG>` - Version message for audit trail
  - Smart version handling based on status transition

- `confluence blog update` - Same enhancements as page update

### Version Logic
```
draft → current: version stays at 1 (first publish)
draft → draft: version unchanged (draft update)
current → current: version increments by 1 (normal update)
current → draft: error (cannot unpublish)
```

### Error Handling
- Improved `BadRequest` suggestions in `crates/api/src/error.rs`:
  - Detects "Version number must be 1" errors and suggests using `page publish`
  - Detects generic version conflicts and suggests fetching latest

### Files Modified
- `crates/cli/src/commands/confluence/pages.rs` - Version logic fix, added `publish_page()`, `publish_blogpost()`
- `crates/cli/src/commands/confluence/mod.rs` - Added `Publish` subcommands, `--status`, `--message` flags
- `crates/api/src/error.rs` - Improved error suggestions
- `crates/cli/tests/confluence_integration.rs` - Added 3 new tests for draft publishing

### Documentation
- `docs/26122025.md` - Full implementation plan and API reference

### Tests Added
- `test_publish_draft_page` - Verifies draft page publishes with version 1
- `test_update_published_page_increments_version` - Verifies normal updates increment version
- `test_publish_draft_blogpost` - Verifies draft blog post publishes with version 1

## 2025-12-22 - Remove Codecov Integration
- Removed Codecov upload from CI workflow
  - Coverage job was generating reports successfully but upload failing (no token)
  - Removed `codecov/codecov-action@v4` step from `.github/workflows/ci.yml`
  - Removed entire coverage job (lines 43-57)
  - CI time improved by ~6m31s per run
- Updated branch protection documentation
  - Removed `coverage` from required status checks in `.github/BRANCH_PROTECTION.md`
  - Added comment explaining removal rationale
- Note: Coverage can still be generated locally
  - Developers can install: `cargo install cargo-llvm-cov`
  - Generate HTML report: `cargo llvm-cov --workspace --html`
  - View report: `open target/llvm-cov/html/index.html`
- Files modified: `.github/workflows/ci.yml`, `.github/BRANCH_PROTECTION.md`, `todo.md`

## 2025-12-22 - Week 5-6: Quality Gates & Property Testing
- Week 5 - Branch Protection & Quality Gates:
  - Created `.github/CODEOWNERS` for automatic review requests
    - Default owner: @omar16100 for all files
    - Security-sensitive: auth crate, deny.toml, CI workflows require approval
  - Created `.github/BRANCH_PROTECTION.md` documenting required settings
    - PR reviews required (1 approval, Code Owner approval)
    - Status checks: fmt, clippy, test (ubuntu + macos) # coverage removed 2025-12-22
    - Linear history enforced (no merge commits)
    - Force push disabled, includes administrators
  - Created GitHub templates:
    - `.github/ISSUE_TEMPLATE/bug_report.md` - Bug report template
    - `.github/ISSUE_TEMPLATE/feature_request.md` - Feature request template
    - `.github/pull_request_template.md` - PR checklist and guidelines

- Week 6 - Property Testing:
  - Added `proptest` 1.4 to workspace dependencies
  - Added proptest dev-dependency to CLI crate
  - Created property tests for JQL query builder (`crates/cli/src/query/jql.rs`):
    - `escape_never_panics`: Any string can be safely escaped
    - `escaped_strings_are_quoted`: Escaped output always quoted
    - `escaping_increases_or_maintains_length`: Length preservation
    - `no_unescaped_quotes_in_output`: No injection vulnerabilities
    - `builder_with_condition_produces_output`: Non-empty with conditions
    - `multiple_conditions_use_and`: Proper AND joining
    - `in_list_never_panics`: IN lists handle arbitrary values
  - Created property tests for URL params builder (`crates/cli/src/query/url_params.rs`):
    - `encoding_never_panics`: Any key/value can be encoded
    - `no_unencoded_special_chars`: Special chars properly encoded
    - `special_chars_encoded`: &, =, #, + are percent-encoded
    - `multiple_params_separated`: Multiple params use & separator
    - `optional_none_excluded`: None values don't appear in output
    - `empty_builder_empty_output`: Empty builder produces empty string
  - All 13 property tests passing (100 cases each by default)
  - Property tests verify security against injection attacks

- Files created:
  - `.github/CODEOWNERS`
  - `.github/BRANCH_PROTECTION.md`
  - `.github/ISSUE_TEMPLATE/bug_report.md`
  - `.github/ISSUE_TEMPLATE/feature_request.md`
  - `.github/pull_request_template.md`

- Files modified:
  - `Cargo.toml` - added proptest workspace dependency
  - `crates/cli/Cargo.toml` - added proptest dev-dependency
  - `crates/cli/src/query/jql.rs` - added property_tests module
  - `crates/cli/src/query/url_params.rs` - added property_tests module

## 2025-12-22 - Week 3: Performance Benchmarking
- Added criterion benchmark framework:
  - Added `criterion` to workspace dependencies with features: html_reports, async_tokio
  - Configured for async benchmarks and HTML report generation

- Bulk operations benchmarks (`crates/bulk/benches/bulk_benchmarks.rs`):
  - Concurrency levels: Tests performance with 1, 4, 8, 16 concurrent tasks
  - Task counts: Benchmarks different batch sizes (10, 50, 100, 200 items)
  - Progress bar overhead: Measures impact of progress display on performance
  - All benchmarks use realistic async task simulation (100μs per task)

- API benchmarks (`crates/api/benches/api_benchmarks.rs`):
  - Rate limiter concurrent access: Tests mutex contention with 1-16 parallel threads
  - Pagination logic: Benchmarks has_next() and next_start() calculations
  - Page processing: Tests different page sizes (10, 50, 100, 500 items)
  - Measures pagination state management overhead

- Auth/encryption benchmarks (`crates/auth/benches/auth_benchmarks.rs`):
  - Key derivation: Benchmarks Argon2 key derivation from machine ID
  - Encryption/decryption: Tests AES-256-GCM with different payload sizes
  - Roundtrip performance: Measures full encrypt-decrypt cycles
  - Token sizes: Realistic benchmarks for short (32B), medium (64B), long (128B), JWT (512B) tokens
  - Establishes baseline for security-critical operations

- Benchmark configuration:
  - Added `criterion` dev-dependency to bulk, api, and auth crates
  - Configured `[[bench]]` targets with `harness = false` in all Cargo.toml files
  - Benchmarks run with: `cargo bench --bench <name>`
  - HTML reports generated in `target/criterion/`

- Usage:
  ```bash
  cargo bench --bench bulk_benchmarks
  cargo bench --bench api_benchmarks
  cargo bench --bench auth_benchmarks
  cargo bench  # Run all benchmarks
  ```

- Files created:
  - `crates/bulk/benches/bulk_benchmarks.rs`
  - `crates/api/benches/api_benchmarks.rs`
  - `crates/auth/benches/auth_benchmarks.rs`

- Files modified:
  - `Cargo.toml` - added criterion workspace dependency
  - `crates/bulk/Cargo.toml` - benchmark config
  - `crates/api/Cargo.toml` - benchmark config
  - `crates/auth/Cargo.toml` - benchmark config

## 2025-12-22 - Week 2: Security & Coverage
- Coverage tracking (REMOVED 2025-12-22 - see removal entry above):
  - Added `coverage` job to CI workflow in `.github/workflows/ci.yml`
  - Uses `cargo-llvm-cov` to generate LCOV coverage reports
  - Uploads to Codecov for tracking and visualization
  - Runs on every PR and main branch push
  - Status: Removed - upload was failing, no token configured

- Security scanning:
  - Created `.github/workflows/security.yml` for automated security audits
  - Runs weekly on Monday + on every PR and main push
  - Uses `cargo-audit` for vulnerability scanning (warnings only, non-blocking)
  - Uses `cargo-deny` for license compliance, dependency bans, and advisory checks

- Cargo-deny configuration:
  - Created `deny.toml` with license allowlist
  - Allowed licenses: MIT, Apache-2.0, BSD-3-Clause, MPL-2.0, Unicode-3.0
  - Configured advisory ignores for 4 unmaintained transitive dependencies:
    - backoff 0.4.0 (RUSTSEC-2025-0012) - monitoring for replacement
    - instant 0.1.13 (RUSTSEC-2024-0384) - dependency of backoff
    - number_prefix 0.4.0 (RUSTSEC-2025-0119) - dependency of indicatif
    - proc-macro-error 1.0.4 (RUSTSEC-2024-0370) - dependency of tabled
  - Multiple versions warning level (not error)

- Automated dependency updates:
  - Created `.github/dependabot.yml` for weekly dependency updates
  - Monitors both Rust crates and GitHub Actions
  - Groups all production dependencies together
  - Limits to 5 open PRs at a time

- Files modified: `.github/workflows/ci.yml`, `.github/workflows/security.yml`
- Files created: `deny.toml`, `.github/dependabot.yml`

## 2025-12-22 - Week 1 Prevention: Pre-commit Hooks & CI Optimization
- Immediate fixes (Day 1):
  - Fixed version test in `crates/cli/tests/cli_integration.rs:15` to use `env!("CARGO_PKG_VERSION")`
  - Synced `.release-please-manifest.json` from "0.1.9" to "0.2.0"
  - Ran `cargo clippy --fix` and `cargo fmt` - all 187 tests passing

- Pre-commit hooks with cargo-husky:
  - Added `cargo-husky` to workspace dependencies in `Cargo.toml`
  - Added `cargo-husky` to cli crate dev-dependencies in `crates/cli/Cargo.toml`
  - Created `.cargo-husky/hooks/pre-commit` script with fmt, clippy, and unit test checks
  - Hooks auto-install on `cargo build` (zero friction for contributors)

- CI optimization:
  - Replaced sequential job with parallel jobs (fmt, clippy, test) in `.github/workflows/ci.yml`
  - Upgraded from deprecated `actions-rs/toolchain` to `dtolnay/rust-toolchain@stable`
  - Added `Swatinem/rust-cache@v2` for dependency caching
  - Added matrix testing (ubuntu-latest + macos-latest)
  - Added `fail-fast: false` to show all failures
  - Expected CI time reduction: 2-3min → 60-90s

- Developer tooling:
  - Added `pre-commit`, `quick-check`, and `ci` targets to `Makefile`
  - Added `pre-commit`, `quick-check`, and `ci` targets to `justfile`
  - Created `CONTRIBUTING.md` with pre-commit workflow documentation

- Files modified: `crates/cli/tests/cli_integration.rs`, `.release-please-manifest.json`, `Cargo.toml`, `crates/cli/Cargo.toml`, `.github/workflows/ci.yml`, `Makefile`, `justfile`
- Files created: `.cargo-husky/hooks/pre-commit`, `CONTRIBUTING.md`

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
