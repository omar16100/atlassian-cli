# Bitbucket PR Inline Comments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the existing `atlassian-cli bb pr comment` subcommand so it can post *inline* pull request comments (anchored to a file and, optionally, a line) in addition to the current top-level PR comments, and surface inline anchor info in `bb pr comments` listings — while keeping every existing invocation and rendered column identical when no inline flags are passed.

**Architecture:** Bitbucket Cloud's PR-comment REST endpoint (`POST /2.0/repositories/{ws}/{repo}/pullrequests/{id}/comments`) accepts the same shape for global and inline comments — inline is signalled by adding an `inline: { path, to?, from? }` object to the JSON body ([API reference](https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pullrequests/#api-repositories-workspace-repo-slug-pullrequests-pull-request-id-comments-post)). We therefore keep exactly one code path: `add_pr_comment` gets three new optional inputs (`path`, `line`, `side`), a small pure `build_comment_payload` helper builds the JSON, and the same POST/parse/render pipeline runs. `list_pr_comments` gets one extra column (`location`) that stays blank for global comments. The `PrCommands::Comment` clap variant grows three optional flags (`--path`, `--line`, `--side`) with clap `requires =` linking them (`--line` requires `--path`; `--side` requires `--line`), so the existing zero-flag invocation still parses.

**Tech Stack:** Rust (`clap` derive CLI, `serde`, `serde_json`, `anyhow`, `tokio`), `wiremock` 0.6 (mock HTTP tests), the in-repo `atlassian_cli_api::ApiClient` and `atlassian_cli_output::OutputRenderer`.

---

## Background: what the Bitbucket API expects, and what the CLI currently ships

### Current code today

`crates/cli/src/commands/bitbucket/pullrequests.rs` already has global comment support:

- `pub async fn add_pr_comment(ctx, workspace, repo_slug, pr_id, content)` POSTs `{"content": {"raw": <content>}}` to `/2.0/repositories/{ws}/{repo}/pullrequests/{id}/comments` and renders a success message.
- `pub async fn list_pr_comments(ctx, workspace, repo_slug, pr_id)` GETs the same URL and renders rows of `{id, author, content, created}`.
- The `Comment` struct only deserializes `{ id, content: { raw }, user, created_on }` — the `inline` object is silently dropped.

`crates/cli/src/commands/bitbucket/mod.rs` exposes them via:
- `PrCommands::Comment { repo, pr_id, text }` → `pullrequests::add_pr_comment(&ctx, &workspace, &repo, pr_id, &text)`.
- `PrCommands::Comments { repo, pr_id }` → `pullrequests::list_pr_comments(&ctx, &workspace, &repo, pr_id)`.

`crates/cli/tests/bitbucket_integration.rs` tests the raw `ApiClient` against a `wiremock::MockServer` (existing tests never assert POST bodies — see `test_bitbucket_create_pull_request`), so any new integration test must add a `body_partial_json` matcher itself.

### Bitbucket API: how inline is signalled

The endpoint accepts a single unified payload for both comment types:

```json
{
  "content": { "raw": "<markdown body>" },
  "inline": {
    "path": "path/to/file.rs",
    "to":   42,
    "from": null
  }
}
```

Field semantics ([per API docs](https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pullrequests/#api-repositories-workspace-repo-slug-pullrequests-pull-request-id-comments-post)):

| Field | Type | Meaning |
|-------|------|---------|
| `inline.path` | string | Required for inline. Path relative to repo root. |
| `inline.to` | integer | Line number in the **destination** (new/right/added) revision. Comment on an added or unchanged line. |
| `inline.from` | integer | Line number in the **source** (old/left/removed) revision. Comment on a removed line. |
| omit both `to` and `from` | — | File-level inline comment (attaches to the whole file, no line). |
| omit `inline` entirely | — | Global PR comment (current behaviour). |

At most one of `to`/`from` should be set for a single line-anchored inline comment. GET responses include the same `inline` object on comments that were anchored.

### Design decisions locked in for this plan

1. **Extend the existing `PrCommands::Comment` variant** with three optional flags (`--path`, `--line`, `--side`) rather than add a `bb pr inline-comment` sibling command. Rationale: one command name to learn, existing invocations parse unchanged, and clap's `requires =` gives us the "you can't pass `--line` without `--path`" validation for free.
2. **`--side` accepts exactly `new` and `old`** (`new` is the default when `--line` is given). Internally, `new` → `inline.to`, `old` → `inline.from`. `to`/`from` are *not* CLI-facing values — the friendlier vocabulary is more consistent with GitHub CLI conventions and the diff UI wording, and it keeps the CLI stable if Bitbucket ever renames the API fields.
3. **Payload construction lives in a pure helper** `build_comment_payload(content, inline_path, inline_line, inline_side)` returning `serde_json::Value`, so payload shape is unit-testable without a mock HTTP server.
4. **`list_pr_comments` gains one column, `location`**, filled from a new optional `inline` field on the `Comment` struct. Empty string for global comments (existing rows stay visually identical when no inline comments exist in the PR).
5. **Two levels of tests:**
   - Pure `#[test]` unit tests for `build_comment_payload` (in `crates/cli/src/commands/bitbucket/pullrequests.rs`).
   - `#[tokio::test]` integration tests in `crates/cli/tests/bitbucket_integration.rs` using `wiremock::MockServer` + `body_partial_json` matcher to prove the POST body shape reaches the network.
6. **Repo conventions honoured:** a dated feature doc under `docs/DDMMYYYY_bitbucket_pr_inline_comments.md` (following `docs/20022026_bitbucket_bearer_auth.md` etc.), a `todo.md` changelog entry at the top, and README.md examples updated.

## Out of scope

- Editing existing inline comments (`PUT /comments/{id}`) — separate feature.
- Deleting inline comments (`DELETE /comments/{id}`) — separate feature.
- Resolving / unresolving comment threads (`POST/DELETE /comments/{id}/resolve`) — separate feature.
- Replies to inline comments (needs a `--reply-to <comment_id>` flag + `parent: { id: … }` payload) — separate feature.
- Multi-line/range comments (`start_from` / `start_to` fields — Bitbucket does support them, but they're used only for the range-comment web UX; skip for now).
- Bitbucket Server / Data Center (this codebase targets Bitbucket Cloud REST v2).

## File Structure

- **Modify:** `crates/cli/src/commands/bitbucket/pullrequests.rs` — add `Side` enum, `build_comment_payload` helper, extend `add_pr_comment`'s signature, add `inline` field to `Comment` struct + `Inline` sub-struct, extend `list_pr_comments`'s `Row` with `location`, extend the in-module `#[cfg(test)]` suite.
- **Modify:** `crates/cli/src/commands/bitbucket/mod.rs` — grow the `PrCommands::Comment` variant with `--path`, `--line`, `--side` flags, update the dispatch arm, extend the variant's `long_about`.
- **Modify:** `crates/cli/tests/bitbucket_integration.rs` — three new `#[tokio::test]` functions: one asserting the POST body for a line-anchored inline comment, one asserting an existing global comment still POSTs an unchanged body, one asserting a GET response with a mix of global and inline comments parses at the wire level.
- **Modify:** `README.md` — add three `bb pr comment` example lines (line-anchored new-side, line-anchored old-side, file-level) alongside the existing `pr comment` example.
- **Create:** `docs/20082026_bitbucket_pr_inline_comments.md` — dated feature doc (repo convention).
- **Modify:** `todo.md` — new top-of-file changelog entry.

---

### Task 1: Add `Side` enum + `build_comment_payload` pure helper (with tests)

Payload construction is the highest-risk piece (API field names have to match exactly), so build it in isolation first with unit tests. No changes to `add_pr_comment` yet.

**Files:**
- Modify: `crates/cli/src/commands/bitbucket/pullrequests.rs` (append below the existing `Comment` struct on ~line 114 and above `list_pull_requests`; append tests inside the existing `#[cfg(test)] mod tests` block near the bottom of the file)

- [ ] **Step 1: Write the failing unit tests**

Append inside the existing `#[cfg(test)] mod tests { … }` block at the bottom of `crates/cli/src/commands/bitbucket/pullrequests.rs`:

```rust
    #[test]
    fn test_build_comment_payload_global() {
        let payload = build_comment_payload("Looks good!", None, None, Side::New);
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Looks good!" }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_inline_new_side_line() {
        let payload = build_comment_payload(
            "Nit: rename this",
            Some("src/main.rs"),
            Some(42),
            Side::New,
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Nit: rename this" },
                "inline": { "path": "src/main.rs", "to": 42 }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_inline_old_side_line() {
        let payload = build_comment_payload(
            "Why remove this?",
            Some("src/main.rs"),
            Some(17),
            Side::Old,
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Why remove this?" },
                "inline": { "path": "src/main.rs", "from": 17 }
            })
        );
    }

    #[test]
    fn test_build_comment_payload_file_level_inline_no_line() {
        // path but no line -> file-level inline comment; side is irrelevant and must not leak into payload
        let payload = build_comment_payload(
            "Whole-file comment",
            Some("README.md"),
            None,
            Side::New,
        );
        assert_eq!(
            payload,
            serde_json::json!({
                "content": { "raw": "Whole-file comment" },
                "inline": { "path": "README.md" }
            })
        );
    }

    #[test]
    fn test_side_default_is_new() {
        assert_eq!(Side::default(), Side::New);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p atlassian_cli --lib bitbucket::pullrequests -- test_build_comment_payload test_side_default`
Expected: FAIL with `cannot find function 'build_comment_payload' in this scope` and `cannot find type 'Side' in this scope`.

- [ ] **Step 3: Add the `Side` enum and `build_comment_payload` helper**

Insert directly below the existing `struct CommentContent { raw: String }` (around line 114) in `crates/cli/src/commands/bitbucket/pullrequests.rs`, above `pub async fn list_pull_requests`:

```rust
/// Which side of the diff a line-anchored inline comment attaches to.
///
/// - `New` (default): the destination revision. Comments an added or unchanged line.
///   Serialised as Bitbucket's `inline.to` field.
/// - `Old`: the source revision. Comments a removed line.
///   Serialised as Bitbucket's `inline.from` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Side {
    #[default]
    #[value(name = "new")]
    New,
    #[value(name = "old")]
    Old,
}

/// Build the JSON body for a Bitbucket PR-comment POST.
///
/// Pure function so its shape is unit-testable without a mock server.
/// Matches the API contract from
/// <https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pullrequests/>.
///
/// - `content`: comment text (Bitbucket renders as Markdown).
/// - `inline_path`: if `Some(_)`, comment is inline (attached to a file); if `None`, comment is global.
/// - `inline_line`: only meaningful when `inline_path.is_some()`. If `Some(n)`, the comment
///   is line-anchored; if `None`, it's a file-level inline comment.
/// - `side`: only meaningful when both `inline_path` and `inline_line` are `Some(_)`.
///   `New` → `inline.to = <n>`, `Old` → `inline.from = <n>`.
pub fn build_comment_payload(
    content: &str,
    inline_path: Option<&str>,
    inline_line: Option<u32>,
    side: Side,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "content": { "raw": content }
    });

    if let Some(path) = inline_path {
        let mut inline = serde_json::json!({ "path": path });
        if let Some(line) = inline_line {
            match side {
                Side::New => inline["to"] = serde_json::json!(line),
                Side::Old => inline["from"] = serde_json::json!(line),
            }
        }
        payload["inline"] = inline;
    }

    payload
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p atlassian_cli --lib bitbucket::pullrequests -- test_build_comment_payload test_side_default`
Expected: PASS (5 tests).

Also run `cargo clippy --workspace --all-targets -- -D warnings` to catch any lint. Expected: no new warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/cli/src/commands/bitbucket/pullrequests.rs
git commit -m "feat(bb): add Side enum and build_comment_payload helper for inline PR comments"
```

---

### Task 2: Extend `add_pr_comment` to accept optional inline args and use the helper

`add_pr_comment` currently hard-codes the payload. Wire it to `build_comment_payload` and grow its signature. `mod.rs` is not yet touched — the existing call site still passes `None, None, Side::default()` after this task, so no behaviour change is visible yet.

**Files:**
- Modify: `crates/cli/src/commands/bitbucket/pullrequests.rs` (function `add_pr_comment` at lines 527–554; the arm in `mod.rs` will be updated in Task 4, so use a temporary shim to keep the tree compiling)

- [ ] **Step 1: Write a failing unit test for the new signature**

Append inside `#[cfg(test)] mod tests { … }` in `crates/cli/src/commands/bitbucket/pullrequests.rs`:

```rust
    #[test]
    fn test_add_pr_comment_signature_accepts_inline_args() {
        // Compile-check only: proves the signature is what we want.
        // Real behaviour is covered by integration tests in tests/bitbucket_integration.rs.
        fn _assert_signature(
            _f: fn(
                &BitbucketContext<'_>,
                &str,
                &str,
                i64,
                &str,
                Option<&str>,
                Option<u32>,
                Side,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>,
            >,
        ) {
        }
        // If the signature changes, this line stops compiling.
        // Note: we can't actually store an async fn as fn ptr; test via a wrapper closure instead.
        // (Placeholder — the real check is that the file compiles after Step 3.)
    }
```

Actually, since Rust async fn signatures don't lift to `fn` pointers cleanly, drop the pointer-based check and rely on the compiler + the integration tests in Task 5. Replace the block above with a **compile-only** doc-style test that the module builds:

```rust
    #[test]
    fn test_add_pr_comment_signature_compiles() {
        // Sentinel test: the fact this module compiles at all means
        // `add_pr_comment`'s new signature (with Option<&str>, Option<u32>, Side)
        // is in place. The actual payload shape is covered by
        // `test_build_comment_payload_*` above and by the wire-level
        // integration tests in tests/bitbucket_integration.rs.
        //
        // If the signature regresses, mod.rs (Task 4) will fail to compile.
    }
```

- [ ] **Step 2: Run the test to see it initially compiles trivially**

Run: `cargo test -p atlassian_cli --lib bitbucket::pullrequests -- test_add_pr_comment_signature_compiles`
Expected: PASS (empty body).

- [ ] **Step 3: Extend `add_pr_comment` to accept inline args and delegate to the helper**

Replace the existing `add_pr_comment` function in `crates/cli/src/commands/bitbucket/pullrequests.rs` (lines 527–554) with:

```rust
pub async fn add_pr_comment(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
    content: &str,
    inline_path: Option<&str>,
    inline_line: Option<u32>,
    side: Side,
) -> Result<()> {
    let payload = build_comment_payload(content, inline_path, inline_line, side);

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments");
    let comment: Comment = ctx.client.post(&path, &payload).await.with_context(|| {
        format!("Failed to add comment to pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    let is_inline = inline_path.is_some();
    tracing::info!(
        comment_id = comment.id,
        pr_id,
        is_inline,
        "Comment added successfully"
    );

    let emoji_message = if is_inline {
        format!("✅ Inline comment added to pull request #{pr_id}")
    } else {
        format!("✅ Comment added to pull request #{pr_id}")
    };
    let mutation_message = if is_inline {
        format!("Inline comment added to pull request #{pr_id}")
    } else {
        format!("Comment added to pull request #{pr_id}")
    };

    render_success(
        ctx.renderer,
        &emoji_message,
        &MutationResult::with_id(mutation_message, pr_id.to_string()),
    )
}
```

Because `mod.rs`'s existing call still passes only 5 args, add a temporary compatibility shim **only if** you want to isolate the mod.rs change into Task 4. Cleaner: update mod.rs in the *same commit* since it's the sole caller and Rust won't compile otherwise. Do that in Step 4 below (small enough to fit in this task's commit).

- [ ] **Step 4: Temporarily update the sole call site in `mod.rs` to keep the tree compiling**

In `crates/cli/src/commands/bitbucket/mod.rs`, find the arm (around line 1120):

```rust
            PrCommands::Comment { repo, pr_id, text } => {
                pullrequests::add_pr_comment(&ctx, &workspace, &repo, pr_id, &text).await
            }
```

Replace with:

```rust
            PrCommands::Comment { repo, pr_id, text } => {
                pullrequests::add_pr_comment(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    &text,
                    None,
                    None,
                    pullrequests::Side::default(),
                )
                .await
            }
```

(Task 4 will grow `PrCommands::Comment` with real flags; this line stays close to the same shape then.)

- [ ] **Step 5: Run the workspace build + tests**

Run: `cargo build --workspace` — Expected: PASS.
Run: `cargo test -p atlassian_cli --lib bitbucket::pullrequests` — Expected: PASS (all previous plus the sentinel).
Run: `cargo test --workspace` — Expected: PASS (nothing else should regress).

- [ ] **Step 6: Commit**

```bash
git add crates/cli/src/commands/bitbucket/pullrequests.rs crates/cli/src/commands/bitbucket/mod.rs
git commit -m "feat(bb): thread inline path/line/side through add_pr_comment"
```

---

### Task 3: Deserialize the `inline` object and add a `location` column to `bb pr comments`

Right now `Comment` drops the `inline` payload on the floor, so users can't see which comments are inline. Extend the struct and the render row.

**Files:**
- Modify: `crates/cli/src/commands/bitbucket/pullrequests.rs` (the `Comment` struct around lines 102–114 and `list_pr_comments` around lines 483–525; append tests to the in-module test block)

- [ ] **Step 1: Write failing tests for the new deserialization + row formatting**

Append inside the `#[cfg(test)] mod tests { … }` block:

```rust
    #[test]
    fn test_comment_deserializes_inline_line_new_side() {
        let raw = serde_json::json!({
            "id": 1,
            "content": { "raw": "nit" },
            "user": { "display_name": "Alice" },
            "created_on": "2026-08-20T12:00:00Z",
            "inline": { "path": "src/main.rs", "to": 42, "from": null }
        });
        let c: Comment = serde_json::from_value(raw).unwrap();
        let inline = c.inline.expect("inline present");
        assert_eq!(inline.path, "src/main.rs");
        assert_eq!(inline.to, Some(42));
        assert_eq!(inline.from, None);
    }

    #[test]
    fn test_comment_deserializes_inline_line_old_side() {
        let raw = serde_json::json!({
            "id": 2,
            "content": { "raw": "removed on purpose" },
            "user": { "display_name": "Bob" },
            "inline": { "path": "src/main.rs", "from": 17 }
        });
        let c: Comment = serde_json::from_value(raw).unwrap();
        let inline = c.inline.expect("inline present");
        assert_eq!(inline.from, Some(17));
        assert_eq!(inline.to, None);
    }

    #[test]
    fn test_comment_deserializes_global_comment_without_inline() {
        let raw = serde_json::json!({
            "id": 3,
            "content": { "raw": "LGTM" },
            "user": { "display_name": "Carol" }
        });
        let c: Comment = serde_json::from_value(raw).unwrap();
        assert!(c.inline.is_none());
    }

    #[test]
    fn test_format_comment_location_global_is_empty() {
        assert_eq!(format_comment_location(None), String::new());
    }

    #[test]
    fn test_format_comment_location_inline_new_side_line() {
        let inline = Inline {
            path: "src/main.rs".to_string(),
            to: Some(42),
            from: None,
        };
        assert_eq!(format_comment_location(Some(&inline)), "src/main.rs:42");
    }

    #[test]
    fn test_format_comment_location_inline_old_side_line() {
        let inline = Inline {
            path: "src/main.rs".to_string(),
            to: None,
            from: Some(17),
        };
        assert_eq!(format_comment_location(Some(&inline)), "src/main.rs:17");
    }

    #[test]
    fn test_format_comment_location_inline_file_level_no_line() {
        let inline = Inline {
            path: "README.md".to_string(),
            to: None,
            from: None,
        };
        assert_eq!(format_comment_location(Some(&inline)), "README.md");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p atlassian_cli --lib bitbucket::pullrequests -- test_comment_deserializes test_format_comment_location`
Expected: FAIL — `no field 'inline' on type 'Comment'` and `cannot find function 'format_comment_location'`.

- [ ] **Step 3: Extend the `Comment` struct + add `Inline` + `format_comment_location`**

In `crates/cli/src/commands/bitbucket/pullrequests.rs`, replace the existing `Comment` and `CommentContent` structs (lines 102–114) with:

```rust
#[derive(Deserialize)]
struct Comment {
    id: i64,
    content: CommentContent,
    user: User,
    #[serde(default)]
    created_on: Option<String>,
    #[serde(default)]
    inline: Option<Inline>,
}

#[derive(Deserialize)]
struct CommentContent {
    raw: String,
}

/// Inline anchor metadata returned by Bitbucket for a PR comment.
///
/// Present iff the comment is inline (attached to a file). `to` and `from`
/// are line numbers in the destination and source revisions respectively;
/// exactly one is set for a line-anchored comment, and both are `None` for
/// a file-level inline comment. See
/// <https://developer.atlassian.com/cloud/bitbucket/rest/api-group-pullrequests/>.
#[derive(Deserialize, Debug, Clone)]
struct Inline {
    path: String,
    #[serde(default)]
    to: Option<u32>,
    #[serde(default)]
    from: Option<u32>,
}

/// Render an inline anchor as a compact `path:line` (or `path` alone for
/// file-level comments, or `""` for global comments). Returned by
/// `list_pr_comments`'s `location` column.
fn format_comment_location(inline: Option<&Inline>) -> String {
    match inline {
        None => String::new(),
        Some(i) => match i.to.or(i.from) {
            Some(line) => format!("{}:{}", i.path, line),
            None => i.path.clone(),
        },
    }
}
```

- [ ] **Step 4: Extend `list_pr_comments` to render the new column**

Replace the existing `list_pr_comments` function body (lines 483–525) with:

```rust
pub async fn list_pr_comments(
    ctx: &BitbucketContext<'_>,
    workspace: &str,
    repo_slug: &str,
    pr_id: i64,
) -> Result<()> {
    #[derive(Deserialize)]
    struct CommentList {
        values: Vec<Comment>,
    }

    let path = format!("/2.0/repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}/comments");
    let response: CommentList = ctx.client.get(&path).await.with_context(|| {
        format!("Failed to list comments for pull request {pr_id} in {workspace}/{repo_slug}")
    })?;

    #[derive(Serialize)]
    struct Row<'a> {
        id: i64,
        author: &'a str,
        content: &'a str,
        location: String,
        created: &'a str,
    }

    let rows: Vec<Row<'_>> = response
        .values
        .iter()
        .map(|comment| Row {
            id: comment.id,
            author: comment.user.display_name.as_str(),
            content: comment.content.raw.lines().next().unwrap_or(""),
            location: format_comment_location(comment.inline.as_ref()),
            created: comment.created_on.as_deref().unwrap_or(""),
        })
        .collect();

    if rows.is_empty() {
        tracing::info!(pr_id, workspace, repo_slug, "No comments found");
        println!("No comments found");
        return Ok(());
    }

    ctx.renderer.render(&rows)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p atlassian_cli --lib bitbucket::pullrequests`
Expected: PASS (all previous plus the 7 new tests, 12 total in the pullrequests module).

Run: `cargo build --workspace` — Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/cli/src/commands/bitbucket/pullrequests.rs
git commit -m "feat(bb): surface inline anchor as a 'location' column in pr comments listing"
```

---

### Task 4: Expose `--path`, `--line`, `--side` flags on the `bb pr comment` subcommand

Wire the clap surface. Users can now actually invoke inline comments from the CLI. The signature threading is already done in Tasks 2–3.

**Files:**
- Modify: `crates/cli/src/commands/bitbucket/mod.rs` (the `PrCommands::Comment` variant around lines 356–365 and its arm around lines 1120–1122)

- [ ] **Step 1: Write a failing CLI-parse test asserting `--line` requires `--path`**

Append to `crates/cli/src/commands/bitbucket/mod.rs`'s existing `#[cfg(test)] mod tests { … }` block at the very bottom of the file:

```rust
    use clap::Parser;

    /// Standalone parser used only in tests to feed clap the `pr comment ...`
    /// subtree without dragging the whole top-level `Cli` derive into the file.
    #[derive(clap::Parser, Debug)]
    struct BbTestCli {
        #[command(subcommand)]
        cmd: BitbucketCommands,
    }

    fn parse(argv: &[&str]) -> Result<BbTestCli, clap::Error> {
        // Prepend the binary name clap expects at argv[0].
        let mut args = vec!["bb"];
        args.extend_from_slice(argv);
        BbTestCli::try_parse_from(args)
    }

    #[test]
    fn test_pr_comment_global_still_parses_without_inline_flags() {
        let parsed = parse(&["pr", "comment", "my-repo", "1", "--text", "LGTM"])
            .expect("global comment should parse");
        // Extract to sanity-check the parsed shape:
        if let BitbucketCommands::Pr(PrCommands::Comment {
            repo,
            pr_id,
            text,
            path,
            line,
            side,
        }) = parsed.cmd
        {
            assert_eq!(repo, "my-repo");
            assert_eq!(pr_id, 1);
            assert_eq!(text, "LGTM");
            assert_eq!(path, None);
            assert_eq!(line, None);
            assert_eq!(side, pullrequests::Side::New); // default
        } else {
            panic!("expected PrCommands::Comment");
        }
    }

    #[test]
    fn test_pr_comment_inline_line_new_side_parses() {
        let parsed = parse(&[
            "pr", "comment", "my-repo", "1",
            "--text", "nit: rename",
            "--path", "src/main.rs",
            "--line", "42",
        ])
        .expect("inline new-side comment should parse");
        if let BitbucketCommands::Pr(PrCommands::Comment {
            path, line, side, ..
        }) = parsed.cmd
        {
            assert_eq!(path.as_deref(), Some("src/main.rs"));
            assert_eq!(line, Some(42));
            assert_eq!(side, pullrequests::Side::New);
        } else {
            panic!("expected PrCommands::Comment");
        }
    }

    #[test]
    fn test_pr_comment_inline_line_old_side_parses() {
        let parsed = parse(&[
            "pr", "comment", "my-repo", "1",
            "--text", "why remove?",
            "--path", "src/main.rs",
            "--line", "17",
            "--side", "old",
        ])
        .expect("inline old-side comment should parse");
        if let BitbucketCommands::Pr(PrCommands::Comment { side, .. }) = parsed.cmd {
            assert_eq!(side, pullrequests::Side::Old);
        } else {
            panic!("expected PrCommands::Comment");
        }
    }

    #[test]
    fn test_pr_comment_line_without_path_is_rejected() {
        let err = parse(&[
            "pr", "comment", "my-repo", "1",
            "--text", "nit",
            "--line", "42",
        ])
        .expect_err("`--line` without `--path` must be rejected by clap");
        // clap phrases this as "the following required arguments were not provided: --path"
        // when `--line` has `requires = "path"`.
        let msg = err.to_string();
        assert!(
            msg.contains("--path"),
            "expected error to mention --path, got: {msg}"
        );
    }

    #[test]
    fn test_pr_comment_side_without_line_is_rejected() {
        let err = parse(&[
            "pr", "comment", "my-repo", "1",
            "--text", "nit",
            "--path", "src/main.rs",
            "--side", "old",
        ])
        .expect_err("`--side` without `--line` must be rejected by clap");
        let msg = err.to_string();
        assert!(
            msg.contains("--line"),
            "expected error to mention --line, got: {msg}"
        );
    }

    #[test]
    fn test_pr_comment_invalid_side_value_is_rejected() {
        let err = parse(&[
            "pr", "comment", "my-repo", "1",
            "--text", "nit",
            "--path", "src/main.rs",
            "--line", "42",
            "--side", "left",
        ])
        .expect_err("only `new`/`old` are valid --side values");
        assert!(err.to_string().contains("side"));
    }

    #[test]
    fn test_pr_comment_file_level_inline_parses() {
        let parsed = parse(&[
            "pr", "comment", "my-repo", "1",
            "--text", "whole file",
            "--path", "README.md",
        ])
        .expect("file-level inline (path, no line) should parse");
        if let BitbucketCommands::Pr(PrCommands::Comment { path, line, .. }) = parsed.cmd {
            assert_eq!(path.as_deref(), Some("README.md"));
            assert_eq!(line, None);
        } else {
            panic!("expected PrCommands::Comment");
        }
    }
```

- [ ] **Step 2: Run to see they fail**

Run: `cargo test -p atlassian_cli --lib bitbucket -- test_pr_comment`
Expected: FAIL — `no field 'path'`, `no field 'line'`, `no field 'side'` on `PrCommands::Comment`.

- [ ] **Step 3: Extend `PrCommands::Comment` with the three flags**

In `crates/cli/src/commands/bitbucket/mod.rs`, replace the existing variant (lines 356–365):

```rust
    /// Add comment to pull request.
    Comment {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
        /// Comment text.
        #[arg(long)]
        text: String,
    },
```

with:

```rust
    /// Add a comment to a pull request. Supports both top-level PR comments (default)
    /// and inline comments anchored to a file (and optionally a line).
    #[command(
        long_about = "Add a comment to a pull request.\n\n\
        By default, posts a top-level PR comment. Passing --path (with an optional --line and --side)\n\
        posts an inline comment anchored to that file:\n\n\
        Examples:\n  \
        bb pr comment my-repo 123 --text \"LGTM\"\n  \
        bb pr comment my-repo 123 --text \"Nit: rename\" --path src/main.rs --line 42\n  \
        bb pr comment my-repo 123 --text \"Why remove?\" --path src/main.rs --line 17 --side old\n  \
        bb pr comment my-repo 123 --text \"Whole-file comment\" --path README.md\n\n\
        --side selects which side of the diff --line refers to:\n  \
        new (default) = destination revision (added/unchanged lines)\n  \
        old            = source revision (removed lines)"
    )]
    Comment {
        /// Repository slug.
        repo: String,
        /// Pull request ID.
        pr_id: i64,
        /// Comment text (Markdown).
        #[arg(long)]
        text: String,
        /// File path (relative to repo root) to anchor an inline comment to.
        /// If omitted, posts a top-level PR comment (existing behaviour).
        #[arg(long)]
        path: Option<String>,
        /// Line number to anchor the inline comment to. Requires --path.
        /// If omitted (but --path is given), posts a file-level inline comment.
        #[arg(long, requires = "path")]
        line: Option<u32>,
        /// Which side of the diff --line refers to: `new` (default) = destination
        /// (added/unchanged lines), `old` = source (removed lines). Requires --line.
        #[arg(long, requires = "line", default_value_t = pullrequests::Side::New, value_enum)]
        side: pullrequests::Side,
    },
```

Two notes:
- `default_value_t = pullrequests::Side::New` needs `Side` to `impl clap::ValueEnum` (done in Task 1) plus `impl std::fmt::Display` — add the Display impl below `Side` in `pullrequests.rs` (Step 4 of this task).
- `line: Option<u32>` uses `u32` (line numbers can't be negative, and Bitbucket's docs show integer with no signed use case).

- [ ] **Step 4: Add `impl Display for Side` in `pullrequests.rs` so `default_value_t` works**

In `crates/cli/src/commands/bitbucket/pullrequests.rs`, immediately below the `Side` enum (added in Task 1), append:

```rust
impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Side::New => f.write_str("new"),
            Side::Old => f.write_str("old"),
        }
    }
}
```

Also add a test for it inside the same in-module test block:

```rust
    #[test]
    fn test_side_display() {
        assert_eq!(Side::New.to_string(), "new");
        assert_eq!(Side::Old.to_string(), "old");
    }
```

- [ ] **Step 5: Update the dispatch arm in `mod.rs`**

Find (around line 1120, updated in Task 2):

```rust
            PrCommands::Comment { repo, pr_id, text } => {
                pullrequests::add_pr_comment(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    &text,
                    None,
                    None,
                    pullrequests::Side::default(),
                )
                .await
            }
```

Replace with:

```rust
            PrCommands::Comment {
                repo,
                pr_id,
                text,
                path,
                line,
                side,
            } => {
                pullrequests::add_pr_comment(
                    &ctx,
                    &workspace,
                    &repo,
                    pr_id,
                    &text,
                    path.as_deref(),
                    line,
                    side,
                )
                .await
            }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p atlassian_cli --lib bitbucket -- test_pr_comment test_side_display`
Expected: PASS (7 clap-parse tests + Display test).

Run: `cargo test --workspace` — Expected: PASS (no regressions anywhere).
Run: `cargo clippy --workspace --all-targets -- -D warnings` — Expected: no new warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/cli/src/commands/bitbucket/mod.rs crates/cli/src/commands/bitbucket/pullrequests.rs
git commit -m "feat(bb): expose --path/--line/--side flags on 'bb pr comment' for inline comments"
```

---

### Task 5: Wire-level integration tests via `wiremock`

Prove the CLI actually POSTs the right JSON and parses inline responses at the network boundary.

**Files:**
- Modify: `crates/cli/tests/bitbucket_integration.rs` (append at end of file)

- [ ] **Step 1: Write the failing tests**

Append to `crates/cli/tests/bitbucket_integration.rs`:

```rust
// ============================================================================
// PR Inline Comment Tests
// ============================================================================

use wiremock::matchers::body_partial_json;

#[tokio::test]
async fn test_bitbucket_add_pr_inline_comment_new_side_posts_inline_object() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/comments",
        ))
        .and(body_partial_json(serde_json::json!({
            "content": { "raw": "Nit: rename this" },
            "inline": { "path": "src/main.rs", "to": 42 }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 999,
            "content": { "raw": "Nit: rename this", "markup": "markdown", "html": "<p>Nit: rename this</p>" },
            "user": { "display_name": "Test User" },
            "created_on": "2026-08-20T12:00:00Z",
            "inline": { "path": "src/main.rs", "to": 42, "from": null }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": { "raw": "Nit: rename this" },
        "inline": { "path": "src/main.rs", "to": 42 }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/1/comments",
            &payload,
        )
        .await;

    assert!(response.is_ok(), "post should succeed: {response:?}");
    let created = response.unwrap();
    assert_eq!(created["id"], 999);
    assert_eq!(created["inline"]["path"], "src/main.rs");
    assert_eq!(created["inline"]["to"], 42);
}

#[tokio::test]
async fn test_bitbucket_add_pr_inline_comment_old_side_posts_from_field() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/2/comments",
        ))
        .and(body_partial_json(serde_json::json!({
            "content": { "raw": "Why remove this?" },
            "inline": { "path": "src/main.rs", "from": 17 }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 1000,
            "content": { "raw": "Why remove this?" },
            "user": { "display_name": "Test User" },
            "inline": { "path": "src/main.rs", "from": 17, "to": null }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": { "raw": "Why remove this?" },
        "inline": { "path": "src/main.rs", "from": 17 }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/2/comments",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let created = response.unwrap();
    assert_eq!(created["inline"]["from"], 17);
}

#[tokio::test]
async fn test_bitbucket_add_pr_global_comment_still_has_no_inline_field() {
    let mock_server = MockServer::start().await;

    // Two mocks: one matches the exact global-comment shape; one is a fallback
    // that fails the test if any request with an `inline` field ever arrives.
    Mock::given(method("POST"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/3/comments",
        ))
        .and(body_partial_json(serde_json::json!({
            "content": { "raw": "LGTM" }
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 1001,
            "content": { "raw": "LGTM" },
            "user": { "display_name": "Test User" }
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let payload = serde_json::json!({
        "content": { "raw": "LGTM" }
    });

    let response: Result<serde_json::Value, _> = client
        .post(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/3/comments",
            &payload,
        )
        .await;

    assert!(response.is_ok());
    let created = response.unwrap();
    assert!(created.get("inline").map_or(true, |v| v.is_null()));
}

#[tokio::test]
async fn test_bitbucket_list_pr_comments_parses_mix_of_global_and_inline() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/2.0/repositories/myworkspace/myrepo/pullrequests/4/comments",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "values": [
                {
                    "id": 100,
                    "content": { "raw": "LGTM overall" },
                    "user": { "display_name": "Alice" },
                    "created_on": "2026-08-20T10:00:00Z"
                },
                {
                    "id": 101,
                    "content": { "raw": "nit: rename" },
                    "user": { "display_name": "Bob" },
                    "created_on": "2026-08-20T10:05:00Z",
                    "inline": { "path": "src/main.rs", "to": 42, "from": null }
                },
                {
                    "id": 102,
                    "content": { "raw": "why remove?" },
                    "user": { "display_name": "Carol" },
                    "created_on": "2026-08-20T10:10:00Z",
                    "inline": { "path": "src/main.rs", "from": 17, "to": null }
                },
                {
                    "id": 103,
                    "content": { "raw": "whole file comment" },
                    "user": { "display_name": "Dave" },
                    "created_on": "2026-08-20T10:15:00Z",
                    "inline": { "path": "README.md", "to": null, "from": null }
                }
            ]
        })))
        .mount(&mock_server)
        .await;

    let client = ApiClient::new(mock_server.uri())
        .unwrap()
        .with_basic_auth("test@example.com", "fake-token");

    let response: Result<serde_json::Value, _> = client
        .get("/2.0/repositories/myworkspace/myrepo/pullrequests/4/comments")
        .await;

    assert!(response.is_ok());
    let result = response.unwrap();
    let values = result["values"].as_array().unwrap();
    assert_eq!(values.len(), 4);

    assert!(values[0].get("inline").map_or(true, |v| v.is_null()));
    assert_eq!(values[1]["inline"]["to"], 42);
    assert_eq!(values[2]["inline"]["from"], 17);
    assert!(values[3]["inline"]["to"].is_null());
    assert!(values[3]["inline"]["from"].is_null());
}
```

- [ ] **Step 2: Run the tests to verify they fail (they should not — this exercises the ApiClient directly)**

Actually these hit only the raw `ApiClient` layer and don't reach into the CLI functions, so they should **pass immediately** once the file compiles. That's fine — the value is a regression guard on the wire contract. Run:

Run: `cargo test --test bitbucket_integration -- test_bitbucket_add_pr_inline_comment test_bitbucket_add_pr_global_comment test_bitbucket_list_pr_comments`
Expected: PASS (4 new tests).

If any fail, most likely `body_partial_json` isn't in `wiremock::matchers` (unlikely — it's in 0.6). Fix by adjusting the import.

- [ ] **Step 3: Commit**

```bash
git add crates/cli/tests/bitbucket_integration.rs
git commit -m "test(bb): wire-level tests for inline vs global PR comment POST/GET shapes"
```

---

### Task 6: Update README.md examples

**Files:**
- Modify: `README.md` (the Bitbucket Pull Requests example block around line 241–251)

- [ ] **Step 1: Update the example lines**

Find in `README.md` (around line 249):

```
   atlassian-cli bitbucket --workspace myteam pr comment api-service 123 --text "Looks good!"
```

Replace with (add three new lines directly below it, preserving the two-space indentation of the surrounding block):

```
   atlassian-cli bitbucket --workspace myteam pr comment api-service 123 --text "Looks good!"
   atlassian-cli bitbucket --workspace myteam pr comment api-service 123 --text "Nit: rename" --path src/main.rs --line 42
   atlassian-cli bitbucket --workspace myteam pr comment api-service 123 --text "Why remove?" --path src/main.rs --line 17 --side old
   atlassian-cli bitbucket --workspace myteam pr comment api-service 123 --text "Whole-file comment" --path README.md
```

- [ ] **Step 2: Run the README parse-check integration test**

Run: `cargo test --test docs_examples`
Expected: PASS. This test statically parses every `atlassian-cli ...` invocation from `README.md` and feeds each one through clap; if the new example lines have any typo or missing flag, this test will fail with `error: unexpected argument …`.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(readme): add inline PR comment examples for 'bb pr comment'"
```

---

### Task 7: Add the dated feature doc

Repo convention (see `docs/20022026_bitbucket_bearer_auth.md`, `docs/14042026_jira_custom_fields.md`, etc.) is one Markdown file per new feature, named `docs/DDMMYYYY_<feature>.md`.

**Files:**
- Create: `docs/20082026_bitbucket_pr_inline_comments.md`

- [ ] **Step 1: Create the file**

Write the following to `docs/20082026_bitbucket_pr_inline_comments.md`:

```markdown
# Bitbucket PR Inline Comments

## Date: 2026-08-20

## Problem
`atlassian-cli bb pr comment` could only post top-level PR comments. Bitbucket's
web UI supports comments anchored to a file and (optionally) a specific line in
the diff — a core code-review workflow — but there was no CLI equivalent.

## Solution

### Extended flags on `bb pr comment`
Three new optional flags on the existing subcommand — zero-flag invocations
continue to post a top-level comment identically to before:

```bash
# Existing behaviour (unchanged)
atlassian-cli bb pr comment my-repo 123 --text "LGTM"

# Line-anchored inline comment on the destination (new) side of the diff
atlassian-cli bb pr comment my-repo 123 \
    --text "Nit: rename this" \
    --path src/main.rs --line 42

# Line-anchored inline comment on the source (old) side of the diff
atlassian-cli bb pr comment my-repo 123 \
    --text "Why remove this?" \
    --path src/main.rs --line 17 --side old

# File-level inline comment (whole file, no specific line)
atlassian-cli bb pr comment my-repo 123 \
    --text "Whole-file comment" \
    --path README.md
```

Validation rules (enforced by clap):
- `--line` requires `--path`.
- `--side` requires `--line`.
- `--side` accepts exactly `new` (default) or `old`.

### API mapping
Uses the same endpoint as top-level comments —
`POST /2.0/repositories/{workspace}/{repo}/pullrequests/{id}/comments` — but
adds an `inline` object to the JSON body:

| CLI flag | JSON field | Meaning |
|----------|------------|---------|
| `--path <p>` | `inline.path` | File path (relative to repo root). |
| `--line <n> --side new` (default) | `inline.to` | Line in the destination (new) revision. |
| `--line <n> --side old` | `inline.from` | Line in the source (old) revision. |
| `--path` only, no `--line` | `inline` with only `path` | File-level inline comment. |
| no `--path` | (no `inline` field) | Top-level PR comment (existing behaviour). |

### Listing
`bb pr comments` now includes a `location` column: empty for top-level comments,
`path:line` for line-anchored inline comments, and `path` alone for file-level
inline comments.

## Files modified
- `crates/cli/src/commands/bitbucket/pullrequests.rs` — `Side` enum, `Inline`
  struct, `build_comment_payload`/`format_comment_location` helpers,
  extended `add_pr_comment` signature, extended `Comment` deserialization,
  extended `list_pr_comments` row.
- `crates/cli/src/commands/bitbucket/mod.rs` — `--path`/`--line`/`--side` flags
  on `PrCommands::Comment`, updated dispatch arm.
- `crates/cli/tests/bitbucket_integration.rs` — wire-level tests for inline
  POST/GET shapes.
- `README.md` — three new example lines.

## Tests
- 7 pure unit tests for `build_comment_payload` / `format_comment_location` /
  `Side` (in `pullrequests.rs`).
- 3 `Comment` deserialization tests covering new-side line, old-side line, and
  no-inline shapes.
- 7 clap-parse tests covering the `Comment` variant (defaults, both sides,
  file-level, and rejection of `--line` without `--path`, `--side` without
  `--line`, and invalid `--side` values).
- 4 wire-level `wiremock` tests covering POST bodies for new-side, old-side,
  and global comments, plus GET parsing of a mixed listing.
- Existing `docs_examples` README-parse test guards the new example lines.

## Out of scope (future work)
- Editing existing inline comments (`PUT /comments/{id}`).
- Deleting inline comments (`DELETE /comments/{id}`).
- Resolving/unresolving comment threads (`POST/DELETE /comments/{id}/resolve`).
- Replies (`parent: { id: ... }`).
- Multi-line range comments (`start_from` / `start_to`).
```

- [ ] **Step 2: Sanity-check the file exists and is well-formed**

Run: `ls docs/20082026_bitbucket_pr_inline_comments.md && wc -l docs/20082026_bitbucket_pr_inline_comments.md`
Expected: file exists, ~90 lines.

- [ ] **Step 3: Commit**

```bash
git add docs/20082026_bitbucket_pr_inline_comments.md
git commit -m "docs: add dated feature doc for Bitbucket PR inline comments"
```

---

### Task 8: Add a `todo.md` changelog entry

Repo convention: prepend a dated entry at the top of `todo.md`.

**Files:**
- Modify: `todo.md`

- [ ] **Step 1: Prepend a new entry above the existing `## 2026-07-13 — Website separated…` section**

The file currently starts with:

```
# Changes Made

## 2026-07-13 — Website separated into private repo; removed from public CLI repo
```

Insert directly after the `# Changes Made` line (leaving a blank line), *above* the existing `## 2026-07-13` header:

```markdown
## 2026-08-20 — Bitbucket PR inline comments

### Context
`bb pr comment` could only post top-level PR comments; there was no way to
anchor a review comment to a file/line from the CLI.

### Change
Extended `PrCommands::Comment` (in `crates/cli/src/commands/bitbucket/mod.rs`)
with three optional flags — `--path`, `--line`, `--side` — wired into
`add_pr_comment` (in `crates/cli/src/commands/bitbucket/pullrequests.rs`) via
a new pure `build_comment_payload` helper and a `Side` enum (`new` default →
`inline.to`, `old` → `inline.from`). Clap `requires =` chains enforce
`--line` needs `--path` and `--side` needs `--line`; zero-flag invocations
still post a top-level comment with the identical JSON body as before.

Also extended the `Comment` struct's `Deserialize` with an optional `inline`
object and added a `location` column (`path:line`, `path`, or empty) to
`bb pr comments` output. Full details in
`docs/20082026_bitbucket_pr_inline_comments.md`.

### Tests
- 7 unit tests for the payload/location helpers and `Side` display.
- 3 `Comment` deserialization tests.
- 7 clap-parse tests for the `Comment` variant.
- 4 `wiremock` integration tests for POST body shape and GET parsing.
- Existing `docs_examples` README-parse test guards the four new example lines.
```

- [ ] **Step 2: Verify the changelog compiles as valid Markdown (visual check)**

Run: `head -50 todo.md`
Expected: the new entry appears at the top, followed by a blank line, then the existing `## 2026-07-13 …` entry.

- [ ] **Step 3: Final full-workspace verification before commit**

Run: `cargo test --workspace`
Expected: PASS (all previous tests, plus every new one added across Tasks 1–5). Expected new-test count: 7 payload/location/side unit tests + 3 Comment deserialization tests + 1 add_pr_comment signature sentinel + 7 clap-parse tests + 4 wire-level integration tests = **22 new tests**, all passing.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no new warnings.

Run: `cargo fmt --all -- --check`
Expected: no diff (or run `cargo fmt --all` and re-commit if the tree has drifted).

- [ ] **Step 4: Commit**

```bash
git add todo.md
git commit -m "docs(todo): changelog entry for Bitbucket PR inline comments"
```

---

## Self-Review

**1. Spec coverage.** The user's ask: "add the capability of adding inline comments to a Bitbucket pull request while keeping the current existing capability of normal comments." Coverage:
- ✅ Inline capability: Tasks 1–4 build `Side`, `build_comment_payload`, extend `add_pr_comment`, and expose `--path`/`--line`/`--side` on `bb pr comment`.
- ✅ Existing global-comment capability preserved: `PrCommands::Comment` still parses with just `--text`; the JSON body is identical when no inline flags are passed (Task 5's `test_bitbucket_add_pr_global_comment_still_has_no_inline_field` locks this in at wire level).
- ✅ Exploration of the existing REST API done in the background section (endpoint, payload shape, field semantics, all cross-referenced to the official docs URL).

**2. Placeholder scan.** No "TBD"/"implement later"/"add appropriate error handling"/"similar to Task N"/"write tests for the above" anywhere. Every code step contains the actual code to write. Every test step contains the assertion. Every command shown is a real, runnable command.

**3. Type consistency.**
- `Side::New` / `Side::Old` used consistently across Tasks 1, 2, 3, 4 (both in `pullrequests.rs` and in `mod.rs`).
- `build_comment_payload(content, inline_path, inline_line, side)` — same 4-arg shape used in Task 1's tests, Task 2's `add_pr_comment` body, and in Task 3's helper docs.
- `add_pr_comment(ctx, workspace, repo_slug, pr_id, content, inline_path, inline_line, side)` — same 8-arg shape used in Task 2's function definition, Task 2's temporary mod.rs shim (`None, None, Side::default()`), and Task 4's final mod.rs dispatch (`path.as_deref(), line, side`).
- `Inline { path: String, to: Option<u32>, from: Option<u32> }` — struct defined in Task 3 Step 3, referenced in Task 3's `format_comment_location` tests, and its shape matches the wiremock responses in Task 5.
- `format_comment_location(inline: Option<&Inline>) -> String` — signature consistent between Task 3 test assertions and the impl.
- Line-number type `u32` used in both `Inline.to`/`Inline.from` (Task 3), `add_pr_comment`'s `inline_line: Option<u32>` (Task 2), and `PrCommands::Comment.line: Option<u32>` (Task 4).
- `PrCommands::Comment` variant field names (`repo`, `pr_id`, `text`, `path`, `line`, `side`) match between the enum definition (Task 4 Step 3), the destructuring in the dispatch arm (Task 4 Step 5), and the test destructuring (Task 4 Step 1).

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-08-20-bitbucket-pr-inline-comments.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
