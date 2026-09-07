# CLI feedback remediation plan

Status: complete on `feat/cli-feedback-remediation`, pending review and release.
879 tests across 32 suites, clippy clean under `-D warnings`.

**All 17 findings are addressed or explicitly deferred.** Findings 4 and 12 need
only a release; nothing on this branch reaches a user until a tag exists.

Deferred, and not claimed as done:

| Finding | Shipped | Deferred |
| --- | --- | --- |
| 5 | `uuid` columns on `permission list` and `pr reviewers`, so a UUID is obtainable | the member/name resolver described under Phase 2 |
| 8 | both `whoami` commands | `pipeline_status`, `approve_pull_request` and `get_pr_diff` still bypass the renderer |
| 11 | the envelope carries `total`/`truncated`/`next` | the default flip and `--no-envelope`, plus the ~20 direct `render(&rows)` conversions |

Also not done, and listed here so the gap is visible rather than implied. This
list is kept current; an earlier version of it went stale when the follow-up
branch converted more than it claimed.

- **Pagination.** Done for every Bitbucket list and for `jira project list`.
  **Not** done for the rest of the Jira tree (webhooks, automation, audit, field
  and workflow lists), nor for JSM or Opsgenie.
- **Path safety.** Every DELETE site in the Bitbucket tree is guarded, and
  `crates/api` refuses a restructuring path centrally. Roughly 89 raw
  `format!` interpolations remain across Jira, JSM and Opsgenie; the central
  guard catches `.`/`..`, backslash, control characters and `#`, but a `?` in an
  interpolated value cannot be caught centrally and needs per-site encoding.
- **Confirmation.** `bb bulk`, `bb branch delete` and `bb repo delete` are
  gated. Roughly ten deletes across Jira, JSM, Opsgenie and Bamboo have no
  confirmation at all; several others use a `--force` flag rather than a typed
  name. Which pattern those should adopt is a product decision, not a mechanical
  sweep.
- **`archive-repos`** still carries a subcommand name that describes something
  it does not do. Its help text now says so; renaming it is a breaking change
  held for the next major.

**This branch should ship as 0.9.0, not 0.8.x** (`Cargo.toml` still says
`0.8.0`; the bump is part of the release step, not of this work). It carries deliberate breaking changes:
`bb bulk delete-branches` and `archive-repos` list instead of acting, `bb branch
delete` requires the branch name typed, `--dry-run` conflicts with `--execute`,
and the envelope's `truncated` is now tri-state. The original sequencing in this
document binned several of those as patch-safe; that was wrong.

## Context

A user drove `atlassian-cli` through a full day of real Jira and Bitbucket work
and filed 14 findings. Every finding was checked against HEAD before being
accepted, and the plan was then reviewed adversarially, which corrected several
of the conclusions below.

### Three versions are in play

Conflating them mis-triages the report.

| Version | What it is |
| --- | --- |
| `0.2.8` | what the reporter ran (`/opt/homebrew/Cellar/atlassian-cli/0.2.8`) |
| `0.7.2` | the newest tag and release (2026-08-22), and what the tap formula points at |
| `0.8.0` | `Cargo.toml` on `main`, **unreleased** — no `v0.8.0` tag exists |

The tap formula is **not** stale: `atlassian-cli.rb:4` pins `0.7.2`, which is
current. The reporter's *installation* is stale.

- Fixed on or before v0.7.2 → already shipped, the reporter only needed
  `brew upgrade`. Finding 4 only.
- Fixed on `main` after v0.7.2 → written but unreleased, available to nobody.
  Finding 12's `--fields` is commit `2c830f4`, **2026-08-28**, six days after the
  v0.7.2 release.

`main` sits **four user-facing commits** ahead of the newest tag, two features
and two fixes, none of them released:

| Commit | Change |
| --- | --- |
| `fe81e6c` | feat(config): XDG config paths, and a movable config directory (#130) |
| `f7509c3` | fix(auth): dispatch user-info endpoint on the profile's product (#131) |
| `2c830f4` | feat(jira): `--fields` on issue get and issue search (#134) |
| `d2599aa` | fix(config): a `/wiki` base URL no longer doubles the segment (#135) |

**Cutting a release is therefore the first deliverable, not the last** — nothing
in this plan reaches a user until a tag exists, and neither does any of the
above. Note that `fe81e6c` relocates the config directory, so 0.8.0 carries a
migration users should be told about in the release notes regardless of this
plan.

### Triage

| # | Finding | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | `bb pipeline steps` truncates at 10 | Real, on **three** paths | `pipelines.rs:503` (`fetch_steps`), `:909` (`get_pipeline_logs`), `:1557` (`pipeline_has_failed_steps`) — each a single GET, no `pagelen`, no `next`; Bitbucket defaults to 10 |
| 2 | `jira issue search` caps at 100 | Real, on **two** paths | `jira/issues.rs:129-134` parses `isLast`/`nextPageToken` then marks both `#[allow(dead_code)]`; `jira/field_selection.rs:447-464` (the `--fields` path) never parses them at all |
| 3 | `bb permission list` 404s | Real | `bitbucket/permissions.rs:42,100,130` — all three use `/permissions` |
| 4 | `bb pr reviewers --add` 404s | Fixed, released, **unverified** | `bitbucket/pullrequests.rs:789-846` does the GET-then-PUT with a merged `reviewers` array; shipped at or before v0.7.2. The PUT body is only `{title, reviewers}` (`:822-828`) and has never been checked against live Bitbucket |
| 5 | `--reviewers` needs UUIDs, nothing resolves them | Real | `pullrequests.rs:398`; no user-search command exists in the repo |
| 6 | API-created PRs get no default reviewers | Real, and correct by Atlassian's design | no `/default-reviewers` request exists; Atlassian confirm default reviewers are suggestions applied by the UI, never by the API |
| 7 | No OAuth scope preflight | Real | no scope handling in `crates/auth` or `crates/config` |
| 8 | `-f json` ignored by `whoami` | Real, and **wider than reported** | `auth.rs:667`, `bitbucket/workspaces.rs:344`, plus `pipelines.rs:1120,1154`, `pullrequests.rs:597`, `pullrequests.rs:872-889` |
| 9 | `auth whoami` rejects `--bitbucket` | Real, **fix is not the flag** | `auth.rs:147` `WhoamiArgs` lacks it, but `whoami()` (`:631-653`) has no Bitbucket code path at all and hard-requires `base_url`, email and a Jira token |
| 10 | `logs_url` makes the steps table unusable | Real, **no public mechanism** | `pipelines.rs:155`; the column-taking renderer helpers are private and the one public entry shares its column list with CSV |
| 11 | Inconsistent top-level JSON shape | Real, deliberate | `render_list_or_empty` vs `render` in `crates/output/src/lib.rs` |
| 12 | `jira issue get` drops parent, labels, dates | Real; partly mitigated, unreleased | `--fields` (`field_selection.rs`) covers it on demand, but `view_issue` still renders a fixed nine-field subset (`issues.rs:258-270`), and `2c830f4` post-dates v0.7.2 |
| 13 | `bb pr get` hides reviewers and participants | Partly | `bb pr reviewers` exists but shows no UUID; `pr get` still emits `approvals` as a count string (`pullrequests.rs:341-362`) |
| 14 | No raw API passthrough outside Jira | Real | `commands/api.rs` is product-agnostic, wired only at `jira/mod.rs:95,1440` |
| 15 | **`bb bulk` truncates silently** (not reported) | Real | `bitbucket/bulk.rs:43,121` single GET at `pagelen=100`, no `next` |
| 16 | **`bb pr comments` truncates silently** (not reported) | Real | `pullrequests.rs:643` — no `pagelen`, no `next`; Bitbucket default is 20 |
| 17 | **`bb bulk delete-branches` deletes unmerged branches** (not reported) | Real, **shipped, destructive** | `bitbucket/bulk.rs:135-176` — see below |

### Finding 17 is the priority, ahead of everything the user reported

`delete_merged_branches` (`bulk.rs:114-189`) is exposed as
`bb bulk delete-branches`, whose help text reads "Delete merged branches"
(`bitbucket/mod.rs:939`). **It never checks merge status.** The only filter is:

```rust
let protected = ["main", "master", "develop", "development"];
let is_protected = protected.contains(&branch.name.as_str())
    || exclude_patterns.iter().any(|p| branch.name.contains(p));
if !is_protected { /* DELETE */ }
```

Every branch that is not one of four hardcoded names, or matched by
`--exclude`, is deleted regardless of whether it was ever merged. Active feature
branches, release branches and long-running integration branches all qualify.
Confirmed present in `git show v0.7.2:crates/cli/src/commands/bitbucket/bulk.rs`,
so it is **live in the current release**, not a regression on `main`.

The `pagelen=100` bug in finding 15 is the only thing currently bounding the
blast radius. **Fixing pagination first — as the first draft of this plan
proposed — would remove that accidental cap and let the command delete every
branch in the repository.** Sequencing matters here more than anywhere else in
the plan.

**Resolution chosen: keep the behaviour, add rails.** Deriving a real merged set
(from `state=MERGED` pull requests, or an ancestry check per branch) was
considered and rejected, because it would silently narrow a command people may
already rely on. Instead:

- `delete_merged_branches` is renamed `delete_branches`, and the help text now
  states that merge status is not checked and that unmerged branches are
  deleted.
- **Listing is the default.** Deleting requires `--execute`.
- `--execute` requires typing the repository slug, or `--yes` to skip it.
  Without a terminal and without `--yes`, it refuses rather than hanging, so a
  cron job cannot silently delete.
- `--dry-run` is still accepted and now hidden. It is honoured as a veto
  (`execute && !dry_run`), so an old script that passed it can never start
  deleting.

The behaviour change is deliberate: an invocation that previously omitted
`--dry-run` used to delete and now lists instead.

Ordering, therefore:

1. Make the command safe. **Done** — see the status line above.
2. Only then paginate it.

`archive_stale_repos` (`bulk.rs:43`) is the same shape and must be audited for
the same class of defect before its pagination is touched.

Findings 1, 2, 15 and 16 are the worst *reported* class: the output looks
authoritative and is wrong. Finding 17 is worse than all of them, because the
output is not merely wrong, it is acted on destructively.

## Decisions

1. **Scope: all real findings**, including 15 and 16 discovered during review.
2. **List output gets an envelope** carrying the truncation signal. Chosen over
   stderr-only, which a `jq` consumer cannot see.
3. **Auto-paginate up to `--limit`.** Chosen over one-page-plus-envelope.
4. **Default reviewers stay opt-in** behind a flag.
5. **Scope preflight: REOPENED.** See 1d.

## Sequencing

Revised: the original plan gated the zero-risk, highest-value item behind the
riskiest one.

| Step | Contents | Risk |
| --- | --- | --- |
| **0. Finding 17** | make `bb bulk delete-branches` safe | fixes data loss |
| **1. Release 0.8.0** | tag `main` plus step 0 | none |
| **2. 0.8.x patch** | 1c passthrough, findings 3, 9, 13, 1d 403 enrichment | additive |
| **3. 0.8.x patch** | 1a pagination + truncation signal, findings 1, 2, 15, 16 | behavioural |
| **4. 0.9.0** | 1b envelope default, findings 5, 6, 8, 10 | breaking |

Step 0 precedes the release because shipping 0.8.0 with a known destructive
defect would be worse than the current situation, where at least the pagination
bug caps it.

Step 1 delivers `--fields` for the first time. It does **not** deliver
`jira api`, which shipped in v0.5.0 (`3b453da`), and it does not "close finding
4" — v0.7.2 already contains that fix, so the reporter only ever needed
`brew upgrade`.

Step 2 gives them `bb api`, which converts every remaining DTO gap into an
inconvenience, without waiting on the breaking release.

**The truncation signal moves from step 4 to step 3**, alongside the driver that
creates it. Shipping auto-pagination with a page budget but no way to report
budget exhaustion would leave `bulk.rs` silently incomplete on the very path
that deletes things. For `bulk.rs` specifically the driver must fetch to
completion or fail loudly; a silent partial list is not acceptable there at any
release stage.

**Findings 8 and 10 move to 0.9.0.** `pipeline_status` prints pretty JSON
unconditionally today (`pipelines.rs:1154-1155`, and a literal `{}` at `:1120`),
so the default output of `bb pipeline status` is already machine-parseable with
no `-f` flag. Routing it through `OutputRenderer` turns the default into a
table and breaks every script parsing it — the same class of break as the
envelope flip, and it cannot ride in a patch.

## Phase 1a: pagination in `crates/api`

`crates/api/src/pagination.rs` defines a `Paginator` trait and `collect_pages`
helper that **no production code calls** — but `crates/api/benches/api_benchmarks.rs:1,43,54,66,87`
constructs `PagedResponse`, so replacing it breaks the bench build. Either port
the bench or keep `PagedResponse` as a deprecated re-export.

It is also Jira-shaped for an endpoint that no longer exists
(`startAt`/`maxResults`/`isLast`), which is why nothing adopted it.

Meanwhile the behaviour it should provide is **reimplemented three times**:

- `build_request_path` (`pipelines.rs:362-378`) follows `next`, validating
  scheme and host before use.
- `resolve_pipeline_id` (`pipelines.rs:456-470`) follows `next` by
  `strip_prefix`, with `MAX_PAGES = 10`.
- `list_variables` and friends (`variables.rs:195-213, 404-420, 461-480`) follow
  `next` to completion, correctly.

The weak `strip_prefix` variant is **not a vulnerability**: a foreign absolute
URL survives it unchanged and is then rejected by `safe_join`
(`crates/api/src/lib.rs:379-392`), which enforces same-origin. The defence is at
the client layer, where it belongs. But three hand-rolled copies is the symptom
of the primitive living in the wrong place, and the strong host validation from
`build_request_path` must move into the shared driver.

### The result-key problem

A single `fetch_paged<T>` cannot deserialize `T` directly: `client.get::<T>`
(`lib.rs:434`) parses the whole body, and items live under a per-endpoint key —
`values` (`pipelines.rs:73`, `pullrequests.rs:18`, `permissions.rs:8`), `issues`
(`issues.rs:126-135`). A runtime cursor enum cannot select that key for a
compile-time `Vec<T>` without a `serde_json::Value` round-trip, which double-parses
and destroys error attribution.

Make the *wrapper* generic instead and delete the bespoke ones:

```rust
#[derive(Deserialize)]
pub struct BitbucketPage<T> {
    pub values: Vec<T>,
    #[serde(default)] pub next: Option<String>,
    #[serde(default)] pub size: Option<u64>,
}

#[derive(Deserialize)]
pub struct JiraPage<T> {
    pub issues: Vec<T>,
    #[serde(default, rename = "nextPageToken")] pub next_page_token: Option<String>,
    #[serde(default, rename = "isLast")] pub is_last: Option<bool>,
}

pub trait Page: DeserializeOwned {
    type Item;
    fn into_parts(self) -> (Vec<Self::Item>, Option<Continuation>, Option<u64>);
}

pub async fn fetch_paged<P: Page>(
    client: &ApiClient, path: &str, limit: Option<usize>, budget: usize,
) -> Result<(Vec<P::Item>, PageInfo)>
```

Two impls total, not one per call site. Item structs (`PipelineStep`, `Issue`)
are untouched. Every Bitbucket list uses `values`/`next` uniformly, so one impl
covers the whole product. Ownership works: each page is freshly deserialized, so
`into_parts(self)` consuming it is fine at every call site.

### Three pieces the sketch leaves undefined

Each is load-bearing, and getting any of them wrong produces a subtly broken
driver rather than a compile error.

1. **`Continuation` is an enum, and the driver must rebuild from the original
   path.** Bitbucket returns an absolute `next` URL; Jira returns an opaque
   `nextPageToken` that goes into a query parameter. Naively appending the token
   sends two `nextPageToken` parameters by page three. The driver therefore
   keeps the original path and re-derives the request each iteration. The
   parameter name is endpoint-specific, so `Continuation::Token` carries its own
   parameter name rather than `crates/api` hardcoding a Jira-search detail.
   The Bitbucket arm passes its URL straight to `safe_join`, which already
   enforces same-origin.
2. **Page size is owned by the driver, not the caller.** Today both Jira callers
   bake `maxResults=limit.min(1000)` into `path` (`issues.rs:137-142`,
   `field_selection.rs:453-458`) while `fetch_paged` would also take `limit` —
   double-encoding it. The callers stop embedding it. Conversely the steps
   endpoint sets no `pagelen` at all, so it would page at Bitbucket's default of
   10 and issue ten times the necessary requests; the driver sets a sensible
   page size per cursor kind.
3. **`budget` counts requests, not items**, and is separate from `limit`.
   `limit` is what the user asked for; `budget` is the guard against a typo'd
   query hammering a rate-limited API. Exhausting `budget` sets
   `truncated: true`. Exhausting it in `bulk.rs` is an error instead, per the
   sequencing note above.

### Call sites converted

Genuine truncations, in severity order:

- `bulk.rs:43,121` (finding 15) — **first**, because it feeds deletions.
- `pipelines.rs:503,909,1557` (finding 1) — all three. Converting only
  `fetch_steps` leaves `bb pipeline logs` ignoring steps past 10 and
  `pipeline_has_failed_steps` capable of missing a failure on page 2, which
  silently corrupts `--wait` exit status.
- `issues.rs:137` and `field_selection.rs:453` (finding 2) — both. `--fields` is
  the recommended workaround for finding 12, so a user escaping one defect
  currently lands in another.
- `pullrequests.rs:643` (finding 16), and `:275`.
- `permissions.rs` — the re-pathed `permissions-config` lists are paginated too.
- **Not converted:** `branches.rs:55`, `repos.rs:38`, `workspaces.rs:43,110`,
  `commits.rs:90`. These were listed as in scope and were not reached; they
  still cap at `pagelen=min(limit,100)` with no cursor follow, so `--limit 500`
  silently returns 100.

**Not converted:** `variables.rs` already paginates correctly. Touching it is
refactoring, not a fix, and is out of scope.

## Phase 1b: envelope

`OutputRenderer` already carries an `envelope` flag (`main.rs:51`,
`output/src/lib.rs:31-35`) emitting `{"data": [...], "count": N}`. Extend rather
than replace: keys stay `data`/`count`, with `total`, `truncated` and `next`
added. Renaming to `values` would break today's `--envelope` users for nothing.

The envelope becomes the **default for JSON and YAML**, `--no-envelope` opts
out. Gated to 0.9.0.

**`total` is usually `null`.** `/rest/api/3/search/jql` does not return a total;
obtaining one needs a separate approximate-count endpoint. Bitbucket omits `size`
on expensive queries. The field is populated only where the API supplies it, and
the documentation must say so rather than implying a count is always available.

### The paths that bypass `render_list`

Without these, "a list is always the envelope" is false in exactly the cases
that matter, re-creating finding 11 inside its own fix:

- `render_rows_ordered` (`output/src/lib.rs:158-180`) sends JSON and YAML through
  `_ => self.render(&value)` at `:177`, emitting a bare array. This is the
  `--fields` path — the one most exposed to truncation. **Gets the envelope.**
- Direct `render(&rows)` on list data at **20 sites across every product**, not
  the four the previous draft named. Jira: `issues.rs:700,855`,
  `projects.rs:275,459`, `fields_workflows.rs:135,243`, `automation.rs:53`,
  `audit.rs:90`, `webhooks.rs:51`, `field_selection.rs:498`. Confluence:
  `pages.rs:89,430,968`, `spaces.rs:79`, `search.rs:65`, `attachments.rs:52`.
  Opsgenie: `alerts.rs:68`. Bamboo: `projects.rs:45`, `artifacts.rs:51`. Auth:
  `auth.rs:605`. **All convert to `render_list`.** A grep-based sweep is part of
  the phase, not a hand-listed set, because a missed site silently keeps the old
  shape and re-creates finding 11 inside its own fix.
- `render_list_or_empty` short-circuits on empty for Table and Markdown only
  (`:140-143`), so JSON already falls through correctly. An empty result must
  still yield `{"data": [], "count": 0}`.
- The `api` passthrough is **exempt by design**, documented as intentional.
  Precisely: `format_body` (`commands/api.rs:225-243`) parses and pretty-prints
  JSON, so "bytes verbatim" is strictly true only for `--output` and the binary
  path; either way it must not be wrapped, because a passthrough that reshapes
  the server's response is not a passthrough.

`Csv` and `Quiet` are line-oriented, keep current behaviour including the
empty-list short-circuit at `:119`, and see no envelope — matching how the
existing flag already scopes it (`:93-110`).

## Phase 1c: product-agnostic `api` passthrough

`commands/api.rs` (595 lines) has no Jira coupling beyond doc comments; `run`
takes `&ApiClient`, and `BitbucketContext.client` (`bitbucket/utils.rs:7`) is
one. Wire `ApiArgs` under `bitbucket`, `confluence` and `jsm` as
`jira/mod.rs:95,1440` does.

The reporter's top request, additive, zero risk. **Ships in step 2, not gated
behind the breaking release.**

**One real asymmetry, which "wire it like Jira" would get wrong.**
`bitbucket/mod.rs:969-1017` requires a workspace — from `--workspace`, the git
remote, or the profile — before `BitbucketContext` is constructed at `:1026`.
Only `Whoami` escapes it, via an early return at `:961-963`. Wiring `Api` at the
bottom of the match the way `jira/mod.rs:1440` does would make
`bb api /2.0/user` fail with "Workspace required" for anyone outside a Bitbucket
checkout — on the one command whose entire purpose is to reach endpoints the
typed commands cannot. `bb api` needs the same early exit `Whoami` has.

Verified as fine: `ConfluenceContext` and `JsmContext` expose `client: ApiClient`
identically (`confluence/utils.rs:8-11`, `jsm/utils.rs:9-12`); `commands/api.rs`
has no product coupling; and both `safe_join` (`api/src/lib.rs:379-392`) and the
redirect origin policy (`:351-365`) behave correctly against
`BITBUCKET_API_URL` (`auth/src/lib.rs:14`), since the check is same-origin
relative to whatever base the profile built.

Two cosmetic consequences: the `--help` text "must resolve to the profile's own
site" (`commands/api.rs:48`) is false for Bitbucket and needs rewording, and
`jsm api` would duplicate `jira api` exactly — same client, same base URL — so
it is dropped rather than shipped as a confusing alias.

## Phase 1d: scope handling — DECISION REOPENED

The 403 body is confirmed:

```json
{"type":"error","error":{"message":"Your credentials lack one or more required privilege scopes.",
 "detail":{"granted":["repository:write","account"],"required":["pullrequest"]}}}
```

**The reactive half is sound and should be built regardless:** parse `detail`,
diff the arrays, name the missing scope, and say *create a replacement token*
(scopes are fixed at creation and cannot be widened). No cached state, no map,
accurate by construction.

**The cached preflight originally chosen does not survive review.** Six
objections, in order of severity:

1. **It fails closed and can wedge.** The self-heal only fires when a request is
   sent. A user who adds a scope server-side hits a stale cache, is refused
   locally, no request goes out, no 403 arrives, and nothing refreshes until
   they discover `auth scopes --refresh`. The status quo — server authoritative,
   403 with a hint — is strictly better in that direction.
2. **A wrong map entry refuses a command the token can run.** That false
   negative is worse than the mid-task 403 this was meant to remove.
3. **Two scope vocabularies are live.** Classic OAuth (`pullrequest`,
   `repository:write`) and scoped API tokens (`write:pipeline:bitbucket` — the
   reporter's form). The map needs both spellings per command.
4. **Env-var tokens bypass `auth login`** (`auth.rs:68-121`), so nothing is ever
   cached for them.
5. **One `scopes` field conflates products.** A profile can hold Jira and
   Bitbucket credentials at once.
6. **Classic Jira API tokens are unscoped**, so the Jira half of the map has
   nothing to check.

**Decided: drop the per-command block.** 1d is now two pieces, neither of which
caches a command-to-scope map:

1. **403 enrichment.** `ApiError::Forbidden` currently keeps the raw body
   (`crates/api/src/lib.rs:850-856`). Parse `error.detail`, diff `required`
   against `granted`, and render the missing scope plus the instruction to
   *create a replacement token* — scopes are fixed at creation and cannot be
   widened, so "add the scope" would send the user somewhere that cannot help.
   When `granted` already covers `required`, say so: the cause is then
   repository permissions or an IP allowlist, not scopes, and claiming otherwise
   sends the user down the wrong path.
2. **`auth scopes`**, a read-only listing that blocks nothing.

The server stays authoritative. Nothing can refuse a command the token could
actually have run, which was the failure mode that killed the cached design.

## Phase 2: endpoints and capability

- **Finding 3.** `permissions.rs` moves to `/2.0/repositories/{ws}/{repo}/permissions-config/users`
  and `.../permissions-config/groups` for list, `.../permissions-config/users/{id}`
  for grant and revoke. Both collections merge into the existing `entity_type`
  rows. Requires `repository:admin` (or `admin:repository:bitbucket` plus
  `write:permission:bitbucket` for API tokens) — worth stating in the error text.
- **Finding 5.** Resolve a UUID, account id, or display name against
  `/2.0/workspaces/{ws}/members`. Ambiguity is an error, not a guess, following
  the `--fields` precedent. **Email is not resolvable** — Bitbucket does not
  expose member emails to non-admins — so the error must say so rather than
  reporting "not found".
- **Finding 6.** `--default-reviewers` on `pr create` reads
  `/2.0/repositories/{ws}/{repo}/effective-default-reviewers`, not
  `/default-reviewers`. The effective endpoint merges repository- and
  project-level reviewers and tags each with `reviewer_type`; the plain one
  returns repository-level only and would silently omit project defaults.
  Note `{target_username}` path segments reject UUIDs (BCLOUD-20706), so
  membership checks use the `?q=uuid="..."` filter form.
- **Finding 13.** `pr get` gains reviewer and participant detail; `pr reviewers`
  gains a `uuid` column, so one command's output is valid input to another.

## Phase 3: output and flag correctness

- **Finding 8**, full inventory: both `whoami` implementations, plus
  `pipeline_status` (`pipelines.rs:1154`, and the literal `println!("{{}}")` at
  `:1120`), `approve_pull_request` (`pullrequests.rs:597`) and the `get_pr_diff`
  stub (`pullrequests.rs:872-889`). All route through `OutputRenderer`.
- **Finding 9.** Adding `--bitbucket` to `WhoamiArgs` is flag parity only and
  fixes nothing on its own: `whoami()` has no Bitbucket path. It needs a
  Bitbucket branch modelled on `test_auth` (`auth.rs:704-721`), which can then
  delegate to `bitbucket::workspaces::whoami`.
- **Finding 10.** The previous draft's mechanism was wrong twice.
  `render_table_with` (`output/src/lib.rs:187`), `render_csv_with` (`:208`) and
  `render_markdown_table_with` (`:292`) are all **private**, so a caller cannot
  reach them. The only public column-taking entry is `render_rows_ordered`
  (`:158`), and it applies one column list to Table, CSV **and** Markdown alike
  (`:167-170`), so routing through it would strip `logs_url` from CSV too,
  contradicting the intent. It also takes `&[Value]`, while steps are typed
  structs.

  What actually works: the caller inspects `renderer.format()` and passes a
  format-dependent column list, with JSON and YAML unaffected because they
  ignore columns entirely via the `_` arm at `:177`. `StepInfo` is serialized to
  `Value` first. This is a caller-side change plus a `Serialize` round-trip, not
  the free parameter the draft claimed.

## Phase 4: release

Tag 0.9.0 with the envelope default called out as breaking. The formula is
generated by `cargo-dist` (`dist-workspace.toml`) and follows the tag, so no
manual edit is needed.

## Verification

- Unit tests per phase, colocated as the codebase already does.
- New `crates/cli/tests/pagination_e2e.rs`: a mock server returning a multi-page
  response, asserting every page is followed and a truncated result is labelled.
  Must fail against pre-fix code. Covers all three `/steps/` sites, both Jira
  search paths, and `bulk.rs`.
- New e2e for `permissions-config`, asserting **zero hits** on the old
  `/permissions` path so a regression fails loudly.
- **A test for the envelope default flip** and **a test for `whoami -f json`** —
  the two behaviour changes most likely to regress, and both absent from the
  first draft of this plan.
- **A live `bb pr reviewers --add`** against a real repository. Finding 4 is
  currently closed on code reading alone, which contradicts the standing rule
  that configured is not verified.
- Live verification against a real Bitbucket workspace and Jira site before any
  phase is called done.
- `docs/c4model.md` updated in the same PR as 1a, which changes the `crates/api`
  component boundary.

## Deviations

- The first draft claimed the Homebrew formula was stale. It is not; the
  reporter's installation was. Corrected above.
- The first draft claimed nothing calls `pagination.rs`. The benches do.
- The first draft listed `variables.rs` as a truncation site. It paginates
  correctly.
- The first draft dated `--fields` to 26-08-2026. It is 2026-08-28.
- The first draft proposed `fetch_paged<T>` with a runtime cursor enum, which
  does not type-check against the real call sites. Replaced with the `Page`
  trait.
- Findings 15 and 16 were discovered during the first review round and added.

Second review round:

- **Finding 17 discovered**, and it reorders the whole plan. Fixing finding 15's
  pagination first, as the previous revision proposed, would have removed the
  only bound on a destructive bug.
- The 1c "wire it like Jira" claim missed Bitbucket's workspace gate
  (`bitbucket/mod.rs:969-1017`), which would have made `bb api` unusable outside
  a git checkout.
- The envelope bypass list named 4 sites. There are 20, across all four
  products.
- Finding 8's `pipeline_status` fix is breaking, not additive: default output is
  already JSON (`pipelines.rs:1154-1155`). Moved to 0.9.0 with finding 10.
- Finding 10's mechanism did not exist: the column-taking helpers are private,
  and the public entry shares columns between Table, CSV and Markdown.
- Claimed step 0 delivers `jira api` "for the first time". It shipped in v0.5.0
  (`3b453da`). Only `--fields` is unreleased.
- Claimed `main` was two feature merges ahead of the tag. It is four commits.
- The `Page` sketch left `Continuation`, page-size ownership and `budget`
  semantics undefined; all three are now specified.
- Minor: `logs_url` is at `pipelines.rs:155`; `view_issue` renders nine fields,
  not eight; `commands/api.rs:225-243` is `format_body`, which pretty-prints.
