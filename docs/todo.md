# atlassiancli TODO

## 0. Research & Validation
- [ ] Interview 3–5 Jira/Confluence/Opsgenie admins to confirm must-have workflows.
- [ ] Collect sample API payloads for each Atlassian product (Confluence, Bitbucket, JSM, Opsgenie, Bamboo, Jira admin).
- [ ] Document rate limits, auth requirements, and pagination patterns per API.
- [ ] Define primary personas and usage scenarios for launch docs.

## Phase 1 – Foundation (Weeks 1‑3) ✅ COMPLETE
### Week 1 – Project Setup
- [x] Initialize Cargo workspace (`Cargo.toml` workspace) for `atlassiancli`, create `crates/cli` binary crate, and scaffold Clap root command plus product subcommands.
- [x] Establish repo layout (`crates/cli`, `crates/api`, `crates/auth`, `crates/config`, `crates/output`, `crates/bulk`, `internal/utils`, `configs`, `docs`, `scripts`, `tests`).
- [x] Implement config loader pointing to `~/.atlcli/config.yaml` with profile selection + env var overrides.
- [x] Create `justfile`/Makefile tasks for `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test`, and `cargo install --path crates/cli`.
- [x] Add GitHub Actions CI (lint + unit tests + build matrix).
- [x] Provide config example template in `configs/config.example.yaml`.

### Week 2 – Authentication Layer
- [x] Implement API token auth (Basic auth w/ email+token and PAT styles) for all products.
- [x] Add keyring/OS credential storage with fallback to environment variable overrides.
- [x] Ship `atlcli auth login`, `logout`, `whoami`, `test` commands covering multiple profiles.
- [x] Document auth flows in docs plus troubleshooting steps.

### Week 3 – Common Infrastructure
- [x] Build shared HTTP client with retry, exponential backoff, Atlassian rate limit respect, user-agent tagging.
- [x] Add request middleware for auth injection, logging (debug traces), pagination helpers.
- [x] Implement global output renderer supporting table (default), JSON, CSV, YAML, quiet.
- [x] Create structured error type with codes + suggestions, plus debug logging flag.
- [x] Stand up bulk worker abstraction (concurrency limits, dry-run flag, progress bars, transaction log file).
- [x] Ensure unit tests cover config/auth/output modules.

## Phase 2 – Jira CLI (Weeks 4‑6) ✅ COMPLETE
- [x] Week 4: `jira` command group with issue CRUD, transitions, assign/unassign, watchers, link management, `jira search --jql`.
- [x] Week 5: Project lifecycle: list/get/create/delete, components, versions, roles; custom fields list/create/update; workflow listing/export.
- [x] Week 6: Bulk operations (transition/assign/label/export/import), automation rules (list/create/enable/disable), webhook CRUD, audit log access.
- [x] Cross-cutting: Validate pagination, add JSON schema to outputs, write integration tests with wiremock (11 tests), document examples in README.

## Phase 3 – Confluence CLI (Weeks 7‑9)
- [ ] Build `confluence` command group with shared options (`--space`, `--cql`, `--limit`, etc.) and pagination helpers.
- [ ] Implement space CRUD + permissions management.
- [ ] Implement page/blog CRUD with body file support, versioning, restrictions, labels, comments.
- [ ] Add attachment upload/download/list/delete with resumable uploads for large files.
- [ ] Deliver search commands (CQL + text) and saved query helpers.
- [ ] Add bulk operations (export, delete, label changes) with dry-run + confirmation toggles.
- [ ] Provide analytics commands for page/space view metrics.
- [ ] Write example scripts/workflows for docs (doc pipeline, backups, notifications).

## Phase 4 – Bitbucket CLI (Weeks 10‑12)
- [ ] Build `bitbucket` group covering workspaces, projects, and repos with consistent resource identifiers.
- [ ] Implement repo lifecycle (create/list/update/delete/archive) and permissions commands for users/groups.
- [ ] Implement branch/branch-protection, tags, and branch model configuration.
- [ ] Deliver pull request workflow: create/update/merge/decline, comments, tasks, approvals, reviewers, diffs.
- [ ] Add pipelines/deployments management (trigger/stop, logs, variables, schedules, deployments).
- [ ] Provide commit/source browsing helpers and artifact download.
- [ ] Implement webhooks, SSH keys, access tokens, and repo-level bulk ops (archive stale repos, delete merged branches).

## Phase 5 – JSM CLI (Weeks 13‑14)
- [ ] Implement `jsm` group: service desks CRUD, request types, portal settings.
- [ ] Deliver customer request lifecycle ops (create/update/comment/resolve/reopen) with participant + approval handling.
- [ ] Add organizations + customer management commands.
- [ ] Implement SLA visibility, reporting exports, and CSAT reporting.
- [ ] Integrate knowledge base article operations and linking to requests.
- [ ] Build Insight asset schema/object CRUD/search + linking to issues.
- [ ] Provide queue, automation rule, and announcement management.

## Phase 6 – Opsgenie CLI (Weeks 15‑16)
- [ ] Implement `opsgenie` group with alert, incident, schedule, and team subcommands.
- [ ] Build alert lifecycle coverage (create/list/get/ack/close/snooze/assign/tags/notes/priority).
- [ ] Add incident management commands (timeline, responders, status page, notes).
- [ ] Implement schedules, rotations, overrides, on-call lookups, and exports.
- [ ] Provide team management, routing rules, escalation/notification policies.
- [ ] Include integrations, heartbeat monitoring, maintenance windows, and reporting commands.
- [ ] Ensure user/contact management and forwarding rules are covered.

## Phase 7 – Bamboo CLI (Weeks 17‑18)
- [ ] Implement `bamboo` group with project, plan, build, deployment, agent, and variable subcommands.
- [ ] Support plan/branch CRUD, enable/disable, clone, delete.
- [ ] Build build execution commands (trigger, stop, queue, history, logs, artifacts, test results).
- [ ] Implement deployment projects/environments, trigger/status/history, and permissions.
- [ ] Add agent inventory (list/get, capabilities, enable/disable) and server health/info commands.
- [ ] Manage plan/deployment variables, repositories, labels, permissions, and notifications.

## Extended Jira Module (Ongoing Enhancements)
- [x] Fill missing Jira admin/work management features: project CRUD, permissions, roles, components, versions, categories, avatars.
- [x] Implement custom field management (list, create, delete).
- [ ] Implement issue type, workflow schemes, screen, priority, status, resolution management.
- [ ] Add advanced agile/analytics commands (epics, backlog, story points, burndown/velocity, cycle-time).
- [x] Provide automation rules, webhooks, audit logs.
- [ ] Provide app properties, notification schemes, permission schemes.
- [x] Include bulk operations (transition, assign, label, export/import).
- [ ] Add JQL validation tools and advanced search helpers.
- [ ] Surface system-level config (application properties, license, health checks, reindex).

## Documentation, QA & Release Readiness (Weeks 19‑20)
- [ ] Create comprehensive docs site (atlassiancli.com) with getting started, installation, auth setup, command reference (auto-gen), troubleshooting, and cookbook recipes.
- [ ] Publish example scripts (Confluence doc pipeline, multi-repo Bitbucket PR workflow, incident response automation).
- [ ] Provide quickstart templates (Docker image, GitHub Actions workflow, Jenkins shared library).
- [ ] Establish integration tests against Atlassian sandbox tenants with recorded fixtures and cleanup scripts.
- [ ] Add smoke/E2E tests for each command group validating output formats.
- [ ] Set up CI release workflow producing Linux/macOS/Windows binaries, Docker images, Homebrew tap, apt/yum packages, and `cargo install atlassiancli` instructions.
- [ ] Prepare documentation: feature comparison vs ACLI/Appfire, FAQ, roadmap, launch blog post, Atlassian Community announcement.
- [ ] Define support process (issue templates) and version/update policy (semver, `atlcli version --check-update`).
- [ ] Set up CI with lint/tests/security scans (`cargo fmt`, `cargo clippy`, `cargo test`, `cargo audit`, `cargo deny`) and release automation.
