# Bitbucket PR Inline Comments

## Date: 2026-08-20

## Problem

`bb pr comment` posted only top-level PR comments. Reviewers routinely want to
anchor feedback to a file — and a specific line on either side of the diff —
which Bitbucket's REST API supports via the `inline` object on
`POST /2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments`.
`bb pr comments` also silently dropped the returned `inline` metadata on the
read path, so users could not tell which comments were anchored where.

## Solution

Extend the existing subcommand with optional flags rather than adding a new
`bb pr inline-comment` verb. The `--path` flag is the switch that turns a
top-level comment into an inline one; `--line` and `--side` refine it.

### CLI surface

```bash
# Top-level PR comment (unchanged behaviour).
bb pr comment my-repo 123 --text "LGTM"

# Inline comment anchored to a line on the new (destination) side of the diff.
bb pr comment my-repo 123 --text "Nit: rename" --path src/main.rs --line 42

# Inline comment anchored to a removed line on the old (source) side.
bb pr comment my-repo 123 --text "Why remove?" --path src/main.rs --line 17 --side old

# Whole-file inline comment (no --line).
bb pr comment my-repo 123 --text "See README" --path README.md
```

Flag validation is enforced by `clap`, not the API:

- `--line` and `--side` both require `--path` (`requires = "path"`).
- `--side` requires `--line` (`requires = "line"`), because Bitbucket only
  distinguishes new/old for a specific line — whole-file comments have neither.
- `--line` is `value_parser!(u32).range(1..)`, so `--line 0` is rejected
  client-side rather than making a round trip for a 400. Bitbucket line numbers
  are 1-indexed.
- `--side` is a `clap::ValueEnum` with values `new` (default) and `old`, mapped
  to the API's `inline.to` and `inline.from` fields respectively.

### API payload

`build_comment_payload(content, inline_path, inline_line, side)` is the single
pure function that owns the JSON shape. It is deliberately independent from
`ApiClient` so it can be unit-tested exhaustively without a mock server:

```json
// Top-level
{"content": {"raw": "LGTM"}}

// Inline, new side
{"content": {"raw": "Nit"}, "inline": {"path": "src/main.rs", "to": 42}}

// Inline, old side
{"content": {"raw": "Why?"}, "inline": {"path": "src/main.rs", "from": 17}}

// Whole-file (no line, side is ignored)
{"content": {"raw": "See README"}, "inline": {"path": "README.md"}}
```

Only one of `to`/`from` is set per request. Sending both is legal per the API
docs but has no semantic value on the write path, and mirroring the user's
`--side` choice keeps the wire format unambiguous.

### Read path

`bb pr comments` now surfaces a `LOCATION` column populated from the response's
`inline` object:

- `None` → empty string (top-level comment)
- `Some { path, to: Some(n) }` → `path:n`
- `Some { path, from: Some(n) }` → `path:n`
- `Some { path, to: None, from: None }` → `path` (whole-file inline)

When both `to` and `from` are present in the response, `to` wins. That is
pinned by `test_format_comment_location_prefers_to_when_both_set` so a future
serde change or refactor cannot silently flip which side is displayed.

## Files modified

- `crates/cli/src/commands/bitbucket/pullrequests.rs` — `Side` enum,
  `build_comment_payload`, extended `add_pr_comment` signature, `Inline`
  struct, `format_comment_location`, `LOCATION` column on the list row.
- `crates/cli/src/commands/bitbucket/mod.rs` — `--path`, `--line`, `--side`
  flags on `PrCommands::Comment`, with `requires` chains, `value_parser`
  range on `--line`, and `long_about` examples.
- `README.md` — new `bb pr comment` examples and feature bullet updated to
  mention inline comments.
- `crates/cli/tests/bitbucket_integration.rs` — 4 wiremock tests covering the
  new-side POST body, old-side POST body, top-level POST body (regression
  guard that no `inline` object leaks in), and mixed-response GET parsing.

## Tests

- **Unit tests** in `crates/cli/src/commands/bitbucket/pullrequests.rs`:
  - `build_comment_payload` — global comment, inline whole-file, inline
    new-side, inline old-side, side-ignored-when-no-line, empty path, unicode.
  - `Side::default()` returns `New`.
  - `Side` `Display` matches the `clap` `value_name` (`"new"`, `"old"`) so
    generated `--help` and error messages stay consistent.
  - `Comment` serde: deserializes with `inline`, without `inline`, and with
    a partially-populated `inline` object.
  - `format_comment_location`: `None`, path only, path+to, path+from, and the
    tie-break case where both `to` and `from` are set (prefers `to`).
- **Clap-parse tests** in `crates/cli/src/commands/bitbucket/mod.rs`: eight
  cases covering the happy paths, each `requires` chain, the default `--side
  new`, and the `--line 0` rejection.
- **Integration tests** in `crates/cli/tests/bitbucket_integration.rs`: four
  wiremock tests using `body_partial_json` to pin the exact POST shapes for
  new-side, old-side, and top-level; plus a GET test that parses a payload
  mixing global comments, `to`-anchored inline, `from`-anchored inline, and a
  whole-file inline comment.

## Deliberately deferred

- **Threaded replies (`parent.id`).** `POST /pullrequests/{id}/comments`
  accepts a `parent` object that turns the new comment into a reply. Adding
  it needs its own UX: a way to identify the parent (`--reply-to <id>`?), and
  a decision about whether `--path` is allowed alongside `--reply-to` (the
  API allows it but it is redundant — replies inherit the parent's anchor).
  Not worth designing here.
- **Suggestions / `edit_url`.** Bitbucket's newer "suggestion" comments piggy-
  back on the same endpoint but require a different content shape. Same
  scoping argument as replies.
- **Client-side path validation.** We do not check that `--path` actually
  exists on either side of the diff. The API rejects invalid paths with a
  useful error, and pre-fetching the diff to validate would double the
  request count on every inline comment.
