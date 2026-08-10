# C4 Model: atlassian-cli

Architecture documentation using the C4 model for the atlassian-cli Rust workspace.

## Level 1: System Context

High-level view showing atlassian-cli and its interactions with users and external systems.

```
                              ┌─────────────────────┐
                              │   Developer/DevOps  │
                              │       (User)        │
                              └──────────┬──────────┘
                                         │
                                         │ Terminal Commands
                                         ▼
                              ┌─────────────────────┐
                              │   atlassian-cli     │
                              │                     │
                              │ Unified CLI for     │
                              │ Atlassian Products  │
                              └──────────┬──────────┘
                                         │
                                         │ REST API (HTTPS/JSON)
          ┌──────────────────────────────┼──────────────────────────────┐
          │                    │                    │                   │
          ▼                    ▼                    ▼                   ▼
┌───────────────────┐ ┌───────────────────┐ ┌───────────────────┐ ┌───────────────────┐
│    Jira Cloud     │ │ Confluence Cloud  │ │  Bitbucket Cloud  │ │      OpsGenie     │
│                   │ │                   │ │                   │ │                   │
│ Issue tracking &  │ │ Documentation &   │ │ Git repos &       │ │ Incident & alert  │
│ project mgmt      │ │ knowledge base    │ │ CI/CD pipelines   │ │ management        │
└───────────────────┘ └───────────────────┘ └───────────────────┘ └───────────────────┘
          │                                                                 │
          ▼                                                                 │
┌───────────────────┐                                           ┌───────────────────┐
│        JSM        │                                           │      Bamboo       │
│                   │                                           │                   │
│ ITSM & service    │                                           │ CI/CD build &     │
│ desk              │                                           │ deployment        │
└───────────────────┘                                           └───────────────────┘
```

### External Systems

| System | Base URL | Purpose |
|--------|----------|---------|
| Jira Cloud | `https://{instance}.atlassian.net/rest/api/3/` | Issues, projects, workflows, automation |
| Confluence Cloud | `https://{instance}.atlassian.net/wiki/api/v2/` | Pages, spaces, attachments |
| Bitbucket Cloud | `https://api.bitbucket.org/2.0/` | Repos, branches, PRs, pipelines |
| JSM | `https://{instance}.atlassian.net/rest/servicedeskapi/` | Service desks, requests, queues, SLAs |
| OpsGenie | `https://api.opsgenie.com/v2/` (EU: `api.eu.opsgenie.com`) | Alerts, incidents, schedules, teams, heartbeats |
| Bamboo | `https://{instance}/rest/api/latest/` | Plans, builds, deployments, agents |

---

## Level 2: Container Diagram

Shows the internal structure of atlassian-cli as a Rust workspace with 6 crates.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                            atlassian-cli Workspace                              │
│                                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐      │
│  │    cli      │───▶│    api      │───▶│   auth      │    │   config    │      │
│  │  (Binary)   │    │  (Library)  │    │  (Library)  │    │  (Library)  │      │
│  │             │    │             │    │             │    │             │      │
│  │ Entry point │    │ HTTP client │    │ Credential  │    │ YAML profile│      │
│  │ & commands  │    │ & retry     │    │ encryption  │    │ management  │      │
│  └──────┬──────┘    └─────────────┘    └──────┬──────┘    └──────┬──────┘      │
│         │                                      │                  │             │
│         │           ┌─────────────┐            │                  │             │
│         │──────────▶│   output    │            │                  │             │
│         │           │  (Library)  │            │                  │             │
│         │           │             │            │                  │             │
│         │           │ Multi-format│            │                  │             │
│         │           │ rendering   │            │                  │             │
│         │           └─────────────┘            │                  │             │
│         │                                      │                  │             │
│         │           ┌─────────────┐            │                  │             │
│         └──────────▶│    bulk     │            │                  │             │
│                     │  (Library)  │            │                  │             │
│                     │             │            │                  │             │
│                     │ Concurrent  │            │                  │             │
│                     │ executor    │            │                  │             │
│                     └─────────────┘            │                  │             │
│                                                │                  │             │
└────────────────────────────────────────────────┼──────────────────┼─────────────┘
                                                 │                  │
                           ┌─────────────────────┘                  │
                           │                                        │
                           ▼                                        ▼
                 ┌──────────────────┐                    ┌──────────────────┐
                 │ credentials.enc  │                    │   config.yaml    │
                 │                  │                    │                  │
                 │ ~/.atlassian-cli/│                    │ ~/.atlassian-cli/│
                 │ (Encrypted)      │                    │ (YAML)           │
                 └──────────────────┘                    └──────────────────┘
                           │
                           │ REST API (HTTPS)
                           ▼
                 ┌──────────────────────────────────────────┐
                 │         Atlassian Product APIs           │
                 │  Jira, Confluence, Bitbucket, JSM,       │
                 │  OpsGenie, Bamboo                        │
                 └──────────────────────────────────────────┘
```

### Crate Responsibilities

| Crate | Type | Lines | Purpose |
|-------|------|-------|---------|
| `cli` | Binary | ~1100 | Command parsing, routing, orchestration |
| `api` | Library | ~800 | HTTP client wrapper with resilience |
| `auth` | Library | ~400 | Credential encryption & storage |
| `config` | Library | ~300 | YAML configuration management |
| `output` | Library | ~250 | Output format rendering |
| `bulk` | Library | ~350 | Concurrent execution engine |

---

## Level 3: Component Diagrams

### CLI Crate Components

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              cli Crate                                     │
│                                                                            │
│  ┌──────────────────┐                                                      │
│  │     main.rs      │                                                      │
│  │   (Entry Point)  │                                                      │
│  │                  │                                                      │
│  │ CLI bootstrap &  │                                                      │
│  │ clap parsing     │                                                      │
│  └────────┬─────────┘                                                      │
│           │                                                                │
│           │ Routes to                                                      │
│           ▼                                                                │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │                        commands/                                    │   │
│  │                                                                     │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │   │
│  │  │  jira/   │  │confluence│  │bitbucket/│  │   jsm/   │            │   │
│  │  │          │  │    /     │  │          │  │          │            │   │
│  │  │ Issues   │  │ Pages    │  │ Repos    │  │ Service  │            │   │
│  │  │ Projects │  │ Spaces   │  │ Branches │  │ desks    │            │   │
│  │  │ Workflows│  │ Search   │  │ PRs      │  │ Requests │            │   │
│  │  │ Webhooks │  │ Analytics│  │ Pipelines│  │ Queues   │            │   │
│  │  └────┬─────┘  └────┬─────┘  └──────────┘  └──────────┘            │   │
│  │       │             │                                               │   │
│  │  ┌──────────┐  ┌──────────┐                                        │   │
│  │  │opsgenie/ │  │ bamboo/  │                                        │   │
│  │  │          │  │          │                                        │   │
│  │  │ Alerts   │  │ Plans    │                                        │   │
│  │  │ Incidents│  │ Builds   │                                        │   │
│  │  │ Schedules│  │ Deploys  │                                        │   │
│  │  │ Teams    │  │ Agents   │                                        │   │
│  │  └──────────┘  └──────────┘                                        │   │
│  │                                                                     │   │
│  └───────┼─────────────┼───────────────────────────────────────────────┘   │
│          │             │                                                    │
│          │ Uses        │ Uses                                               │
│          ▼             ▼                                                    │
│  ┌────────────────────────────────┐                                        │
│  │           query/               │                                        │
│  │                                │                                        │
│  │  ┌──────────┐  ┌──────────┐   │                                        │
│  │  │  jql.rs  │  │  cql.rs  │   │                                        │
│  │  │          │  │          │   │                                        │
│  │  │ JQL      │  │ CQL      │   │                                        │
│  │  │ Builder  │  │ Builder  │   │                                        │
│  │  └──────────┘  └──────────┘   │                                        │
│  └────────────────────────────────┘                                        │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Command Modules

**Shared (`commands/`):**
- `api.rs` - Raw authenticated REST passthrough (`jira api`), product-agnostic;
  built on `ApiClient::request_raw`, which returns status/headers/body with no
  status-to-error mapping

**Jira (`commands/jira/`):**
- `issues.rs` - CRUD, search, transitions, assignments
- `attachments.rs` - Attachment list/get/download/upload/delete, plus the shared
  `JiraAttachment` model used by `issues.rs` for `issue get`
- `projects.rs` - Project management, roles
- `fields_workflows.rs` - Custom fields, workflow transitions
- `automation.rs` - Automation rules management
- `webhooks.rs` - Webhook CRUD
- `audit.rs` - Audit log retrieval
- `bulk.rs` - Bulk issue operations

**Confluence (`commands/confluence/`):**
- `pages.rs` - Page CRUD, publishing drafts
- `spaces.rs` - Space management
- `attachments.rs` - File attachments
- `search.rs` - CQL-based search
- `analytics.rs` - Page view analytics
- `bulk.rs` - Bulk page operations

**Bitbucket (`commands/bitbucket/`):**
- `repos.rs` - Repository management
- `branches.rs` - Branch operations
- `pullrequests.rs` - PR management
- `pipelines.rs` - CI/CD pipeline control
- `commits.rs` - Commit history
- `permissions.rs` - Access control
- `webhooks.rs` - Webhook management
- `bulk.rs` - Bulk repository operations

**JSM (`commands/jsm/`):**
- `servicedesk.rs` - Service desk management
- `requests.rs` - Request CRUD, transitions
- `queues.rs` - Queue management
- `customers.rs` - Customer management
- `organizations.rs` - Organization management
- `approvals.rs` - Approval workflows
- `sla.rs` - SLA operations
- `knowledgebase.rs` - KB article search

**OpsGenie (`commands/opsgenie/`):**
- `alerts.rs` - Alert CRUD, acknowledge, close, escalate
- `incidents.rs` - Incident management
- `schedules.rs` - On-call schedules, timeline
- `teams.rs` - Team management
- `escalations.rs` - Escalation policies
- `services.rs` - Service definitions
- `heartbeats.rs` - Health monitoring
- `oncall.rs` - Who is on-call queries

**Bamboo (`commands/bamboo/`):**
- `projects.rs` - Project management
- `plans.rs` - Build plan management
- `builds.rs` - Build execution and results
- `branches.rs` - Branch management
- `deployments.rs` - Deployment projects and environments
- `agents.rs` - Build agent management
- `queues.rs` - Build and deployment queues
- `artifacts.rs` - Artifact management

---

### API Crate Components

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              api Crate                                     │
│                                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │                         ApiClient                                   │   │
│  │                      (HTTP Client Core)                             │   │
│  │                                                                     │   │
│  │  - Request execution                                                │   │
│  │  - Auth handling (Basic/Bearer/GenieKey)                            │   │
│  │  - HTTPS enforcement                                                │   │
│  │  - SSRF protection                                                  │   │
│  │  - JSON/XML response handling                                       │   │
│  └───────────────────────────────┬────────────────────────────────────┘   │
│                                  │                                         │
│           ┌──────────────────────┼──────────────────────┐                 │
│           │                      │                      │                 │
│           ▼                      ▼                      ▼                 │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐        │
│  │   RetryConfig    │  │   RateLimiter    │  │    Paginator     │        │
│  │                  │  │                  │  │                  │        │
│  │ - Exponential    │  │ - x-ratelimit    │  │ - Multi-page     │        │
│  │   backoff        │  │   header tracking│  │   aggregation    │        │
│  │ - Max 3 attempts │  │ - Auto-throttle  │  │ - Streaming      │        │
│  │ - 500ms-30s      │  │ - 80% warning    │  │   collection     │        │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘        │
│                                  │                                         │
│                                  ▼                                         │
│                        ┌──────────────────┐                               │
│                        │     ApiError     │                               │
│                        │                  │                               │
│                        │ - Error types    │                               │
│                        │ - is_retryable() │                               │
│                        │ - suggestion()   │                               │
│                        └──────────────────┘                               │
│                                  │                                         │
│                                  │ HTTPS                                   │
│                                  ▼                                         │
│                        ┌──────────────────┐                               │
│                        │  Atlassian APIs  │                               │
│                        └──────────────────┘                               │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Key Abstractions

```rust
// ApiClient - Core HTTP client
pub struct ApiClient {
    client: reqwest::Client,
    base_url: Url,
    auth: AuthMethod,
    retry_config: RetryConfig,
    rate_limiter: RateLimiter,
}

// Methods: get<T>, post<T>, put<T>, delete<T>, get_text
// Features: HTTPS enforcement, SSRF protection, automatic retry
```

---

### Auth Crate Components

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              auth Crate                                    │
│                                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │                     Credential Storage                              │   │
│  │                                                                     │   │
│  │  - set_secret() / get_secret()     (plaintext)                      │   │
│  │  - set_secret_encrypted() / get_secret_encrypted()  (AES)           │   │
│  └───────────────────────────────┬────────────────────────────────────┘   │
│                                  │                                         │
│                    ┌─────────────┴─────────────┐                          │
│                    │                           │                          │
│                    ▼                           ▼                          │
│  ┌───────────────────────────┐  ┌───────────────────────────┐            │
│  │       Encryption          │  │        Migration          │            │
│  │                           │  │                           │            │
│  │  - AES-256-GCM            │  │  - migrate_plaintext_     │            │
│  │  - Argon2 key derivation  │  │    to_encrypted()         │            │
│  │  - SecretString wrapper   │  │  - Automatic on first use │            │
│  └─────────────┬─────────────┘  └───────────────────────────┘            │
│                │                                                          │
│                ▼                                                          │
│  ┌───────────────────────────┐                                           │
│  │    credentials.enc        │                                           │
│  │                           │                                           │
│  │  ~/.atlassian-cli/        │                                           │
│  │  (0600 permissions)       │                                           │
│  └───────────────────────────┘                                           │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Security Features

- **AES-256-GCM** encryption for tokens at rest
- **Argon2** key derivation from user passphrase
- **SecretString** wrapper prevents accidental logging
- **0600 permissions** on credential files (Unix)
- Secure file deletion with zero-overwrite

---

### Config Crate Components

```
┌────────────────────────────────────────────────────────────────────────────┐
│                             config Crate                                   │
│                                                                            │
│  ┌───────────────────────────┐      ┌───────────────────────────┐         │
│  │          Config           │      │         Profile           │         │
│  │                           │      │                           │         │
│  │  - default_profile        │─────▶│  - base_url               │         │
│  │  - profiles: IndexMap     │ 1:N  │  - email                  │         │
│  │                           │      │  - workspace              │         │
│  └─────────────┬─────────────┘      └───────────────────────────┘         │
│                │                                                           │
│                │ Loads/Saves                                               │
│                ▼                                                           │
│  ┌───────────────────────────┐                                            │
│  │       Config Loader       │                                            │
│  │                           │                                            │
│  │  - YAML deserialization   │                                            │
│  │  - File creation          │                                            │
│  │  - Migration from old dir │                                            │
│  └─────────────┬─────────────┘                                            │
│                │                                                           │
│                ▼                                                           │
│  ┌───────────────────────────┐                                            │
│  │      config.yaml          │                                            │
│  │                           │                                            │
│  │  ~/.atlassian-cli/        │                                            │
│  └───────────────────────────┘                                            │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Configuration Structure

```yaml
# ~/.atlassian-cli/config.yaml
default_profile: work
profiles:
  work:
    base_url: https://company.atlassian.net
    email: user@company.com
  personal:
    base_url: https://personal.atlassian.net
    workspace: my-workspace  # Bitbucket workspace
```

---

### Output Crate Components

```
┌────────────────────────────────────────────────────────────────────────────┐
│                             output Crate                                   │
│                                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │                       OutputRenderer                                │   │
│  │                    (Format-agnostic core)                           │   │
│  └───────────────────────────────┬────────────────────────────────────┘   │
│                                  │                                         │
│           ┌──────────────────────┼──────────────────────┐                 │
│           │           │          │          │           │                 │
│           ▼           ▼          ▼          ▼           ▼                 │
│  ┌─────────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐         │
│  │   Table     │ │  JSON   │ │  YAML   │ │   CSV   │ │  Quiet  │         │
│  │  Formatter  │ │Formatter│ │Formatter│ │Formatter│ │Formatter│         │
│  │             │ │         │ │         │ │         │ │         │         │
│  │  (default)  │ │--output │ │--output │ │--output │ │--output │         │
│  │  tabled     │ │  json   │ │  yaml   │ │  csv    │ │  quiet  │         │
│  └─────────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘         │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

---

### Bulk Crate Components

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              bulk Crate                                    │
│                                                                            │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │                        BulkExecutor                                 │   │
│  │                   (Concurrent task runner)                          │   │
│  │                                                                     │   │
│  │  - Semaphore-limited concurrency                                    │   │
│  │  - run<T, F>() - void operations                                    │   │
│  │  - execute_with_results<T, R, F>() - with return values             │   │
│  └───────────────────────────────┬────────────────────────────────────┘   │
│                                  │                                         │
│           ┌──────────────────────┼──────────────────────┐                 │
│           │                      │                      │                 │
│           ▼                      ▼                      ▼                 │
│  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐        │
│  │    BulkConfig    │  │    BulkResult    │  │     Progress     │        │
│  │                  │  │                  │  │                  │        │
│  │  - concurrency:4 │  │  - successes     │  │  - indicatif     │        │
│  │  - dry_run       │  │  - failures      │  │  - progress bar  │        │
│  │  - fail_fast     │  │  - indices       │  │  - status updates│        │
│  └──────────────────┘  └──────────────────┘  └──────────────────┘        │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Bulk Execution Model

```rust
pub struct BulkExecutor {
    concurrency: usize,      // Semaphore-limited (default: 4)
    dry_run: bool,           // Skip actual API calls
    fail_fast: bool,         // Stop on first error
}

// Methods:
// - run<T, F>() - Execute void operations
// - execute_with_results<T, R, F>() - Execute with return values
```

---

## Level 4: Code (Key Structures)

### Core Structs

```
┌──────────────────────────────────────────────────────────────────────────┐
│                            Class Diagram                                  │
│                                                                          │
│  ┌─────────────────────────┐           ┌─────────────────────────┐      │
│  │       ApiClient         │           │      BulkExecutor       │      │
│  ├─────────────────────────┤           ├─────────────────────────┤      │
│  │ - client: reqwest       │           │ - concurrency: usize    │      │
│  │ - base_url: Url         │           │ - dry_run: bool         │      │
│  │ - auth: AuthMethod      │           │ - fail_fast: bool       │      │
│  │ - retry_config          │◀──────────│                         │      │
│  │ - rate_limiter          │   uses    ├─────────────────────────┤      │
│  ├─────────────────────────┤           │ + run<T,F>()            │      │
│  │ + get<T>(path)          │           │ + execute_with_results()│      │
│  │ + post<T>(path, body)   │           └─────────────────────────┘      │
│  │ + put<T>(path, body)    │                                            │
│  │ + delete(path)          │                                            │
│  └────────────┬────────────┘                                            │
│               │                                                          │
│               │ contains                                                 │
│               ▼                                                          │
│  ┌─────────────────────────┐           ┌─────────────────────────┐      │
│  │      RetryConfig        │           │       RateLimiter       │      │
│  ├─────────────────────────┤           ├─────────────────────────┤      │
│  │ - max_retries: u32      │           │ - limit: u32            │      │
│  │ - initial_backoff: ms   │           │ - remaining: u32        │      │
│  │ - max_backoff: ms       │           │ - reset_at: Instant     │      │
│  └─────────────────────────┘           └─────────────────────────┘      │
│                                                                          │
│  ┌─────────────────────────┐           ┌─────────────────────────┐      │
│  │         Config          │           │     OutputRenderer      │      │
│  ├─────────────────────────┤           ├─────────────────────────┤      │
│  │ + default_profile       │           │ - format: OutputFormat  │      │
│  │ + profiles: IndexMap    │           ├─────────────────────────┤      │
│  ├─────────────────────────┤           │ + render<T>(data)       │      │
│  │ + load() -> Config      │           └─────────────────────────┘      │
│  │ + save()                │                                            │
│  └────────────┬────────────┘                                            │
│               │ contains                                                 │
│               ▼                                                          │
│  ┌─────────────────────────┐                                            │
│  │        Profile          │                                            │
│  ├─────────────────────────┤                                            │
│  │ - base_url: String      │                                            │
│  │ - email: String         │                                            │
│  │ - workspace: Option     │                                            │
│  └─────────────────────────┘                                            │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### Data Flow Sequence

```
┌──────┐     ┌──────────┐     ┌──────────┐     ┌───────────┐     ┌──────────┐
│ User │     │ cli::main│     │commands/*│     │ ApiClient │     │ Atlassian│
└──┬───┘     └────┬─────┘     └────┬─────┘     └─────┬─────┘     └────┬─────┘
   │              │                │                 │                │
   │  atlassian-cli jira issue search               │                │
   │──────────────▶                │                 │                │
   │              │                │                 │                │
   │              │ Route to       │                 │                │
   │              │ JiraCommands   │                 │                │
   │              │───────────────▶│                 │                │
   │              │                │                 │                │
   │              │                │ get("/search")  │                │
   │              │                │────────────────▶│                │
   │              │                │                 │                │
   │              │                │                 │ Check rate     │
   │              │                │                 │ limit          │
   │              │                │                 │────┐           │
   │              │                │                 │    │           │
   │              │                │                 │◀───┘           │
   │              │                │                 │                │
   │              │                │                 │ HTTPS GET      │
   │              │                │                 │───────────────▶│
   │              │                │                 │                │
   │              │                │                 │   200 OK +     │
   │              │                │                 │   JSON         │
   │              │                │                 │◀───────────────│
   │              │                │                 │                │
   │              │                │  Deserialized   │                │
   │              │                │◀────────────────│                │
   │              │                │                 │                │
   │              │  Render output │                 │                │
   │              │◀───────────────│                 │                │
   │              │                │                 │                │
   │ Table/JSON/YAML/CSV          │                 │                │
   │◀─────────────│                │                 │                │
   │              │                │                 │                │

Error Handling:
   │              │                │                 │                │
   │              │                │                 │ 429 Rate Limit │
   │              │                │                 │◀───────────────│
   │              │                │                 │                │
   │              │                │                 │ Exponential    │
   │              │                │                 │ backoff        │
   │              │                │                 │────┐           │
   │              │                │                 │    │ wait      │
   │              │                │                 │◀───┘           │
   │              │                │                 │                │
   │              │                │                 │ Retry request  │
   │              │                │                 │───────────────▶│
```

---

## Data Flows

### Authentication Flow

```
┌─────────────┐
│ CLI Command │
└──────┬──────┘
       │
       ▼
┌──────────────────────┐     ┌──────────────────────┐
│ Profile specified?   │────▶│  Load from config    │
│                      │ Yes └──────────┬───────────┘
└──────────┬───────────┘               │
           │ No                        │
           ▼                           │
┌──────────────────────┐               │
│ Use default profile  │               │
└──────────┬───────────┘               │
           │                           │
           └─────────────┬─────────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │   Load credentials   │
              └──────────┬───────────┘
                         │
                         ▼
              ┌──────────────────────┐     ┌──────────────────────┐
              │   Env var set?       │────▶│ Use ATLASSIAN_API_   │
              │                      │ Yes │ TOKEN                │
              └──────────┬───────────┘     └──────────┬───────────┘
                         │ No                         │
                         ▼                            │
              ┌──────────────────────┐               │
              │ Encrypted file?      │               │
              └──────────┬───────────┘               │
                    Yes  │  No                       │
           ┌─────────────┴─────────────┐             │
           │                           │             │
           ▼                           ▼             │
┌──────────────────────┐  ┌──────────────────────┐  │
│ Decrypt              │  │ Read plaintext       │  │
│ credentials.enc      │  │ credentials          │  │
└──────────┬───────────┘  └──────────┬───────────┘  │
           │                         │              │
           └────────────┬────────────┴──────────────┘
                        │
                        ▼
              ┌──────────────────────┐
              │  Create ApiClient    │
              └──────────┬───────────┘
                         │
                         ▼
              ┌──────────────────────┐
              │ Execute API request  │
              └──────────────────────┘
```

### Bulk Operation Flow

```
┌───────────────────┐
│   Input Items     │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│   BulkExecutor    │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐     ┌───────────────────────────────────┐
│    Dry Run?       │────▶│ Log operations, skip API calls    │
│                   │ Yes └─────────────────┬─────────────────┘
└─────────┬─────────┘                       │
          │ No                              │
          ▼                                 │
┌───────────────────┐                       │
│ Acquire semaphore │◀──────────────────┐   │
└─────────┬─────────┘                   │   │
          │                             │   │
          ▼                             │   │
┌───────────────────┐                   │   │
│Execute operation  │                   │   │
└─────────┬─────────┘                   │   │
          │                             │   │
          ▼                             │   │
┌───────────────────┐                   │   │
│    Success?       │                   │   │
└─────────┬─────────┘                   │   │
     Yes  │  No                         │   │
   ┌──────┴──────┐                      │   │
   │             │                      │   │
   ▼             ▼                      │   │
┌────────┐  ┌────────────────┐          │   │
│Record  │  │  Fail fast?    │          │   │
│success │  └───────┬────────┘          │   │
└───┬────┘     Yes  │  No               │   │
    │       ┌───────┴───────┐           │   │
    │       │               │           │   │
    │       ▼               ▼           │   │
    │  ┌─────────┐   ┌────────────┐     │   │
    │  │ Abort   │   │Record error│     │   │
    │  │remaining│   │ continue   │     │   │
    │  └────┬────┘   └─────┬──────┘     │   │
    │       │              │            │   │
    │       │              └──────┬─────┘   │
    │       │                     │         │
    └───────┼─────────────────────┘         │
            │         │                     │
            │         ▼                     │
            │  ┌───────────────┐            │
            │  │ More items?   │            │
            │  └───────┬───────┘            │
            │     Yes  │  No                │
            │          │    │               │
            │          │    └───────┐       │
            │          │            │       │
            │          └────────────┼───────┘
            │                       │
            └──────────┬────────────┘
                       │
                       ▼
              ┌──────────────────┐
              │ Return BulkResult│
              └──────────────────┘
```

---

## Technology Stack

| Layer | Technology | Purpose |
|-------|------------|---------|
| CLI Framework | Clap 4.5 | Argument parsing with derive macros |
| Async Runtime | Tokio 1.40 | Non-blocking I/O |
| HTTP Client | Reqwest 0.12 | REST API communication |
| Serialization | Serde | JSON/YAML/XML conversion |
| Output | Tabled, Colored | Terminal tables & colors |
| Progress | Indicatif | Progress bars |
| Security | AES-GCM, Argon2 | Credential encryption |
| Error Handling | Anyhow, Thiserror | Error propagation |
| Logging | Tracing | Structured logging |

### Product-Specific Notes

| Product | Auth Method | Response Format | Notes |
|---------|-------------|-----------------|-------|
| Jira/Confluence/JSM | Basic (email + API token) | JSON | Atlassian Cloud shared auth |
| Bitbucket | Basic or Bearer | JSON | Separate token storage |
| OpsGenie | GenieKey header | JSON | Async ops (202 + requestId), EU endpoint support |
| Bamboo | Basic or PAT | JSON (XML default) | Server/DC product, requires Accept header |

---

## Security Architecture

```
┌────────────────────────────────────────────────────────────────────────────┐
│                           Security Layers                                  │
│                                                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                      HTTPS Enforcement                               │  │
│  │                 (localhost exception for testing)                    │  │
│  └────────────────────────────────┬────────────────────────────────────┘  │
│                                   │                                        │
│                                   ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                       SSRF Protection                                │  │
│  │                  (URL scheme/host validation)                        │  │
│  └────────────────────────────────┬────────────────────────────────────┘  │
│                                   │                                        │
│                                   ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                    Credential Encryption                             │  │
│  │               (AES-256-GCM with Argon2 key derivation)               │  │
│  └────────────────────────────────┬────────────────────────────────────┘  │
│                                   │                                        │
│                                   ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                     SecretString Wrapper                             │  │
│  │              (prevents accidental credential logging)                │  │
│  └────────────────────────────────┬────────────────────────────────────┘  │
│                                   │                                        │
│                                   ▼                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐  │
│  │                    File Permissions 0600                             │  │
│  │                (Unix: owner read/write only)                         │  │
│  └─────────────────────────────────────────────────────────────────────┘  │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────────────┐
│                            Validation                                      │
│                                                                            │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐        │
│  │  URL Scheme     │───▶│ Host Validation │───▶│ safe_join() for │        │
│  │  Check          │    │                 │    │ paths           │        │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘        │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

**Security Features:**
- **HTTPS-only** communication (localhost exception for testing)
- **SSRF protection** via URL validation
- **AES-256-GCM** encryption for stored credentials
- **Argon2** key derivation
- **SecretString** prevents accidental credential logging
- **Secure deletion** with zero-overwrite
