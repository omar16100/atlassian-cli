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

- Keep files around 2000 LOC (per CLAUDE.md)
- Add unit tests for new features
- Update todo.md with changes
- Run `cargo fmt` before committing
