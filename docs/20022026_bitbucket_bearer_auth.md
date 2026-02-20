# Bitbucket Bearer Auth Support

## Date: 2026-02-20

## Problem
Bitbucket app passwords deprecated (creation disabled Sep 2025, all disabled Jun 2026). atlassian-cli only used Basic auth in CLI paths. Repository/workspace access tokens require Bearer auth.

## Solution

### New CLI flag: `--bearer`
```bash
# Basic auth (API tokens - default)
atlassian-cli auth login --bitbucket --email user@example.com --token <API_TOKEN>

# Bearer auth (repository/workspace/project access tokens)
atlassian-cli auth login --bitbucket --bearer --token <ACCESS_TOKEN>

# Bearer with workspace
atlassian-cli auth login --bitbucket --bearer --workspace myteam --token <TOKEN>
```

### Token types supported
| Type | Auth method | Email required | CLI flag |
|------|------------|----------------|----------|
| Bitbucket API token | Basic | Yes | `--bitbucket` |
| Repository access token | Bearer | No | `--bitbucket --bearer` |
| Workspace access token | Bearer | No | `--bitbucket --bearer` |
| Project access token | Bearer | No | `--bitbucket --bearer` |

### Config changes
New optional field in profile:
```yaml
profiles:
  ci:
    workspace: myteam
    bitbucket_token_type: bearer  # new field, omitted = basic
```

### Endpoint changes
Bearer tokens (access tokens) cannot use `/2.0/user` (user-scoped). These endpoints now use `/2.0/workspaces` for bearer:
- `auth test --bitbucket`
- `auth status`
- `bitbucket whoami`
- `verify_auth()` pre-check

## Files modified
- `crates/config/src/lib.rs` — `bitbucket_token_type` field
- `crates/cli/src/commands/auth.rs` — `--bearer` flag, help text, error messages, bearer-aware test/status
- `crates/cli/src/main.rs` — `BitbucketProfile.is_bearer`, bearer client construction
- `crates/cli/src/commands/bitbucket/utils.rs` — `BitbucketContext.is_bearer`, verify_auth endpoint
- `crates/cli/src/commands/bitbucket/mod.rs` — passes is_bearer to context
- `crates/cli/src/commands/bitbucket/workspaces.rs` — whoami for bearer

## Tests
- 6 config tests (token_type serialization, backwards compat, roundtrip)
- 4 auth unit tests (is_bitbucket_bearer helper)
- 2 CLI integration tests (help text, bearer profile without email)
