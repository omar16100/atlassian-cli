# Jira attachments (issue #93)

Status: implemented on `feat/jira-attachments`.

## Problem

`jira issue get` has exposed attachment metadata since PR #64, but the only handle
it gives is the `content` URL, which sits behind Atlassian auth. Retrieving a file
meant opening a browser or re-doing auth by hand with curl. Coding agents driving
the CLI could only ever see the text of an issue, never its screenshots or CSVs.

## Commands

```
atlassian-cli jira attachment list <ISSUE_KEY>
atlassian-cli jira attachment get <ATTACHMENT_ID>
atlassian-cli jira attachment download <ATTACHMENT_ID> [--output PATH|-] [--force]
atlassian-cli jira attachment download --issue <KEY> [--dir DIR] [--force]
atlassian-cli jira attachment upload <ISSUE_KEY> --file PATH [--file PATH...]
atlassian-cli jira attachment delete <ATTACHMENT_ID> --force
```

Endpoints used, all Jira Cloud v3:

| Command | Request |
| --- | --- |
| `list` | `GET /rest/api/3/issue/{key}?fields=attachment` |
| `get` | `GET /rest/api/3/attachment/{id}` |
| `download` | `GET /rest/api/3/attachment/content/{id}` |
| `upload` | `POST /rest/api/3/issue/{key}/attachments`, multipart, field name `file` |
| `delete` | `DELETE /rest/api/3/attachment/{id}` |

## Design notes

**The redirect.** `/rest/api/3/attachment/content/{id}` answers with a 302 to a
short-lived signed URL on Atlassian's media host. `ApiClient::get_bytes` handles
this with no change: reqwest follows the redirect under its default
`Policy::limited(10)` and strips `Authorization` on the cross-host hop, which is
correct because the redirect target carries its own token. The same-origin check
in `safe_join` sees only the initial URL, which is ours. `?redirect=false` was
rejected as an approach: it hands back a cross-host absolute URL that
`safe_join` refuses, which would have forced a hand-rolled request with no SSRF
check, no retry and no rate limiting.

**Filename safety.** Server-supplied filenames are reduced to exactly one path
segment by `safe_filename` before being joined to the cwd or `--dir`. It strips
both `/` and `\` components, control characters, the Windows-illegal set, and
trailing dots and spaces; falls back to `attachment-{id}` when nothing usable
remains; suffixes Windows reserved device names; and truncates to 255 bytes on a
char boundary. A proptest asserts the single-segment invariant for arbitrary
input.

**Logging moved to stderr.** `init_tracing` used `fmt()` with no `.with_writer`,
so tracing-subscriber defaulted to stdout. A rate-limit or retry warning would
have corrupted `download --output - | file -`. Logs now go to stderr, which is
what the neighbouring `.with_ansi(stderr().is_terminal())` already implied.

**No silent clobber.** Unlike `confluence attachment download`, which truncates
whatever is at the target path, this refuses to overwrite without `--force`.

**Bulk mode** reuses the list response, so it costs one request per file and no
per-file metadata call. Downloads run sequentially so the shared rate limiter
still applies. Colliding filenames are disambiguated by attachment id. Per-file
status is rendered as rows, and any failure produces a non-zero exit.

**Upload** goes through the raw reqwest client because `ApiClient` has no
multipart helper, so it gets no same-origin check, retry or rate limiting.
`validate_ref` on the issue key is the compensating control: the host cannot
change (the key sits mid-path), but `/`, `..`, `?`, `#` and percent escapes would
change which path the request hits.

## Behaviour changes

`jira issue get --format json` now includes `author` and `created` on each
attachment. Additive only, no key removed or renamed.

## Limitations

- The client-wide 30s timeout covers the whole request including body transfer,
  so a large attachment on a slow link will fail and be retried from scratch.
- Bodies are buffered whole in memory in both directions.
- Jira Cloud only. Data Center uses `/rest/api/2` and different auth.
- Out of scope on purpose: thumbnails, archive expand endpoints,
  `issue create --attach`, ADF-embedded media. Attachment pagination is not a
  gap: the v3 issue response inlines the entire `attachment` array.

## Tests

- Unit, in `crates/cli/src/commands/jira/attachments.rs`: `safe_filename` against
  traversal, control characters, reserved names and overlong input, plus a
  proptest; `resolve_download_path` containment; `resolve_single_target`;
  `unique_download_name`; `validate_ref`; `upload_part_name`; `de_id_to_string`.
- Integration, `crates/cli/tests/jira_integration.rs`: list and get; the 302 to a
  second `MockServer` on a different port; a credential-leak regression asserting
  zero auth-bearing requests reach the redirect target; 404 and 403 mapping;
  single and multi-file multipart upload; 204 delete.
- CLI surface, `crates/cli/tests/cli_integration.rs`: `--help` lists all five
  subcommands, and the four mutually exclusive argument combinations are
  rejected. Worth having because an invalid `ArgGroup` only panics at runtime.
