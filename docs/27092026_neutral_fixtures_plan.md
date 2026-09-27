# Neutral test fixtures and current test counts (plan)

Status: merged in PR #149 (docs and test fixtures only, no release needed).

## Context

- Two `parse_git_remote` tests in `crates/cli/src/commands/bitbucket/git.rs`
  used a real employer-linked Bitbucket workspace and internal repository name
  as fixtures, and the root `todo.md` named two locally stored profiles after
  the same workspace. Neither belongs in a public tree.
- The README test coverage section was measured at `50bef34` (912 passed).
  PR #146 added 15 tests since then, so the figures were stale.

## Approach

- Fixtures now use `acmeteam/web_app` for both the HTTPS and SSH remote cases.
  The shape is unchanged: a plain lowercase workspace and a repository name
  with an underscore, with the `.git` suffix.
- `todo.md` says "both stored profiles" without naming them.
- A case-insensitive search of the tree for the employer-linked names returns
  no hits.
- README: one fresh `cargo test --workspace` run on 27 Sep 2026 at `e3dc5b1`
  (macOS, Rust 1.95.0): 927 passed, 0 failed, 1 ignored. Unit tests 424 to
  430 and integration tests 251 (22 files) to 260 (23 files, the new
  `bulk_export_e2e.rs`); the other rows and the per-product suite counts are
  unchanged.
- No CHANGELOG entry: nothing user-visible (AGENTS.md).

## Verification

- `cargo test --workspace --locked`: 927 passed, 0 failed, 1 ignored.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features
  -- -D warnings`.
- Pushes to `main` trigger only `ci.yml` and `security.yml`;
  `publish-crates.yml` runs on `v*` tags and `release.yml` on pull requests and
  version tags, so merging publishes nothing.
