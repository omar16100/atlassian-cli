# README accuracy pass (plan)

Status: merged in PR #145 (docs only, no release needed).

## Context

A maintenance review on 27 Sep 2026 found README statements that no longer
matched the repository:

- Test counts from early development ("Total: 99 passing tests", "44 tests",
  and per-crate figures such as "Auth crate: 3 tests"). The workspace now runs
  912 tests.
- "Multi-platform builds (Linux, macOS, Windows)". CI tests on Ubuntu and macOS
  only, and the release workflow builds four macOS and Linux targets
  (`dist-workspace.toml`). Nothing builds or tests Windows.
- Opsgenie and Bamboo marked "Placeholder", although both command groups exist
  in `crates/cli/src/commands/opsgenie/` and `crates/cli/src/commands/bamboo/`
  with mocked-API integration tests.
- JSM described as "service desk and request operations" and listed as missing
  organizations and SLAs, both of which exist.
- "Package releases (binaries, Docker, Homebrew)" as a next step, although
  binaries ship on GitHub Releases and Homebrew through
  `omar16100/homebrew-atlassian-cli`.
- Eight em dashes, against the repository's writing style (`AGENTS.md`).
- Personal filesystem paths in `docs/14012026.md` and the root `todo.md`.

## Approach

- Replace the test counts with one dated measurement: `cargo test --workspace`
  on 27 Sep 2026 at commit `50bef34`, macOS: 912 passed, 0 failed, 1 ignored,
  with a per-target table. Drop the undated counts from the status checklist.
- Describe CI from `.github/workflows/` as it is: fmt and clippy on Ubuntu,
  tests on Ubuntu and macOS; `security.yml`; `release.yml` for macOS and Linux
  binaries.
  State that Windows is neither built nor tested, and list the release targets
  under "Pre-built Binaries".
- Describe the JSM, Opsgenie and Bamboo command groups by their actual
  subcommands (from each `mod.rs`), and point remaining work at `docs/todo.md`
  Phases 5 to 7. Next steps: JSM Insight / Assets (no such commands exist),
  remaining roadmap items, recipes, and a Docker image.
- Em dashes replaced with colons. No project-site or domain links were added
  or removed; the only new links point at `docs/todo.md` and the Homebrew tap
  repository.
- Bitbucket bulk commands (from codex review): the status checklist said
  "archive stale repos, delete merged branches", but `archive-repos` disables
  issues and wiki (no archive API) and `delete-branches` does not check merge
  status (`crates/cli/src/commands/bitbucket/mod.rs`, `BulkCommands`). Both the
  checklist and the examples now say so, and the examples drop the deprecated
  hidden `--dry-run`, since listing is the default.
- Personal paths replaced with `<codex-reviews>/`, `<local-plans>/` or the
  repository-relative path.

## Verification

- `grep -c` for U+2014 in `README.md`: 0.
- `git grep` for `/Users/` across the tree: no hits.
- `cargo test -p atlassian-cli --test docs_examples`: README command lines still
  parse.
- Every figure in the new Test Coverage section comes from the dated
  `cargo test --workspace` run named above.

## Found, not fixed here

- `jira bulk export` and `confluence bulk export` panic on every invocation:
  "Mismatch between definition and access of `format`". The subcommands define
  their own `format: String` argument, which clashes with the global
  `--format` (`OutputFormat`) read in `crates/cli/src/main.rs`. Reproduced at
  `50bef34` and with an older 0.2.8 binary. `docs_examples.rs` does not catch
  it because it only fails on exit code 2 (clap usage errors), and a panic
  exits 101. This is a code fix, out of scope for a docs-only change; the README
  examples at the Jira and Confluence bulk sections are unchanged.
