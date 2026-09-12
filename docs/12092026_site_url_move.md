# Project site moved to a new URL — 12 September 2026

Status: in progress on `fix/site-url-move`, releases in 0.9.1.

## Problem

The project site has moved to **`https://atlassian-cli.pages.dev`**. The previous address is being
retired and will no longer be under this project's control after **24 September 2026**, so every
link in this repository that pointed at it had to be retargeted before that date.

The `Cargo.toml` `homepage` field is the one that matters most. Crate metadata is immutable once
published, so the 0.9.0 listing on crates.io and docs.rs would keep advertising an address the
project no longer controls, indefinitely, unless a new version supersedes it. That is what 0.9.1 is
for.

## Design notes

Changed:

| File | Change |
| --- | --- |
| `Cargo.toml` | `homepage` to the new host; workspace `version` to 0.9.1. All six crates inherit both via `version.workspace = true` and `homepage.workspace = true`. |
| `README.md` | Six documentation links retargeted. |
| `SECURITY.md` | The sentence naming the website in the credentials-handling statement. |
| `docs/index.md` | The pointer to the separately maintained site repository. |
| `.github/workflows/publish-crates.yml` | Made the crates.io publish idempotent, see below. |

Deliberately **not** changed: the historical entries in `todo.md` and `docs/14012026.md`. Those are a
record of work done at the time, not live links, and rewriting them would falsify the log.

The product name, the crate names and the repository name are unchanged. This change is about URLs
only.

### The publish workflow had to be fixed first

`publish-crates.yml` published the six crates in six separate steps with a blind `sleep 30` between
each. If any crate failed partway, the rerun died immediately on the first crate with "crate version
already uploaded", and the release could not be completed without hand-editing the workflow. Given
this release has a fixed deadline, that failure mode was unacceptable.

Each crate is now skipped if that exact version already exists on crates.io, and index propagation
is polled rather than slept, so a rerun after a partial release resumes where it stopped.

## Limitations

- Publishing 0.9.1 is what actually fixes crates.io. Until the `v0.9.1` tag is pushed, the live
  listing still shows the old address.
- `*.pages.dev` carries no domain-level search authority and cannot host email. It is a working
  home, not a strong one. Moving off it later would be another migration, though redirects from it
  would be under this project's control and could be held indefinitely.
- Already-published artifacts cannot be corrected: 0.9.0 and earlier on crates.io, and the generated
  docs.rs pages for them, keep the old homepage.
- External links this project does not control will keep pointing at the old address.

## Tests

- `cargo check --workspace` passes; `cargo test --workspace` passes, 33 suites, 0 failures.
- `grep -rn "atlassiancli\.com"` over the tree, excluding `todo.md` and `docs/14012026.md`, returns
  nothing.
- Publish workflow: YAML parses, the crates.io probe correctly returns 200 for a published version
  and 404 for an unpublished one, and a dry simulation of the loop skips every already-published
  crate and selects every missing one.
- The new host was verified live before these links were changed: 101 of 101 sitemap URLs return
  200, canonicals are self-referential, and the site's Cloudflare Pages Functions still serve
  markdown content negotiation on `/` and `/ja/`.
