# AGENTS.md

Operating instructions for coding agents working in this repository. Humans
should read `CONTRIBUTING.md` first; this file covers the conventions an agent
cannot infer from the code.

## What this project is

`atlassian-cli` is a unified command line client for Atlassian Cloud: Jira,
Confluence, Bitbucket, JSM, Opsgenie and Bamboo.

It is an independent, unofficial open source project. It is not affiliated with,
endorsed by, or sponsored by Atlassian. Never write anything that implies
otherwise, in code, help text, documentation or commit messages.

Never invent facts. No fabricated statistics, benchmark numbers, download
counts, testimonials or compliance claims. If a number cannot be traced to a
source in this repository or to a cited external one, leave it out.

## Workspace layout

Six crates, one shared version (`[workspace.package] version` in the root
`Cargo.toml`, with `shared-version = true`).

| Crate | Path | Owns |
| --- | --- | --- |
| `atlassian-cli` | `crates/cli` | The binary: clap command tree, argument parsing, command handlers |
| `atlassian-cli-api` | `crates/api` | HTTP client, request path construction and guards, pagination, product API clients |
| `atlassian-cli-auth` | `crates/auth` | Profiles, credential storage, token resolution, scope handling |
| `atlassian-cli-config` | `crates/config` | Config file discovery (XDG), base URL normalisation, permissions |
| `atlassian-cli-output` | `crates/output` | Table, JSON, YAML, CSV and markdown rendering, the list envelope |
| `atlassian-cli-bulk` | `crates/bulk` | Bulk operation execution and confirmation flow |

`crates/cli/Cargo.toml` pins the five internal crates by path and by version.
Those version pins have to move with the workspace version.

## Commands

```bash
make pre-commit    # fmt + clippy + test, run this before every commit
make quick-check   # fmt + clippy only
make test          # cargo test
make install       # cargo install --path crates/cli
```

CI runs `cargo fmt --all -- --check`,
`cargo clippy --all-targets --all-features -- -D warnings` and
`cargo test --all --no-fail-fast` on Linux and macOS, plus `cargo audit` and
`cargo deny`. The cargo-husky pre-commit hook runs the same fmt and clippy
invocations, but only `cargo test --lib --bins`, so run `make test` yourself
before pushing anything that touches an integration test.

## Code standards

- Keep files around 2000 lines. Split a module rather than letting it grow past
  that.
- `snake_case` for identifiers, modules and file names.
- Every new behaviour ships with unit tests that exercise the behaviour, not the
  scaffolding. Tests must be fast; HTTP is mocked with `wiremock`.
- Log through `tracing`, at a level that makes a failed run diagnosable.
- Anything that deletes, archives or overwrites requires explicit confirmation
  and a dry run by default. See the existing confirmation flow in `crates/bulk`
  before adding another one.
- Never interpolate a caller-supplied value into a request path without the
  encoding and guard helpers in `crates/api`.

## Documentation

- Read `docs/index.md` first. It indexes every document and records the
  conventions.
- `docs/c4model.md` is the source of truth for architecture. Read it before an
  architectural change and update it in the same pull request: containers,
  components, services, dependencies, data flows.
- Dated documents are `docs/DDMMYYYY_topic.md` and open with a status line
  (`shipped in vX.Y.Z (PR #N)`). Evergreen documents are `docs/topic.md`. Add a
  row to `docs/index.md` for any new document.
- The repository-root `todo.md` is the running log of changes, newest last.
  `docs/todo.md` is the forward-looking roadmap. They are different files.

## Changelog

`CHANGELOG.md` at the repository root follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Every user-facing change adds a bullet under `## [Unreleased]`, in the same pull
request that makes the change. A bullet describes what changed for someone using
the CLI, and cites the pull request number.

Leave out anything a user cannot observe: formatting, lint fixes, refactors with
no behaviour change, dependency bumps that close nothing. A dependency bump that
closes an advisory goes under `### Security` with the advisory ID.

This file is not decoration. cargo-dist reads the section matching the version
being released and uses it as the GitHub Release body, so an empty section means
a release with no notes.

## Releasing

Releases are cut by cargo-dist (`dist-workspace.toml`,
`.github/workflows/release.yml`) and published to crates.io by
`.github/workflows/publish-crates.yml`. Pushing a `vX.Y.Z` tag fires both. Use
that exact form: `release.yml` matches any semver-like tag, but
`publish-crates.yml` matches `v*` only, so a tag without the `v` would build the
release and silently skip crates.io.

1. Promote the changelog. Replace `## [Unreleased]` content with a new
   `## [X.Y.Z] - YYYY-MM-DD` section, leave an empty `## [Unreleased]` above it,
   and add the compare link at the bottom of the file.
2. Bump the version in two places: `[workspace.package] version` in the root
   `Cargo.toml`, and the five internal dependency pins in
   `crates/cli/Cargo.toml`.
3. Run `cargo check --workspace` so `Cargo.lock` is updated in the same commit.
4. Run `make pre-commit`. `crates/cli/tests/cli_integration.rs` asserts the
   reported version, so it covers the bump.
5. Confirm the release notes before tagging: `dist plan --output-format=json`
   and check that `announcement_github_body` contains the new changelog section.
6. Open a pull request, get CI green, squash merge to `main`.
7. Tag the merge commit on `main` and push the tag:

   ```bash
   git tag -a vX.Y.Z -m "Release X.Y.Z"
   git push origin vX.Y.Z
   ```

8. The tag fires both workflows: cargo-dist builds four targets, creates the
   GitHub Release with the changelog section as its body, and pushes the updated
   formula to `omar16100/homebrew-atlassian-cli`; the crates.io job publishes the
   six crates in dependency order and is idempotent, so it can be rerun.
9. Verify: `gh release view vX.Y.Z` shows the changelog, and the new version
   appears on crates.io.

## Git

- Never commit to `main`. Branch, then open a pull request.
- Conventional commit subjects: `feat(scope):`, `fix(scope):`, `docs:`, `chore:`,
  `ci:`, `test:`.
- Squash merge by default.

## Writing style

No em dashes. Use commas, colons, parentheses or a second sentence. This applies
to code comments, help text, documentation, changelog entries and commit
messages.
