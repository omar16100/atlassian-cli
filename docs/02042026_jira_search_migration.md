# Jira Search API Migration (CHANGE-2046)

## What Changed

Atlassian removed the legacy Jira search endpoints:

| Old (removed, returns 410) | New |
|---|---|
| `GET /rest/api/3/search?jql=...` | `GET /rest/api/3/search/jql?jql=...` |
| `POST /rest/api/3/search` with JSON body | `POST /rest/api/3/search/jql` with JSON body |
| `GET /rest/api/2/search?jql=...` | Also returns 410 |

The new endpoint at `/rest/api/3/search/jql` accepts the same request/response schema — only the URL path changed.

## What Was Fixed

### v0.3.1 (prior)
- `jira issue search` migrated to `/rest/api/3/search/jql` (GET)
- `auth test` already used `/rest/api/3/myself` (unaffected)

### v0.3.2 (this fix)
- **Bulk operations** (`bulk export`, `bulk transition`, `bulk label`, `bulk assign`) migrated from `POST /rest/api/3/search` to `POST /rest/api/3/search/jql`
- **HTTP 410 handling** added to API client — returns `EndpointGone` error with clear message instead of falling through to generic `ServerError`
- **Error suggestions** — `EndpointGone` errors now suggest updating atlassian-cli instead of misleading users about expired credentials

## Files Changed

- `crates/cli/src/commands/jira/bulk.rs` — migrated 2 search calls
- `crates/api/src/error.rs` — added `EndpointGone` variant
- `crates/api/src/lib.rs` — added `StatusCode::GONE` handling in all HTTP methods
- `crates/cli/tests/jira_integration.rs` — updated mock endpoint, added 410 test

## Reference

- Atlassian changelog: CHANGE-2046
- Canonical auth test endpoint: `GET /rest/api/3/myself`
