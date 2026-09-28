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
                 │ config directory │                    │ config directory │
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
| `config` | Library | ~700 | YAML configuration, and resolving where the config directory lives |
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

**Bitbucket pipelines** (`commands/bitbucket/pipelines/`), split by command
(the single file had passed 2000 lines):

| Module | Owns |
| --- | --- |
| `model` | API response types, `PipelineRow`/`PipelineView`/`StepInfo` |
| `state` | status derivation, icons, exit codes, step outcome, trigger normalisation |
| `rows` | building rows per output format |
| `list` | `list`, `get`, `latest`, build-number resolution |
| `steps`, `logs` | step listing, the failed-step check, log retrieval |
| `trigger`, `status`, `watch` | the remaining commands |

Status derivation (`state::get_pipeline_status`): `state.result.name` once
finished; while `IN_PROGRESS`, `state.stage.name` when it is not `RUNNING`
(Bitbucket reports a build waiting on a manual step as stage `PAUSED`);
otherwise `state.name`. A step's outcome is likewise `result` before `name`,
since a finished step's `name` is always `COMPLETED`. Exit codes for `status`
and `watch`: 0 successful, 1 failed or unrecognised, 2 in progress, pending or
timed out, 3 paused on a manual step; `watch` and `status --wait` stop at a
pause.

Rows (`rows.rs`) are built per format: status icons, and colour in a table,
only where `OutputFormat::is_human()` (table, markdown); the machine formats
get the bare status, numbers as numbers, and `null` for anything the API did
not report.

**Positional repository** (`commands/bitbucket/positional.rs`). Pipeline
commands take `[REPO]` ahead of their identifiers. `split_leading_repo` decides
by count (a value beyond the identifier slots no flag filled is the
repository), then by shape (a first value that is neither digits nor a UUID is
the repository). `choose_repo` resolves argument, then `--repo`, then the git
remote; an argument that disagrees with `--repo` is an error. The argument's
clap id is not `repo`, so it does not shadow the global `--repo`.

**Pull request edits** (`commands/bitbucket/pullrequests.rs`). Bitbucket edits
a pull request with a PUT of the pull request, so `pr update` and
`pr reviewers --add` GET it first and PUT title, description and reviewers
together (`build_pr_put_body`), each from the change or the current value.

#### Configuration directory

`config.yaml`, `credentials` and `credentials.enc` all live in one directory,
resolved by `crates/config/src/paths.rs`:

1. `$ATLASSIAN_CLI_CONFIG_DIR` or `--config-dir`
2. `$XDG_CONFIG_HOME/atlassian-cli`
3. `~/.config/atlassian-cli` (`%LOCALAPPDATA%\atlassian-cli` on Windows)
4. a legacy `~/.atlassian-cli` or `~/.atlcli` that still holds those files

`auth` does not resolve this itself: `CredentialStore` is constructed with the
directory by `main`, which is what keeps the directory movable and keeps `auth`
free of a dependency on `config`. `--config` overrides the config *file* only,
so a shared or read-only config does not imply writing credentials beside it.

A legacy directory is migrated on first use: files are staged and promoted with
one atomic rename, then the original is renamed to `.migrated`.

**Shared (`commands/`):**
- `api.rs` - Raw authenticated REST passthrough (`jira api`), product-agnostic;
  built on `ApiClient::request_raw`, which returns status/headers/body with no
  status-to-error mapping
- `common.rs` - `render_success`/`MutationResult`, the typed confirmation for
  destructive commands, and `ExportFormat`/`resolve_export_format`: the file
  format of `jira bulk export`, `jira audit export` and `confluence bulk export`
  (`--export-format`, else a global `--format` of json or csv, else JSON)

**Global arguments.** `main.rs` declares `--profile`, `--config`, `--config-dir`,
`--format` and `--envelope` as clap globals, and `BitbucketArgs` adds
`--workspace` and `--repo`. clap keys arguments by id and skips propagating a
global into a subcommand that already has an argument with that id, so a
subcommand that redeclares a global's id with another type makes clap panic when
the global is read (the export commands' `format: String` did this until the fix
in #146). The unit tests in `main.rs` fail on any such redefinition and run
clap's `debug_assert` over the whole tree, which catches reused long or short
flag names. Redeclaring a global's id with the same type is allowed: the value
flows up to the global (`auth logout --profile`, positional `repo` under `bb`).

**Jira (`commands/jira/`):**
- `issues.rs` - CRUD, search, transitions, assignments
- `attachments.rs` - Attachment list/get/download/upload/delete, plus the shared
  `JiraAttachment` model used by `issues.rs` for `issue get`
- `projects.rs` - Project management, roles
- `fields_workflows.rs` - Custom fields, workflow transitions
- `field_selection.rs` - `--fields` on `issue get`/`issue search`: display name to
  id resolution via `/rest/api/3/field`, ordered projection, and value flattening
  for the tabular formats only
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
│  │   RetryConfig    │  │   RateLimiter    │  │   fetch_paged    │        │
│  │                  │  │                  │  │                  │        │
│  │ - Exponential    │  │ - x-ratelimit    │  │ - Follows the    │        │
│  │   backoff        │  │   header tracking│  │   server cursor  │        │
│  │ - Max 3 attempts │  │ - Auto-throttle  │  │ - Reports        │        │
│  │ - 500ms-30s      │  │ - 80% warning    │  │   truncation     │        │
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

// Methods: get<T>, post<T>, put<T>, delete<T>, delete_no_content, get_text,
//          get_bytes, response_header, request_raw
// Features: HTTPS enforcement, SSRF protection, automatic retry
```

**Response handling** (`crates/api/src/response.rs`). Every request path sends
its response through the same two functions: `log_response` (method, URL,
status, elapsed time, at debug) and, for a non-2xx, `error_for_status`, which
reads the body once, logs it (credential-scrubbed by `scrub_credentials`, cut
to 2 KB) and maps the status to an `ApiError`. Before this each path carried
its own copy of the mapping and none logged a status or an error body. URLs are
logged through `redact_url`, which blanks credential-like query values. Request
bodies and headers are never logged; only whether a body was sent, or its size
for `request_raw`.

Debug data flow: **`--debug` (global) → `main::tracing_directives` →
`EnvFilter` `info,atlassian_cli=debug` (prefix match, so every workspace crate)
→ `ApiClient` request/response events → stderr.** `RUST_LOG` directives are
appended as refinements; stdout carries only command output.

**Pagination** (`crates/api/src/pagination.rs`).

Until v0.9.0 this module exposed a `Paginator` trait and `PagedResponse` that
no production code called, shaped for a Jira endpoint that has since been
removed. The behaviour it was meant to provide had instead been hand-rolled
three times inside the Bitbucket command modules, in three forms with three
different levels of care about the URL the server handed back. List commands
therefore issued a single request and rendered whatever came back, so a
Bitbucket collection returned 10 of 11 items and a Jira search returned the
server's cap of 100, in both cases with nothing in the output to distinguish
that from a complete answer.

```rust
pub trait Page: DeserializeOwned {
    type Item;
    fn into_parts(self) -> (Vec<Self::Item>, Option<Continuation>, Option<u64>);
}

pub async fn fetch_paged<P: Page>(
    client: &ApiClient, path: &str, limits: PageLimits,
) -> Result<(Vec<P::Item>, PageInfo)>
```

The *wrapper* is generic, not the item: `BitbucketPage<T>` keys on `values`,
`JiraPage<T>` on `issues`. A single `fetch_paged<T>` cannot work, because
`ApiClient::get::<T>` deserializes the whole body and nothing tells it which key
holds the items; passing the key as a string would mean a `serde_json::Value`
round-trip that costs a second parse and discards type errors.

`Continuation` is an enum because the products differ substantively: Bitbucket
returns an absolute URL (resolved through `safe_join`, so a cursor pointing at
another origin is refused), Jira an opaque token that the driver places back on
the *original* path, replacing any previous one; appending would put two
`nextPageToken` values on the third page.

`PageInfo.truncated` is the contract with `crates/output`: `ListMeta` carries it
into the JSON/YAML envelope, and it is `Option<bool>` so that a command which
never paginated reports *nothing* rather than asserting completeness it has not
established.

Data flow: **command → fetch_paged → ApiClient::get (per page) → PageInfo →
ListMeta → OutputRenderer envelope.**

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
│  │  in the config directory  │                                           │
│  │  (0600 permissions)       │                                           │
│  └───────────────────────────┘                                           │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Security Features

- **AES-256-GCM** encryption for tokens at rest
- **Argon2id** key derivation from the machine id and OS user name (salt: the
  machine id bytes). Parameters are pinned in `crates/auth/src/encryption.rs`
  (v0x13, m=19456 KiB, t=2, p=1, 32-byte key) and held by a known-answer test,
  because every stored `credentials.enc` depends on them
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
│  │  in the config directory  │                                            │
│  └───────────────────────────┘                                            │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

#### Configuration Structure

```yaml
# ~/.config/atlassian-cli/config.yaml
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
│  │  (default)  │ │--format │ │--format │ │--format │ │--format │         │
│  │  tabled     │ │  json   │ │  yaml   │ │  csv    │ │  quiet  │         │
│  └─────────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘         │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

Table mode (`OutputRenderer::table_output`): a list of objects is a table with
a column per key; a single object is a `field | value` table in the order its
fields are declared (the value is serialised and read back into an `IndexMap`,
because `serde_json::Value` sorts keys), with lists of objects inside it
rendered below as titled tables and values wrapped at 100 columns. Anything
else falls back to JSON. `render_document` keeps JSON in table mode for the
commands whose output is a raw API document (`jira workflow export`,
`confluence folder get`). `OutputFormat::is_human()` names the table and
markdown formats, where decoration belongs.

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
| CLI Framework | Clap 4.6 | Argument parsing with derive macros |
| Async Runtime | Tokio 1.51 | Non-blocking I/O |
| HTTP Client | Reqwest 0.13 | REST API communication |
| Serialization | Serde | JSON/YAML/XML conversion |
| Output | Tabled, Colored | Terminal tables & colors |
| Progress | Indicatif | Progress bars |
| Security | AES-GCM 0.11, Argon2 0.6 | Credential encryption |
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
