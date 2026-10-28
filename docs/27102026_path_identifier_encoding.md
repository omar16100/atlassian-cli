# Identifiers in write and delete request paths are percent-encoded

Status: included in v0.13.0 (PR #178, integration PR #179).

## Problem

Every command builds its request path with `format!`. For a long time that
meant each call site had to remember to encode what it interpolated, and most
did not. A caller-provided identifier that is not percent-encoded can change the
request target:

- a `?` moves the rest of the path into the query string, so
  `.../repositories/w/r?x/pullrequests/7/comments/9/resolve` is sent as a
  request for `.../repositories/w/r` with a query;
- a `#` starts a fragment that is never transmitted, which truncates the path;
- a `/` adds path segments, so one identifier can address a nested resource.

The central guard in `crates/api` (`reject_restructuring_path`) refuses `#`, dot
segments, backslashes and control characters, but it cannot refuse a `?`: a
query is legitimate, and the guard only sees the finished string. Earlier changes
encoded the identifiers at the Bitbucket delete sites; the Jira, JSM,
Confluence, Bamboo and Opsgenie trees, and the Bitbucket sites that create or
update, were left as they were.

Two further gaps belonged to the same class:

- the Confluence and Jira attachment uploads build their URL by hand and send it
  through the raw `reqwest` client, so the central guard never runs for them;
- `bb` took the workspace and repository slug from a git remote URL without any
  character check.

`opsgenie heartbeat get <name>` was also affected. A name of `a/ping` produced
`heartbeats/a/ping`, which is the endpoint that sends a ping.

## Design notes

### One shared helper

`atlassian_cli_api::path::encode_path_segment(&str) -> Result<String>` (new
module `crates/api/src/path.rs`, next to `query::encode_query_component`):

- everything outside the RFC 3986 unreserved set (letters, digits, `-`, `.`,
  `_`, `~`) is percent-encoded, so `?`, `#`, `%` and space cannot change the
  target, and an identifier that already looks encoded is sent literally;
- an empty value, a value containing `/`, and the dot segments `.` and `..` are
  rejected with `ApiError::InvalidIdentifier`, a new error variant. The error
  message names the rejected value.

A `/` is rejected rather than encoded as `%2F`. None of the identifiers here is
legitimately more than one segment, and servers and proxies do not agree on how
to treat an encoded slash, so refusing is the predictable choice.

`bitbucket/utils.rs::encode_path_segment` was the model for the helper and is now
a re-export of it, so the existing Bitbucket call sites and tests are unchanged.
The output for every value the old function accepted is identical.

`encode_ref_path` (branch and tag names, where `/` is a real separator) stays as
it was.

### Call sites

Every POST, PUT, PATCH and DELETE that interpolates a non-numeric identifier
into the path now goes through the helper, in all products. That includes
`delete_no_content`, `delete_with_body` and the two multipart uploads.

| Product | Files |
| --- | --- |
| Bitbucket | `pullrequests.rs`, `branches.rs`, `repos.rs`, `workspaces.rs`, `webhooks.rs`, `variables.rs`, `bulk.rs`, `pipelines/trigger.rs` |
| Jira | `issues.rs`, `projects.rs`, `fields_workflows.rs`, `attachments.rs`, `bulk.rs` |
| Confluence | `pages.rs`, `attachments.rs`, `spaces.rs`, `folders.rs`, `bulk.rs` |
| JSM | `requests.rs`, `approvals.rs`, `feedback.rs`, `customers.rs` |
| Opsgenie | `alerts.rs`, `incidents.rs`, `schedules.rs`, `services.rs`, `escalations.rs`, `heartbeats.rs`, `teams.rs` |
| Bamboo | `builds.rs`, `plans.rs`, `branches.rs` |

Values typed as integers (pull request ids, webhook ids, Bamboo agent ids, JSM
service desk and organization ids, Jira automation rule ids) cannot contain a
delimiter and are not encoded.

Three details worth knowing:

- Where a command reads a resource and then writes it through the same path
  variable, the read uses the encoded path as well: `bb pr update`,
  `bb pr reviewers --add`, `jira issue comments update`, `confluence page
  update` and `publish`, `confluence blog update` and `publish`, and
  `confluence space update`. Otherwise the version number or content for the
  write could come from a different resource than the one the write addresses.
- Where an identifier is sent as a query value (`jira roles remove-actor --user`,
  `jira issue watchers remove`), the value goes through
  `query::encode_query_component`, so it cannot add parameters.
- Bulk commands in Jira and Confluence encode identifiers taken from search
  results. The Bitbucket bulk commands keep recording partial progress: an
  identifier that cannot be encoded stops the run after reporting what was
  already changed, the way a failed request does.

`opsgenie heartbeat get` and `ping` are encoded as well. `get` is a read, but a
name containing `/` reached the mutating ping endpoint.

### Git remote detection

`bitbucket/git.rs::parse_git_remote` now accepts a workspace and a repository
slug only if every character is a letter, a digit, `-`, `_` or `.`, and neither
is `.` or `..`. Anything else is treated as "not a Bitbucket remote": the
function returns `None` and logs a fixed reason at debug level. The log never
contains the remote URL, which may carry credentials.

The path is validated as it was written, because the URL parser would otherwise
hide the problem: it moves `?` and `#` out of the path, resolves `.` and `..`
segments, and turns backslashes into separators. For HTTPS and `ssh://` remotes
the parser is still used for the authority (user info, port, host), and a
remote is rejected when:

- its URL has a query or a fragment;
- the raw spelling contains a control character, a backslash, or leading or
  trailing whitespace;
- the path as written contains `?` or `#`;
- a segment is `.` or `..`, or one of their percent-encoded spellings, in any
  position;
- the workspace or slug is empty or outside the character set.

The scp-style `git@bitbucket.org:workspace/repo.git` form goes through the same
path checks. When the URL comes from `git remote get-url`, only git's line
terminator (`\n` or `\r\n`) is removed before parsing, so a tab or space at
either end of the value is still seen by those checks. A remote that is
rejected is never used to fill in `--workspace` or `--repo`.

## Limitations

What this change deliberately leaves alone:

- **Read-only requests.** GET requests that interpolate an identifier without
  encoding it are unchanged in every product (for example `jira issue get`,
  `bb repo get`, the Bamboo and Opsgenie getters). A read cannot modify a
  resource. The exceptions are the reads that feed a write, listed above, and
  `opsgenie heartbeat get`.
- **Reads that feed writes and still use the raw identifier.** The identifiers
  in these reads come from search or listing results, or from the same
  command-line argument that the write then encodes:
    - `jira bulk label` reads each issue's labels before the encoded write;
      bulk transition now encodes both its per-issue lookup and write paths
      when integrated with PR #177;
    - `jira issue transition` discovers the available transitions and reads the
      current status before the encoded transition write;
    - `bb bulk` repository and branch listings that decide what to change;
    - `bb pipeline rerun` looks the pipeline up through `pipelines/list.rs`
      before it posts.

  A raw identifier in such a read can only fetch other data; the write that
  follows is encoded, or refused.
- **Query values.** Only the two query values named above are encoded.
  `bamboo build run --stage` and `--revision` are still placed in the query
  string as written, and so are date arguments of the Opsgenie on-call and
  timeline reads.
- **Raw `api` passthrough.** `api` and the per-product raw commands send the
  path the caller types; that is their purpose.
- **Profile-derived workspace.** `extract_workspace_from_url` does not check the
  character set of the workspace it takes from a profile base URL. It is encoded
  at every write site now, but not rejected up front.
- **Precedence of the git remote.** A workspace detected from a git remote still
  takes precedence over the one inferred from the profile. Only the character
  check is new.
- **Existing whitelists** (`jira attachment` key and id validation,
  `accept_safe_identifier` for pipeline identifiers) are kept. The Jira
  attachment upload and delete now also encode the value, which changes nothing
  for the accepted character set.
- **File size.** `jira/issues.rs` (about 2900 lines) is over the 2000 line
  guideline and was before this change. The edits there are in place. The tests
  for the mutating commands of every product, including that file's, live in
  new `path_encoding_tests.rs` modules next to each product. Splitting
  `jira/issues.rs` is a follow-up.
- **A segment-based path builder.** The durable alternative to `format!` that
  [08092026_remaining_hardening.md](08092026_remaining_hardening.md) describes
  is still open. This change closes the gap at the write sites without that
  refactor.

## Tests

Helper (`crates/api/src/path.rs`):

- unreserved characters pass through; `?`, `#`, `%`, space, braces, backslash
  and control characters are encoded; an escape such as `%2e%2e` is encoded, not
  decoded; non-ASCII is UTF-8 encoded;
- `/`, `..`, `.` and the empty string are rejected, and the error names the
  value;
- after encoding, a joined URL has no query, no fragment and the same number of
  segments.

Git remote (`crates/cli/src/commands/bitbucket/git.rs`):

- valid SSH and HTTPS remotes are detected, including `.`, `-` and `_`, an
  `ssh://` remote with a port, and HTTPS remotes with user info or a port;
- names containing `?`, `#`, `%`, a space, a backslash or `+`, and dot segments,
  are discarded for SSH and HTTPS;
- HTTPS and SSH remotes with a raw `?` or `#`, a query or fragment, `/./` or
  `/../` in any position, a backslash, or a tab or newline are discarded;
- the line terminator stripping keeps boundary tabs and spaces, so a printed
  remote with a leading or trailing tab or space is discarded end to end.

Request targets (one `path_encoding_tests.rs` module per product, wiremock, the
received path is asserted, not just the helper output):

- Bitbucket: `bb pr reopen-comment` with slug `r?x` and workspace `w?x`, repo
  delete with workspace `w?x`, webhook create with slug `r#x`, and refused
  slugs `r/x`, `..`, `.` and empty;
- Jira: `jira roles remove-actor` with project `P?x` and a user value that
  contains `&` and `=`, comment delete with key `P-1?x`, watcher remove, issue
  delete with `P-1#x`, and refused keys;
- Confluence: page label remove, page delete and attachment upload with page id
  `1?x`, and refused ids;
- Opsgenie: `team remove-member` with team `T?x` and user `u#1`, alert delete,
  heartbeat ping, `heartbeat get a/ping` refused, and refused team names;
- Bamboo and JSM: label remove, plan disable, feedback delete and participant
  remove with delimiters, and refused keys.

In every refused case these tests cover, the mock server records no request, and in every encoded
case it records exactly one request, to the encoded path and not to the parent
endpoint.

A compatibility matrix on Jira and Opsgenie asserts the path the server
receives for values seen in real identifiers: `user@example.test` becomes `%40`,
`5b:1` becomes `%3A`, `{abc-123}` becomes `%7Babc-123%7D`, non-ASCII text is
UTF-8 escaped, and a literal `%2F` is sent as `%252F`.
