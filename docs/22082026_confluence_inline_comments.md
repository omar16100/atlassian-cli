# Confluence inline comments and threads (issue #122)

Status: shipped in 0.7.0 (PR #123).

## Problem

`confluence page comments <PAGE_ID>` only called
`GET /wiki/api/v2/pages/{id}/footer-comments`. Confluence keeps inline comments,
the highlight-and-comment kind attached to selected text, in a separate
collection, so a page using them returned an empty list even with dozens of
comments visible in the browser. The reporter had a page with 84 comments, none
of them footer comments.

Two further gaps came out of the same code path:

- **Pagination was ignored.** v2 paginates with an opaque cursor in
  `_links.next`, which the command never followed, so every listing was capped
  at the default page size. Even a footer-only page would have reported 25 of 84.
- **Comment text was not escaped.** `add-comment` interpolated the text straight
  into `<p>{}</p>`, so a comment containing `<`, `>` or `&` produced invalid
  storage XHTML.

## What changed

```
confluence page comments <PAGE_ID> [--full] [--replies]
confluence page add-comment <PAGE_ID> <TEXT> [--parent <ID>] [--kind footer|inline]
```

- Both collections are fetched and merged. A `kind` column says which a comment
  is, and doubles as the value to pass to `add-comment --kind` when replying.
- `_links.next` is followed to the end, with a 200-page safety cap (5,000
  comments at the v2 default) against a server that keeps handing back a cursor.
- `--replies` walks each thread via `/{collection}/{id}/children` and fills in a
  `parent` column. That is one request per thread root, so it is opt-in rather
  than the default.
- `--parent` replies into an existing thread. The reply is POSTed to the
  collection its parent lives in, because Confluence will not accept a footer
  reply to an inline thread. `--kind` selects it explicitly rather than the CLI
  guessing.
- Comment text is escaped for storage format.

## Not done

Creating a *new* inline comment. That needs
`inlineCommentProperties.textSelection` plus a match index identifying which
occurrence of the selected text to anchor to, which has no sensible
command-line spelling: the caller would have to know the exact text run and its
ordinal in the page body. Replying to an existing inline thread, which is the
common case, is supported.

## Tests

`crates/cli/tests/confluence_comments_e2e.rs` drives the built binary against a
mock, because the bug was invisible at transport level: the command fetched a
real endpoint successfully and returned an empty list. The tests assert on which
endpoints are called, not just that a call succeeded.

Covered: inline and footer merged into one listing; cursor pagination followed
to the end; `--replies` linking children to their root; replies *not* fetched
without the flag (asserted with `.expect(0)`); a reply posting to its parent's
collection and not the other one; a plain comment unchanged; and storage-format
escaping.

The first two were verified to fail when their fix is reverted.

## Untested

Everything here is mock-tested. I have no Confluence instance with inline
comments to run it against, so the exact v2 response shapes for
`inline-comments` and `/children` are taken from the API reference rather than
observed. The reporter offered to verify.
