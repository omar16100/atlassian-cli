# Changes Made

## 2025-11-26
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

- Added hidden password input + file-based credential storage
  - Token input now hidden via `rpassword` crate (`crates/cli/src/commands/auth.rs:232`)
  - Added file-based storage at `~/.atlassian-cli/credentials` with 600 permissions (`crates/auth/src/lib.rs:54-147`)
  - Token lookup: env var → keychain → credentials file
  - Login stores in both keychain and file; logout deletes from both
