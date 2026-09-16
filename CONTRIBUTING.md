# Contributing to atlassian-cli

## Quick Start

```bash
# Clone and build (automatically installs pre-commit hooks)
git clone https://github.com/omar16100/atlassian-cli
cd atlassian-cli
cargo build
```

## Pre-commit Hooks

Hooks run automatically on `git commit`:
- ✅ Code formatting (`cargo fmt`)
- ✅ Linting (`cargo clippy`)
- ✅ Unit tests

**If hooks fail:**
```bash
cargo fmt              # Fix formatting
cargo clippy --fix     # Fix linting
cargo test             # Run tests
```

**Bypass hooks (for WIP commits only):**
```bash
git commit --no-verify -m "WIP: work in progress"
```

## Local Testing

```bash
make pre-commit   # Run all checks
make quick-check  # Format + clippy only
make test         # Tests only
```

Or using `just`:
```bash
just pre-commit   # Run all checks
just quick-check  # Format + clippy only
just test         # Tests only
```

## CI Pipeline

All PRs must pass:
1. Format check
2. Clippy lints
3. All tests (Linux + macOS)

Runs in ~60-90 seconds (parallel jobs).

## Code Standards

- Keep files around 2000 LOC (per [AGENTS.md](AGENTS.md))
- Add unit tests for new features
- Update todo.md with changes
- Run `cargo fmt` before committing

## Changelog

Every user-facing change adds a bullet under `## [Unreleased]` in
[CHANGELOG.md](CHANGELOG.md), in the same PR, citing the PR number. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Leave out changes a user cannot observe: formatting, lint fixes, refactors with
no behaviour change. A dependency bump that closes an advisory goes under
`### Security` with the advisory ID.

The release workflow reads the section matching the version being released and
uses it as the GitHub Release body, so an empty section means a release with no
notes.

## Releasing

Maintainers only. The full runbook is in [AGENTS.md](AGENTS.md#releasing). In
short: promote the `[Unreleased]` section in `CHANGELOG.md`, bump the version in
the root `Cargo.toml` and the internal pins in `crates/cli/Cargo.toml`, merge to
`main`, then push a `vX.Y.Z` tag. The tag triggers the cargo-dist release and the
crates.io publish.
