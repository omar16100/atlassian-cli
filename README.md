# atlassian-cli

Rust-based multi-product Atlassian Cloud CLI. The current skeleton sets up the workspace, shared crates, and placeholder commands so execution flow, config loading, and output rendering are ready for product-specific implementations.

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
  auth/      # Keyring-backed credential helpers
  config/    # YAML profile loader (~/.atlcli/config.yaml)
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
6. List configured profiles (reads `~/.atlcli/config.yaml` if present):
   ```bash
   atlassian-cli auth list
   ```
   *Tip:* Use `cp configs/config.example.yaml ~/.atlcli/config.yaml` as a starting point before running the login command.
7. Try the Jira, Confluence, Bitbucket, and JSM commands (requires real data):
   ```bash
   # Jira - Issues
   atlassian-cli jira search --jql "project = DEV order by created desc" --limit 5
   atlassian-cli jira get DEV-123
   atlassian-cli jira create --project DEV --issue-type Task --summary "Test task"
   atlassian-cli jira update DEV-123 --summary "Updated summary"
   atlassian-cli jira transition DEV-123 --transition "In Progress"
   atlassian-cli jira assign DEV-123 --assignee user@example.com
   atlassian-cli jira delete DEV-123

   # Jira - Projects
   atlassian-cli jira project list
   atlassian-cli jira project get DEV
   atlassian-cli jira components list --project DEV
   atlassian-cli jira versions list --project DEV

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

   # Confluence
   atlassian-cli confluence search --cql "space = DEV and type = page" --limit 5
   atlassian-cli confluence space list --limit 10

   # Bitbucket - Repositories
   atlassian-cli bitbucket --workspace myteam repo list --limit 10
   atlassian-cli bitbucket --workspace myteam repo get api-service
   atlassian-cli bitbucket --workspace myteam repo create newrepo --name "New Repo" --private
   atlassian-cli bitbucket --workspace myteam repo update api-service --description "Updated description"
   cargo run -- bitbucket --workspace myteam repo delete oldrepo --force

   # Bitbucket - Branches
   cargo run -- bitbucket --workspace myteam branch list api-service
   cargo run -- bitbucket --workspace myteam branch create api-service feature/new --from main
   cargo run -- bitbucket --workspace myteam branch delete api-service feature/old --force
   cargo run -- bitbucket --workspace myteam branch protect api-service --pattern "main" --kind restrict_merges --approvals 2
   cargo run -- bitbucket --workspace myteam branch restrictions api-service

   # Bitbucket - Pull Requests
   cargo run -- bitbucket --workspace myteam pr list api-service --state OPEN --limit 5
   cargo run -- bitbucket --workspace myteam pr get api-service 123
   cargo run -- bitbucket --workspace myteam pr create api-service --title "Add feature" --source feature/new --destination main
   cargo run -- bitbucket --workspace myteam pr update api-service 123 --title "Updated title"
   cargo run -- bitbucket --workspace myteam pr approve api-service 123
   cargo run -- bitbucket --workspace myteam pr merge api-service 123 --strategy merge_commit
   cargo run -- bitbucket --workspace myteam pr comments api-service 123
   cargo run -- bitbucket --workspace myteam pr comment api-service 123 --text "Looks good!"

   # Bitbucket - Workspaces & Projects
   cargo run -- bitbucket workspace list --limit 10
   cargo run -- bitbucket workspace get myteam
   cargo run -- bitbucket --workspace myteam project list
   cargo run -- bitbucket --workspace myteam project create PROJ --name "My Project" --private
   cargo run -- bitbucket --workspace myteam project delete PROJ --force

   # Bitbucket - Pipelines
   cargo run -- bitbucket --workspace myteam pipeline list api-service
   cargo run -- bitbucket --workspace myteam pipeline trigger api-service --ref-name main
   cargo run -- bitbucket --workspace myteam pipeline stop api-service {uuid}

   # Bitbucket - Webhooks & SSH Keys
   cargo run -- bitbucket --workspace myteam webhook list api-service
   cargo run -- bitbucket --workspace myteam webhook create api-service --url https://example.com/hook --events repo:push
   cargo run -- bitbucket --workspace myteam ssh-key list api-service
   cargo run -- bitbucket --workspace myteam ssh-key add api-service --label deploy --key "ssh-rsa ..."

   # Bitbucket - Permissions & Commits
   cargo run -- bitbucket --workspace myteam permission list api-service
   cargo run -- bitbucket --workspace myteam permission grant api-service --user-uuid {uuid} --permission write
   cargo run -- bitbucket --workspace myteam commit list api-service --branch main
   cargo run -- bitbucket --workspace myteam commit diff api-service abc123
   cargo run -- bitbucket --workspace myteam commit browse api-service --commit main --path src/

   # Bitbucket - Bulk Operations
   cargo run -- bitbucket --workspace myteam bulk archive-repos --days 180 --dry-run
   cargo run -- bitbucket --workspace myteam bulk delete-branches api-service --exclude feature/keep --dry-run

   # JSM
   cargo run -- jsm servicedesk list --limit 10
   cargo run -- jsm request list --limit 10
   cargo run -- jsm request get SD-123
   ```

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

# Run tests with output
cargo test -- --nocapture
```

### Test Coverage

- **Config crate**: 12 tests covering profile management, YAML parsing, and error handling
- **Output crate**: 22 tests for all output formats (table/JSON/CSV/YAML/quiet)
- **Bulk crate**: 10 tests for concurrency, dry-run, error handling, and progress tracking
- **CLI integration tests**: 7 tests validating CLI commands and help output
- **Jira integration tests**: 11 tests with wiremock for issues, projects, audit, webhooks, and error handling
- **Bitbucket integration tests**: 14 tests for repos, branches, PRs, approvals, and branch protection
- **Total**: 76 passing tests

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
- ✅ Config loader with profile support (~/.atlcli/config.yaml)
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
- ✅ Integration tests with API mocking (11 tests)

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
- ✅ Integration tests with API mocking (14 tests)

**Additional Products** (Partial)
- ✅ JSM CLI: Service desk and request operations
- ⏳ Confluence CLI: Basic structure
- ⏳ Opsgenie CLI: Placeholder
- ⏳ Bamboo CLI: Placeholder

### Next Steps
- Complete Phase 3: Confluence CLI full implementation
- Complete Phase 5: JSM CLI (organizations, SLA, Insight assets)
- Complete Phase 6: Opsgenie CLI
- Complete Phase 7: Bamboo CLI
- Add recipe documentation for common workflows
- Package releases (binaries, Docker, Homebrew)
