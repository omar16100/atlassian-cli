# atlassian-cli

> Unified CLI for Jira, Confluence, Bitbucket & JSM

Automate your entire Atlassian Cloud stack from the terminal. Bulk operations,
dry-run mode, multiple output formats (JSON/CSV/YAML/table), and profile-based
multi-instance support.

> **Independent open-source project.** `atlassian-cli` is not affiliated with,
> endorsed by, sponsored by, or maintained by Atlassian. Atlassian maintains its
> own separate official CLI (`acli`). Atlassian, Jira, Confluence, Bitbucket, and
> Jira Service Management are trademarks of Atlassian Pty Ltd; product names are
> used here only to identify compatibility.

[![CI](https://github.com/omar16100/atlassian-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/omar16100/atlassian-cli/actions)
[![crates.io](https://img.shields.io/crates/v/atlassian-cli.svg)](https://crates.io/crates/atlassian-cli)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Documentation & guides

Full documentation, command references, and how-to guides live on the project site:

- [Jira guide](https://atlassiancli.com/jira/) — issues, projects, bulk operations, workflows
- [Confluence guide](https://atlassiancli.com/confluence/) — spaces, pages, blog posts, attachments
- [Bitbucket guide](https://atlassiancli.com/bitbucket/) — repos, branches, pull requests, pipelines
- [Jira Service Management guide](https://atlassiancli.com/jsm/) — service desks and requests
- [Installation guide](https://atlassiancli.com/install/) — Homebrew, Cargo, and pre-built binaries
- [Blog](https://atlassiancli.com/blog/) — release notes, tips, and workflow recipes

## Installation

### Homebrew (macOS/Linux)

```bash
# Add the tap (first time only)
brew tap omar16100/atlassian-cli

# Install
brew install atlassian-cli

# Verify installation
atlassian-cli --version
```

### Cargo (from crates.io)

```bash
cargo install atlassian-cli
```

### From Source

```bash
git clone https://github.com/omar16100/atlassian-cli
cd atlassian-cli
cargo install --path crates/cli
```

### Pre-built Binaries

Download the latest release for your platform from the [Releases page](https://github.com/omar16100/atlassian-cli/releases).

## Project Layout
```
crates/
  cli/       # Clap-based binary entry point
  api/       # Thin HTTP client wrapper (reqwest)
  auth/      # Encrypted credential storage (AES-256-GCM)
  config/    # YAML profile loader (~/.atlassian-cli/config.yaml)
  output/    # Output formatting helpers (table/json/yaml/csv/quiet)
  bulk/      # Concurrency + dry-run aware executor
```

## Getting Started
1. Install the Rust toolchain (rustup) and ensure `cargo` is on your PATH.
2. Fetch dependencies and verify the workspace compiles:
   ```bash
   cargo check
   ```
3. Install the CLI locally so the `atlassian-cli` binary is available (ensure `~/.cargo/bin` is in your PATH):
   ```bash
   cargo install --path crates/cli
   ```
4. Run the CLI help to inspect current subcommands:
   ```bash
   atlassian-cli --help
   atlassian-cli jira --help
   atlassian-cli confluence --help
   atlassian-cli bitbucket --help  # or use 'bb' alias
   atlassian-cli bb --help
   ```
5. Add a profile and API token:
   ```bash
   atlassian-cli auth login \
     --profile personal \
     --base-url https://example.atlassian.net \
     --email you@example.com \
     --token $ATLASSIAN_API_TOKEN \
     --default
   ```
6. List configured profiles (reads `~/.atlassian-cli/config.yaml` if present):
   ```bash
   atlassian-cli auth list
   ```
   *Tip:* Use `cp configs/config.example.yaml ~/.atlassian-cli/config.yaml` as a starting point before running the login command.
7. Try the Jira, Confluence, Bitbucket, and JSM commands (requires real data):
   ```bash
   # Jira - Issues
   atlassian-cli jira search --jql "project = DEV order by created desc" --limit 5
   atlassian-cli jira get DEV-123
   atlassian-cli jira create --project DEV --issue-type Task --summary "Test task"
   # Custom fields — discover IDs via `jira fields list`:
   atlassian-cli jira create --project DEV --issue-type Task --summary "cf test" \
     --field 'customfield_10010={"value":"Internal"}' \
     --field 'customfield_10020={"formula":"a=b"}'
   atlassian-cli jira update DEV-123 --summary "Updated summary"
   atlassian-cli jira transition DEV-123 --transition "In Progress"
   atlassian-cli jira assign DEV-123 --assignee user@example.com
   atlassian-cli jira delete DEV-123

   # Jira - Attachments
   atlassian-cli jira attachment list DEV-123
   atlassian-cli jira attachment get 10001
   atlassian-cli jira attachment download 10001                    # -> ./<server filename>
   atlassian-cli jira attachment download 10001 --output ./logo.png
   atlassian-cli jira attachment download 10001 --output - | file - # stream to stdout
   atlassian-cli jira attachment download --issue DEV-123 --dir ./attachments
   atlassian-cli jira attachment upload DEV-123 --file ./report.pdf
   atlassian-cli jira attachment delete 10001 --force

   # Jira - Projects
   atlassian-cli jira project list
   atlassian-cli jira project get DEV
   atlassian-cli jira components list --project DEV
   atlassian-cli jira versions list --project DEV

   # Jira - Roles
   atlassian-cli jira roles list --project DEV
   atlassian-cli jira roles get --project DEV --role-id 10002
   atlassian-cli jira roles actors --project DEV --role-id 10002
   atlassian-cli jira roles add-actor --project DEV --role-id 10002 --user user@example.com
   atlassian-cli jira roles remove-actor --project DEV --role-id 10002 --user user@example.com

   # Jira - Custom Fields & Workflows
   atlassian-cli jira fields list
   atlassian-cli jira workflows list
   atlassian-cli jira workflows export --name "Software Simplified Workflow"

   # Jira - Bulk Operations
   atlassian-cli jira bulk transition --jql "project = DEV AND status = Open" --transition "In Progress" --dry-run
   atlassian-cli jira bulk assign --jql "project = DEV AND assignee is EMPTY" --assignee admin@example.com
   atlassian-cli jira bulk export --jql "project = DEV" --output issues.json --format json

   # Jira - Automation & Webhooks
   atlassian-cli jira automation list
   atlassian-cli jira webhooks list
   atlassian-cli jira audit list --from 2025-01-01 --limit 100

   # Confluence - Search
   atlassian-cli confluence search cql --cql "space = DEV and type = page" --limit 5
   atlassian-cli confluence search text --query "meeting notes" --limit 10
   atlassian-cli confluence search in-space --space DEV --query "api docs"

   # Confluence - Spaces
   atlassian-cli confluence space list --limit 10
   atlassian-cli confluence space get DEV
   atlassian-cli confluence space create --key DOCS --name "Documentation" --description "Team docs"
   atlassian-cli confluence space update DEV --name "Development Space"
   atlassian-cli confluence space delete OLD --force
   atlassian-cli confluence space permissions DEV
   atlassian-cli confluence space add-permission DEV --principal user@example.com --operation read

   # Confluence - Pages
   atlassian-cli confluence page list --space DEV --limit 25
   atlassian-cli confluence page get --id 12345
   atlassian-cli confluence page create --space DEV --title "New Page" --body "<p>Content</p>"
   atlassian-cli confluence page update --id 12345 --title "Updated Title"
   atlassian-cli confluence page delete --id 12345
   atlassian-cli confluence page versions --id 12345
   atlassian-cli confluence page add-label --id 12345 --label documentation
   atlassian-cli confluence page remove-label --id 12345 --label outdated
   atlassian-cli confluence page comments --id 12345
   atlassian-cli confluence page add-comment --id 12345 --body "Great work!"
   atlassian-cli confluence page get-restrictions --id 12345
   atlassian-cli confluence page add-restriction --id 12345 --operation update --user user@example.com
   atlassian-cli confluence page remove-restriction --id 12345 --operation update --user user@example.com

   # Confluence - Blog Posts
   atlassian-cli confluence blog list --space DEV --limit 10
   atlassian-cli confluence blog get --id 67890
   atlassian-cli confluence blog create --space DEV --title "Sprint Recap" --body "<p>Summary</p>"
   atlassian-cli confluence blog update --id 67890 --title "Updated Recap"
   atlassian-cli confluence blog delete --id 67890

   # Confluence - Attachments
   atlassian-cli confluence attachment list 12345
   atlassian-cli confluence attachment get 11111
   atlassian-cli confluence attachment upload 12345 --file ./diagram.png
   atlassian-cli confluence attachment download 11111 --output ./download.png
   atlassian-cli confluence attachment delete 11111 --force

   # Confluence - Bulk Operations
   atlassian-cli confluence bulk delete --space OLD --dry-run
   atlassian-cli confluence bulk add-labels --cql "space = DEV" --labels docs,reviewed --dry-run
   atlassian-cli confluence bulk export --space DEV --output backup.json --format json

   # Confluence - Analytics
   atlassian-cli confluence analytics page-views --id 12345 --from 2025-01-01
   atlassian-cli confluence analytics space-stats --space DEV

   # Bitbucket Commands
   # Note: You can use 'bb' as a shorthand alias for 'bitbucket' in all commands below
   # Examples: atlassian-cli bb whoami  OR  atlassian-cli bitbucket whoami

   # Bitbucket - User Info
   atlassian-cli bitbucket whoami

   # Bitbucket - Repositories
   atlassian-cli bitbucket --workspace myteam repo list --limit 10
   atlassian-cli bitbucket --workspace myteam repo get api-service
   atlassian-cli bitbucket --workspace myteam repo create newrepo --name "New Repo" --private
   atlassian-cli bitbucket --workspace myteam repo update api-service --description "Updated description"
   atlassian-cli bitbucket --workspace myteam repo delete oldrepo --force

   # Bitbucket - Branches
   atlassian-cli bitbucket --workspace myteam branch list api-service
   atlassian-cli bitbucket --workspace myteam branch create api-service feature/new --from main
   atlassian-cli bitbucket --workspace myteam branch delete api-service feature/old --force
   atlassian-cli bitbucket --workspace myteam branch protect api-service --pattern "main" --kind restrict_merges --approvals 2
   atlassian-cli bitbucket --workspace myteam branch restrictions api-service

   # Bitbucket - Pull Requests
   atlassian-cli bitbucket --workspace myteam pr list api-service --state OPEN --limit 5
   atlassian-cli bitbucket --workspace myteam pr get api-service 123
   atlassian-cli bitbucket --workspace myteam pr create api-service --title "Add feature" --source feature/new --destination main
   atlassian-cli bitbucket --workspace myteam pr update api-service 123 --title "Updated title"
   atlassian-cli bitbucket --workspace myteam pr approve api-service 123
   atlassian-cli bitbucket --workspace myteam pr merge api-service 123 --strategy merge_commit
   atlassian-cli bitbucket --workspace myteam pr comments api-service 123
   atlassian-cli bitbucket --workspace myteam pr comment api-service 123 --text "Looks good!"

   # Bitbucket - Workspaces & Projects
   atlassian-cli bitbucket workspace list --limit 10
   atlassian-cli bitbucket workspace get myteam
   atlassian-cli bitbucket --workspace myteam project list
   atlassian-cli bitbucket --workspace myteam project create PROJ --name "My Project" --private
   atlassian-cli bitbucket --workspace myteam project delete PROJ --force

   # Bitbucket - Pipelines
   atlassian-cli bitbucket --workspace myteam pipeline list api-service
   atlassian-cli bitbucket --workspace myteam pipeline trigger api-service --ref-name main
   atlassian-cli bitbucket --workspace myteam pipeline stop api-service {uuid}

   # Bitbucket - Webhooks & SSH Keys
   atlassian-cli bitbucket --workspace myteam webhook list api-service
   atlassian-cli bitbucket --workspace myteam webhook create api-service --url https://example.com/hook --events repo:push
   atlassian-cli bitbucket --workspace myteam ssh-key list api-service
   atlassian-cli bitbucket --workspace myteam ssh-key add api-service --label deploy --key "ssh-rsa ..."

   # Bitbucket - Permissions & Commits
   atlassian-cli bitbucket --workspace myteam permission list api-service
   atlassian-cli bitbucket --workspace myteam permission grant api-service --user-uuid {uuid} --permission write
   atlassian-cli bitbucket --workspace myteam commit list api-service --branch main
   atlassian-cli bitbucket --workspace myteam commit diff api-service abc123
   atlassian-cli bitbucket --workspace myteam commit browse api-service --commit main --path src/

   # Bitbucket - Bulk Operations
   atlassian-cli bitbucket --workspace myteam bulk archive-repos --days 180 --dry-run
   atlassian-cli bitbucket --workspace myteam bulk delete-branches api-service --exclude feature/keep --dry-run

   # JSM
   atlassian-cli jsm servicedesk list --limit 10
   atlassian-cli jsm request list --limit 10
   atlassian-cli jsm request get SD-123
   ```

## Command Aliases

For convenience, the following command aliases are available:

| Full Command | Alias | Description |
|-------------|-------|-------------|
| `bitbucket` | `bb`  | Bitbucket commands |

Example usage:
```bash
# Full command
atlassian-cli bitbucket pipeline list --workspace myworkspace

# Using alias (shorter and faster to type)
atlassian-cli bb pipeline list --workspace myworkspace

# Both commands work identically
atlassian-cli bitbucket repo list --workspace myteam
atlassian-cli bb repo list --workspace myteam
```

## Bitbucket Authentication

Bitbucket requires a **separate scoped API token** from Jira/Confluence.

### Why Separate Tokens?

| Product | Token Type | Creation Method |
|---------|-----------|-----------------|
| Jira/Confluence | Regular API token | "Create API token" |
| **Bitbucket** | Scoped API token | "Create API token with scopes" → select Bitbucket |

Atlassian deprecated Bitbucket app passwords in favor of scoped API tokens. These tokens must be created specifically for Bitbucket with explicit permission scopes.

### Creating a Bitbucket Token

1. Go to https://id.atlassian.com/manage-profile/security/api-tokens
2. Click **"Create API token with scopes"** (not regular "Create API token")
3. Select **Bitbucket** as the app
4. Add required scopes:
   - `read:repository:bitbucket` - list/view repos
   - `write:repository:bitbucket` - create/update repos
   - `read:pullrequest:bitbucket` - view PRs
   - `admin:repository:bitbucket` - admin operations
5. Copy the token

**Note:** Bitbucket `admin:*` scopes do NOT include `read:*` permissions - add both if needed.

### Setting the Bitbucket Token

The CLI checks these environment variables in order:

| Priority | Variable | Description |
|----------|----------|-------------|
| 1 | `ATLASSIAN_CLI_BITBUCKET_TOKEN_{PROFILE}` | Profile-specific Bitbucket token |
| 2 | `ATLASSIAN_BITBUCKET_TOKEN` | Generic Bitbucket token |
| 3 | `BITBUCKET_TOKEN` | Simple fallback |
| 4 | `ATLASSIAN_CLI_TOKEN_{PROFILE}` | Falls back to regular token |

```bash
# Option 1: Profile-specific (recommended for multiple profiles)
export ATLASSIAN_CLI_BITBUCKET_TOKEN_WORK=your-bitbucket-scoped-token

# Option 2: Generic Bitbucket token (simpler for single profile)
export ATLASSIAN_BITBUCKET_TOKEN=your-bitbucket-scoped-token

# Option 3: Simple fallback
export BITBUCKET_TOKEN=your-bitbucket-scoped-token

# The CLI will use this token for Bitbucket commands
atlassian-cli bitbucket repo list --workspace myteam
```

If no Bitbucket-specific token is found, commands fall back to the regular `ATLASSIAN_CLI_TOKEN_{PROFILE}` token.

## Developer Workflow
- `make fmt` / `make clippy` / `make test` keep the workspace tidy using the standard Rust tooling stack (mirrored in `just fmt`, `just clippy`, etc.).
- `make install` (or `just install`) compiles and installs the CLI locally from `crates/cli`.

## Testing

The project includes comprehensive unit and integration tests.

### Running Tests

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p atlassian-cli-config
cargo test -p atlassian-cli-output
cargo test -p atlassian-cli-bulk

# Run integration tests
cargo test --test cli_integration
cargo test --test jira_integration
cargo test --test bitbucket_integration
cargo test --test confluence_integration

# Run tests with output
cargo test -- --nocapture
```

### Test Coverage

- **Config crate**: 12 tests covering profile management, YAML parsing, and error handling
- **Output crate**: 22 tests for all output formats (table/JSON/CSV/YAML/quiet)
- **Bulk crate**: 10 tests for concurrency, dry-run, error handling, and progress tracking
- **Auth crate**: 3 tests for credential helpers
- **CLI integration tests**: 17 tests validating CLI commands and help output
- **Jira integration tests**: 9 tests with wiremock for issues, projects, audit, webhooks, and error handling
- **Bitbucket integration tests**: 15 tests for repos, branches, PRs, approvals, and branch protection
- **Confluence integration tests**: 11 tests for spaces, pages, search, and bulk operations
- **Total**: 99 passing tests

### CI/CD

GitHub Actions workflow runs on every push/PR:
- `cargo fmt --check` - Code formatting
- `cargo clippy -- -D warnings` - Linting
- `cargo test --workspace` - Full test suite
- Multi-platform builds (Linux, macOS, Windows)

## Current Status

### Completed Features

**Phase 1 - Foundation** (100% complete)
- ✅ Cargo workspace with modular crate structure
- ✅ Config loader with profile support (~/.atlassian-cli/config.yaml)
- ✅ API token authentication (Basic auth with email+token)
- ✅ HTTP client with retry, rate limiting, and pagination
- ✅ Multi-format output (table/JSON/CSV/YAML/quiet)
- ✅ Bulk operation executor with concurrency control
- ✅ Comprehensive unit tests (44 tests)
- ✅ CI/CD with GitHub Actions

**Phase 2 - Jira CLI** (100% complete)
- ✅ Issue CRUD operations (create/read/update/delete/search/transition)
- ✅ Issue management (assign/unassign, watchers, links, comments)
- ✅ Project lifecycle (list/get/create/update/delete)
- ✅ Components and versions management
- ✅ Custom fields (list/get/create/delete)
- ✅ Workflows (list/get/export)
- ✅ Bulk operations (transition/assign/label/export/import)
- ✅ Automation rules (list/get/create/update/enable/disable)
- ✅ Webhooks (full CRUD + test)
- ✅ Audit log access (list/export)
- ✅ Role management (list/get/actors/add-actor/remove-actor)
- ✅ Integration tests with API mocking (9 tests)

**Phase 4 - Bitbucket CLI** (100% complete)
- ✅ Repository CRUD operations (list/get/create/update/delete)
- ✅ Branch management (list/get/create/delete/protect/unprotect)
- ✅ Pull request workflow (list/get/create/update/merge/decline)
- ✅ PR approvals, comments, and reviewers
- ✅ Branch protection and restrictions
- ✅ Workspace operations (list/get)
- ✅ Project management (list/get/create/update/delete)
- ✅ Pipeline operations (list/get/trigger/stop/logs)
- ✅ Webhooks (list/create/delete)
- ✅ SSH deploy keys (list/add/delete)
- ✅ Repository permissions (list/grant/revoke)
- ✅ Commit operations (list/get/diff/browse)
- ✅ Bulk operations (archive stale repos, delete merged branches)
- ✅ User info (whoami)
- ✅ Integration tests with API mocking (15 tests)

**Phase 3 - Confluence CLI** (100% complete)
- ✅ Space operations (list/get/create/update/delete/permissions)
- ✅ Page management (CRUD, versions, labels, comments, restrictions)
- ✅ Blog posts (list/get/create/update/delete)
- ✅ Attachments (list/get/upload/download/delete)
- ✅ Search (CQL, text, in-space)
- ✅ Bulk operations (delete, add-labels, export)
- ✅ Analytics (page-views, space-stats)
- ✅ Integration tests with API mocking (11 tests)

**Additional Products** (Partial)
- ✅ JSM CLI: Service desk and request operations
- ⏳ Opsgenie CLI: Placeholder
- ⏳ Bamboo CLI: Placeholder

### Next Steps
- Complete Phase 5: JSM CLI (organizations, SLA, Insight assets)
- Complete Phase 6: Opsgenie CLI
- Complete Phase 7: Bamboo CLI
- Add recipe documentation for common workflows
- Package releases (binaries, Docker, Homebrew)
