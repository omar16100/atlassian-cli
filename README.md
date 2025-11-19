# atlassiancli

Rust-based multi-product Atlassian Cloud CLI. The current skeleton sets up the workspace, shared crates, and placeholder commands so execution flow, config loading, and output rendering are ready for product-specific implementations.

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
3. Run the CLI help to inspect current subcommands:
   ```bash
   cargo run -- --help
   ```
4. Add a profile and API token:
   ```bash
   cargo run -- auth login \
     --profile personal \
     --base-url https://example.atlassian.net \
     --email you@example.com \
     --token $ATLASSIAN_API_TOKEN \
     --default
   ```
5. List configured profiles (reads `~/.atlcli/config.yaml` if present):
   ```bash
   cargo run -- auth list
   ```
   *Tip:* Use `cp configs/config.example.yaml ~/.atlcli/config.yaml` as a starting point before running the login command.
6. Try the Jira, Confluence, Bitbucket, and JSM commands (requires real data):
   ```bash
   # Jira - Issues
   cargo run -- jira search --jql "project = DEV order by created desc" --limit 5
   cargo run -- jira get DEV-123
   cargo run -- jira create --project DEV --issue-type Task --summary "Test task"
   cargo run -- jira update DEV-123 --summary "Updated summary"
   cargo run -- jira transition DEV-123 --transition "In Progress"
   cargo run -- jira assign DEV-123 --assignee user@example.com
   cargo run -- jira delete DEV-123

   # Jira - Projects
   cargo run -- jira project list
   cargo run -- jira project get DEV
   cargo run -- jira components list --project DEV
   cargo run -- jira versions list --project DEV

   # Jira - Custom Fields & Workflows
   cargo run -- jira fields list
   cargo run -- jira workflows list
   cargo run -- jira workflows export --name "Software Simplified Workflow"

   # Jira - Bulk Operations
   cargo run -- jira bulk transition --jql "project = DEV AND status = Open" --transition "In Progress" --dry-run
   cargo run -- jira bulk assign --jql "project = DEV AND assignee is EMPTY" --assignee admin@example.com
   cargo run -- jira bulk export --jql "project = DEV" --output issues.json --format json

   # Jira - Automation & Webhooks
   cargo run -- jira automation list
   cargo run -- jira webhooks list
   cargo run -- jira audit list --from 2025-01-01 --limit 100

   # Confluence
   cargo run -- confluence search --cql "space = DEV and type = page" --limit 5
   cargo run -- confluence space list --limit 10

   # Bitbucket
   cargo run -- bitbucket --workspace myteam repo list --limit 10
   cargo run -- bitbucket --workspace myteam repo get api-service
   cargo run -- bitbucket --workspace myteam pr list api-service --state OPEN --limit 5

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
cargo test -p atlassiancli-config
cargo test -p atlassiancli-output
cargo test -p atlassiancli-bulk

# Run integration tests
cargo test --test cli_integration
cargo test --test jira_integration

# Run tests with output
cargo test -- --nocapture
```

### Test Coverage

- **Config crate**: 12 tests covering profile management, YAML parsing, and error handling
- **Output crate**: 22 tests for all output formats (table/JSON/CSV/YAML/quiet)
- **Bulk crate**: 10 tests for concurrency, dry-run, error handling, and progress tracking
- **CLI integration tests**: 7 tests validating CLI commands and help output
- **Jira integration tests**: 11 tests with wiremock for issues, projects, audit, webhooks, and error handling
- **Total**: 62 passing tests

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

**Additional Products** (Partial)
- ✅ Bitbucket CLI: Repo operations, PR management
- ✅ JSM CLI: Service desk and request operations
- ⏳ Confluence CLI: Basic structure
- ⏳ Opsgenie CLI: Placeholder
- ⏳ Bamboo CLI: Placeholder

### Next Steps
- Complete Phase 3: Confluence CLI full implementation
- Complete Phase 4: Bitbucket CLI (branches, pipelines, permissions)
- Complete Phase 5: JSM CLI (organizations, SLA, Insight assets)
- Complete Phase 6: Opsgenie CLI
- Complete Phase 7: Bamboo CLI
- Add recipe documentation for common workflows
- Package releases (binaries, Docker, Homebrew)
