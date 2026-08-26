# Documentation index

Start here. Every document in `docs/` is listed below; add a row when you add a
file.

## Conventions

**Naming.** Dated documents use `DDMMYYYY_topic.md` and describe a specific piece
of work: what changed, why, and what it means for users. Evergreen documents use
`topic.md` and are kept current rather than superseded.

**Categories and required sections.**

| Category | Naming | Required sections |
| --- | --- | --- |
| Architecture | `topic.md` | Context, containers, components, data flows |
| Feature / change | `DDMMYYYY_topic.md` | Status, problem, design notes, limitations, tests |
| Plan | `DDMMYYYY_topic_plan.md` | Context, approach, verification; updated as work progresses |
| Roadmap / status | `topic.md` | Kept current, no date prefix |

**Status lines.** A dated feature document opens with a one-line status
(`shipped in vX.Y.Z (PR #N)`, `in progress on <branch>`, `abandoned, see ...`).
Update it when the work lands; a stale branch name in a merged document is worse
than no status at all.

## Architecture

| Document | What it covers |
| --- | --- |
| [c4model.md](c4model.md) | **Source of truth for architecture.** Containers, components, services, dependencies and data flows. Read before any architectural change; update in the same PR. |

## Direction

| Document | What it covers |
| --- | --- |
| [vision.md](vision.md) | Opportunity, positioning, differentiation against ACLI and Appfire |
| [plan.md](plan.md) | Phase completion plan per product area |
| [status.md](status.md) | Per-phase implementation, test and documentation status |
| [todo.md](todo.md) | Roadmap checklist across all phases |

Note: the repository-root `todo.md` is a different file. It is the running log of
changes made, newest last; `docs/todo.md` is the forward-looking roadmap.

## Features and changes

| Document | Date | What it covers |
| --- | --- | --- |
| [26082026_auth_product_dispatch.md](26082026_auth_product_dispatch.md) | 2026-08-26 | `auth whoami`/`test`/`status` dispatch on the profile's product, the `--space` filter fix, and 401s that keep the server's reason |
| [16082026_bb_pr_reviewer_status.md](16082026_bb_pr_reviewer_status.md) | 2026-08-16 | `bb pr reviewers` listing with approval status, `--all`, and the `--add` endpoint fix |
| [23082026_xdg_config_paths.md](23082026_xdg_config_paths.md) | 2026-08-23 | XDG config directory resolution, `--config-dir`, legacy migration and 0600/0700 permissions |
| [22082026_confluence_inline_comments.md](22082026_confluence_inline_comments.md) | 2026-08-22 | Confluence inline comments, thread replies, cursor pagination and storage-format escaping |
| [20082026_bitbucket_pr_inline_comments.md](20082026_bitbucket_pr_inline_comments.md) | 2026-08-20 | Inline pull request comments: `bb pr comment --path/--line/--side`, and the `location` column on `pr comments` |
| [10082026_raw_api_passthrough.md](10082026_raw_api_passthrough.md) | 2026-08-10 | `jira api` raw authenticated REST passthrough, `ApiClient::request_raw`, origin and redirect safety |
| [10082026_jira_attachments.md](10082026_jira_attachments.md) | 2026-08-10 | `jira attachment` group: list, get, download (single, stdout, bulk), upload, delete |
| [02042026_jira_search_migration.md](02042026_jira_search_migration.md) | 2026-04-02 | Migration to `/rest/api/3/search/jql` after the old endpoint was removed |
| [14042026_jira_custom_fields.md](14042026_jira_custom_fields.md) | 2026-04-14 | `--field KEY=JSON_VALUE` on issue create/update and bulk |
| [14032026_pipeline_ux_fixes_2.md](14032026_pipeline_ux_fixes_2.md) | 2026-03-14 | Second round of Bitbucket pipeline UX fixes |
| [11032026_pipeline_ux_fixes.md](11032026_pipeline_ux_fixes.md) | 2026-03-11 | Git remote detection, pipeline commands, output formatting, error messages |
| [20022026_bitbucket_bearer_auth.md](20022026_bitbucket_bearer_auth.md) | 2026-02-20 | Bitbucket bearer token support alongside app passwords |
| [14012026.md](14012026.md) | 2026-01-14 | JSM, Opsgenie and Bamboo implementation |
| [26122025.md](26122025.md) | 2025-12-26 | Confluence draft publishing fix |

## Examples

| Path | What it covers |
| --- | --- |
| [examples/](examples) | Runnable scripts per product: Confluence doc pipeline, space backup, bulk cleanup; Jira bulk transition, sprint report, project cleanup; Bitbucket PR automation, repo audit, branch cleanup |

## User-facing documentation

Command reference and how-to guides live on the project site
(atlassiancli.com), which is maintained in a separate repository. `README.md` in
this repository carries the installation instructions and a command overview.
