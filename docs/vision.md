# atlassian-cli Vision

## Opportunity Overview
- The official Atlassian CLI (ACLI) only exposes limited Jira workitem and basic admin commands, leaving major gaps across Jira administration, Confluence, Bitbucket, Jira Service Management (JSM), Opsgenie, and Bamboo.
- Atlassian Cloud REST APIs already provide rich coverage for these products, enabling us to build a modern, automation-friendly CLI portfolio that DevOps, platform, ITSM, and admin teams desperately need.
- Estimated market: ~300k Atlassian Cloud customers with 10–20% potential CLI power users (30k–60k active users).

## Product Vision
Deliver **atlassian-cli**: a unified, open-source, Rust-based CLI that treats Atlassian Cloud as programmable infrastructure. The suite should feel consistent across products, unlock automation/scripting workflows, and make bulk administration tasks safe and repeatable.

### Pillars
1. **Comprehensive Coverage** – Provide first-class CLI support for Confluence, Bitbucket, JSM, Opsgenie, Bamboo, and extended Jira administration features missing from ACLI.
2. **Automation-Ready UX** – Default to scriptable interfaces: predictable exit codes, JSON/CSV outputs, dry-runs, and bulk/batch primitives.
3. **Modern Developer Experience** – Rich CLI ergonomics (Clap-based UX), intuitive help, config profiles, and secure API token management.
4. **Multi-Instance Support** – Manage multiple Atlassian instances with profile-based configuration.
5. **Extensible Foundation** – Shared core libraries and consistent abstractions so new Atlassian products or custom modules slot in quickly.

## Scope & Command Families
- **Confluence CLI** – Space/page/blog CRUD, attachments, labels, permissions, CQL search, bulk operations, analytics, webhooks.
- **Bitbucket CLI** – Repository lifecycle, branches, PR workflow, pipelines/deployments, permissions, webhooks, issues, bulk repo utilities.
- **Jira Service Management CLI** – Service desk config, request ops, organizations, SLA reporting, knowledge base, Insight assets, automation.
- **Opsgenie CLI** – Alerts/incidents, schedules, teams, integrations, users, policies, maintenance, analytics, heartbeat monitoring.
- **Bamboo CLI** – Plans/builds/deployments, agents, variables, permissions, artifacts, health/info.
- **Extended Jira Module** – Projects, workflows, custom fields, schemes, automation, audit, webhooks, analytics to fill ACLI gaps.

## Target Users & Use Cases
- DevOps/platform engineers automating deployments, releases, and infrastructure runbooks.
- Jira/Confluence admins handling configuration-as-code, migrations, and compliance reporting.
- ITSM teams operating service desks, SLAs, knowledge bases, and asset inventories.
- Incident response crews coordinating Opsgenie, Jira, and communication artifacts.
- CI/CD and release managers wiring Atlassian data into pipelines, dashboards, and chatops.

## Reference Architecture
```
atlassian-cli/
├── Cargo.toml                # Workspace manifest
├── Cargo.lock
├── crates/
│   ├── cli/
│   │   ├── src/main.rs       # Clap root + global flags
│   │   └── src/commands/     # jira, confluence, bitbucket, jsm, opsgenie, bamboo
│   ├── api/                  # REST clients per product (reqwest + serde)
│   ├── auth/                 # API token/keyring helpers
│   ├── config/               # Profiles (~/.atlassian-cli/config.yaml)
│   ├── output/               # Table/JSON/CSV/YAML renderers (tabled/serde)
│   └── bulk/                 # Worker pools, dry-run, logs (tokio + rayon)
├── internal/utils/           # Shared utilities/macros
├── configs/config.example.yaml
├── docs/                     # Reference + recipes
├── scripts/                  # Build/release tooling (justfile/cargo-make)
└── tests/                    # Integration/E2E suites (cargo nextest)
```

## Delivery Roadmap (20 Weeks)
1. **Phase 1 – Foundation (Weeks 1‑3)**
   - Week 1: Cargo workspace init, Clap scaffolding, config/profile loader, Makefile/justfile, CI/CD.
   - Week 2: Auth layer (API tokens with email+token and PAT support), multi-profile manager, keyring secure storage, `atlassian-cli auth` commands.
   - Week 3: Shared HTTP client with retry/rate limiting, logging, error handling, pagination helpers, output formatters, unit test harness.
2. **Phase 2 – Jira CLI (Weeks 4‑6)**  
   - Week 4: Issue CRUD/search, transitions, assignments.  
   - Week 5: Project lifecycle, components/versions, custom fields.  
   - Week 6: Bulk ops, automation rules, webhook support, workflow export.
3. **Phase 3 – Confluence CLI (Weeks 7‑9)**  
   - Week 7: Space + page/blog CRUD, permissions.  
   - Week 8: CQL/search, attachments, labels, comments.  
   - Week 9: Bulk export/delete/labeling, analytics.
4. **Phase 4 – Bitbucket CLI (Weeks 10‑12)**  
   - Week 10: Workspace/project/repo + branch management.  
   - Week 11: Pull request lifecycle, reviewers, comments, diffs.  
   - Week 12: Pipelines/pipelines variables, deployments, webhooks.
5. **Phase 5 – JSM CLI (Weeks 13‑14)**  
   - Week 13: Service desks, request types, customer request workflows.  
   - Week 14: Organizations, SLA/CSAT reporting, Insight assets.
6. **Phase 6 – Opsgenie CLI (Weeks 15‑16)**  
   - Week 15: Alerts/incidents plus escalation operations.  
   - Week 16: Schedules, teams, routing policies, integrations.
7. **Phase 7 – Bamboo CLI (Weeks 17‑18)**  
   - Week 17: Plans, builds, queue/history/logs, variables.  
   - Week 18: Deployments, agents, permissions, artifacts.
8. **Phase 8 – Polish & Launch (Weeks 19‑20)**  
   - Week 19: Docs, integration/E2E coverage, benchmarks, release notes.  
   - Week 20: Cross-platform binaries, package manager taps, Docker images, launch assets.

Milestones: Week 3 foundation complete; Week 6 Jira ready; Week 9 Confluence ready; Week 12 Bitbucket ready; Week 14 JSM ready; Week 16 Opsgenie ready; Week 18 Bamboo complete; Week 20 v1.0 launch.

## Technical Guardrails
- Rust (stable 1.79+), Clap 4 CLI framework, Tokio + Reqwest HTTP stack, cargo-make/just-driven tooling, GitHub Actions CI.
- Config stored at `~/.atlassian-cli/config.yaml` with multiple profiles and keyring-backed credentials.
- HTTP client middleware handles retries, pagination, Atlassian rate limits, and structured logging.
- Output modes: table (default), JSON, CSV, YAML, quiet (IDs) to support scripting.
- Bulk engine provides worker pools, dry-run mode, progress bars, and transaction logs for recovery.

## Differentiation
- **Open Source** – Fully open-source CLI with CRUD/search/bulk essentials for maximum community adoption.
- **Broader Coverage** – First-class support for Confluence, Bitbucket, JSM, Opsgenie, Bamboo (not just Jira).
- **Automation-First** – Designed for CI/CD pipelines with predictable outputs, dry-runs, and bulk operations.
- **Modern Stack** – Cloud-first Rust binary, fast startup, low memory, cross-platform.
- **Rich DX** – Intuitive commands, comprehensive help, example-driven docs, plugin-ready architecture.
- Differentiators vs ACLI/Appfire: broader product coverage, automation focus, open-source model, modern CLI ergonomics.

## Success Criteria
- All six Atlassian products plus extended Jira admin covered by coherent CLI verbs by Week 18; v1 launch at Week 20.
- CLI startup <100ms, memory <500MB, 80%+ unit coverage, passing integration suites against sandbox tenants.
- At least three documented CI/CD examples prove config-as-code workflows; JSON/CSV outputs integrate with pipelines.
- Cross-platform binaries (Linux/macOS/Windows) available via GitHub releases and package managers.

## Immediate Next Steps
1. Validate personas and must-have workflows with 3–5 target teams.  
2. Lock Rust/Clap stack decisions, confirm workspace structure, bootstrap scaffolding (`atlassian-cli` binary).
3. Stand up Week 1 deliverables (config, auth shell, CI, docs skeleton).  
4. Prepare marketing presence (atlassian-cli.com) and roadmap artifact mirroring this plan.
