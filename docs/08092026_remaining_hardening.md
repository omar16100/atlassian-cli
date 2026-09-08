# Remaining hardening

Status: open. Nothing here blocks the 0.9.0 release; all of it was found while
remediating the 17 reported findings and is recorded so it does not have to be
rediscovered.

## Problem

The remediation work in
[07092026_cli_feedback_remediation_plan.md](07092026_cli_feedback_remediation_plan.md)
fixed everything the user reported. Along the way it surfaced four classes of
defect that were **outside** that report, each of which exists in more places
than the report touched. This document is that list, with counts taken from the
tree rather than from memory.

The counts below were produced on `fix/destructive-confirmation-consistency`
before it merged. They are approximate in the sense that a grep cannot tell a
dangerous interpolation from a safe one; they are exact as counts of the shape.

## 1. Path building by string interpolation

Every request path is assembled with `format!`, so safety depends on each call
site remembering to encode what it interpolates. Across seven review rounds that
discipline failed in a different way each time.

Two mitigations are in place:

- `crates/api/src/lib.rs::reject_restructuring_path` runs inside `safe_join`, so
  **every** request is checked. It refuses `.` and `..` components (matching
  what the URL parser does, including `%2e` spellings), backslashes, control
  characters and `#`.
- In the Bitbucket tree, `encode_path_segment` guards the interpolated values
  themselves. **No raw `{workspace}/{repo_slug}` prefix remains at any DELETE
  site there.**

What remains:

| Product | Path-building `format!` calls |
| --- | --- |
| Jira | 52 |
| JSM | 43 |
| Confluence | 36 |
| Bamboo | 31 |

None of those four products encodes its interpolated values. The central guard
covers most of the hazard, but **a `?` cannot be caught centrally**: a query is
legitimate, so `.../issue/{key}` with a key of `KEY-1?deleteSubtasks=true`
injects a real parameter, and the guard cannot distinguish that from a caller
that meant to pass a query.

**The durable fix is to stop formatting strings.** A segment-based path builder —
where each segment is a typed value that encodes itself — makes the class
impossible rather than catchable. That is a refactor of the command layer and
should be scoped deliberately, not attempted incrementally; incremental was
tried seven times and produced a new defect on six of them.

## 2. Destructive commands with no confirmation

`bb bulk`, `bb branch delete` and `bb repo delete` were brought onto a common
footing: listing is the default where it makes sense, `--execute` performs the
change, and a typed resource name is required unless `--yes` is passed. Nothing
else in the CLI follows it.

Ten delete operations take no confirmation and no `--force` flag at all:

| Command | Reversible? |
| --- | --- |
| `confluence bulk delete-pages` | **No** — bulk, and the least guarded thing in the CLI |
| `jira field delete` | **No** |
| `jira project component delete` | **No** |
| `jira project version delete` | **No** |
| `jira issue comment delete` | **No** |
| `jira issue link delete` | **No** |
| `jira role remove-actor` | Yes |
| `jira issue watcher remove` | Yes |
| `confluence page label remove` | Yes |
| `confluence page restriction remove` | Yes |

The rest use a `--force` flag, which is the fail-closed pattern and acceptable;
only the typed-name commands go further.

**This needs a product decision before a sweep.** Adding prompts to ten commands
across three products changes behaviour for anyone scripting them, and the
codebase already contains two defensible patterns (`--force`, and typed-name
confirmation). Picking one per command is a judgement about blast radius, not a
mechanical edit.

## 3. Pagination

Done: every Bitbucket list, and `jira project list`.

Not done: the rest of the Jira tree (`webhook list`, `automation list`, `audit`,
the field and workflow lists), and all of JSM and Opsgenie. Each is a single
request rendering whatever the first page held — the defect the original report
opened with, in the products the report did not happen to exercise.

`crates/api/src/pagination.rs` already provides both shapes these need:
`JiraPage` for token-paged endpoints and `JiraOffsetPage` for the classic
`startAt`/`maxResults` ones. The remaining work is call-site conversion, and the
`page_size` / `warn_if_truncated` helpers in `bitbucket/utils.rs` should move
somewhere shared when the first non-Bitbucket product needs them.

## 4. `-f json` ignored

Three commands still write results with `println!` and ignore `--format`:
`pipeline_status`, `approve_pull_request` and the `get_pr_diff` stub.

`pipeline_status` is the awkward one: its default output is *already*
machine-parseable JSON, so routing it through the renderer turns the default
into a table and breaks every script that parses it. That is a breaking change
and belongs with the other breaking changes, not in a patch.

## 5. Naming that still misdescribes behaviour

`bb bulk archive-repos` does not archive. Bitbucket Cloud has no repository
archive API; the command sets `has_issues: false` and `has_wiki: false`, which
hides any issues and wiki pages that exist. Its help text now says so plainly,
but the subcommand name still does not. Renaming it is a breaking change held
for the next major.

## Limitations of this list

- The counts are of *shape*, not of exploitability. A `format!` with an
  interpolated value is not automatically a defect; it is a site that has not
  been reasoned about.
- Opsgenie and Bamboo were never reviewed in this work at all. They appear here
  only where a mechanical count reached them.
- **Nothing in the remediation branches has been verified against a live
  Atlassian instance.** See the plan document; that gap applies to this list too,
  and is the reason none of these items should be fixed blind.

## Tests

None. This document records work not yet done. The mechanisms it refers to —
`reject_restructuring_path`, `encode_path_segment`, `fetch_paged`,
`confirm_destructive` — are covered by tests in their own right; see the plan
document and `crates/api/src/pagination.rs`.
