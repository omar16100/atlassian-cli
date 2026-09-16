# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Entries for 0.5.0 and later are itemised. Earlier releases are summarised: the
early history was rewritten during 0.5.0, so some original commits no longer
resolve. `git log` remains the complete record.

## [Unreleased]

## [0.9.2] - 2026-09-17

### Added

- This changelog, covering every release from 0.1.0 onwards. GitHub Releases now
  carry the matching section as their release notes: cargo-dist reads the
  version's section out of this file and passes it to `gh release create`, so no
  workflow change was needed (#142, closes #138).
- `AGENTS.md` at the repository root: workspace layout, the commands to run before
  committing, documentation conventions, and the release runbook, which was
  previously undocumented (#142).

### Fixed

- The internal path dependencies in `crates/cli/Cargo.toml` were still pinned to
  `0.9.0` after the 0.9.1 bump. They now track the workspace version (#142).
- `CONTRIBUTING.md` pointed at a `CLAUDE.md` that does not exist in this
  repository. It points at `AGENTS.md` now, and gained changelog and release
  sections (#142).

## [0.9.1] - 2026-09-12

### Changed

- Project site links retargeted to `atlassian-cli.pages.dev` across `Cargo.toml`,
  `README.md`, `SECURITY.md` and the docs (#139).

### Fixed

- The crates.io publish workflow is idempotent: it checks whether each crate
  version is already published and skips it, so a release that fails partway
  through can be rerun without manual surgery (#139).

## [0.9.0] - 2026-09-08

A hardening release, driven by 17 findings: 14 reported by a user, 3 found while
reviewing those. Several commands changed behaviour in ways that will be visible
in scripts.

### Added

- `bitbucket api` and `confluence api`: raw authenticated REST passthrough,
  matching the `jira api` command added in 0.5.0.
- A shared pagination primitive in `crates/api`, plus support for Jira's offset
  pagination.
- The list output envelope reports whether the result is complete, so a consumer
  can tell a full page from a truncated one.
- `--default-reviewers` on `bb pr create`.
- Reviewers are shown on `bb pr get`.
- Jira transitions are discoverable, and an issue can be moved by naming its
  destination status.
- 403 responses are enriched with the scopes the token actually carries.

### Changed

- Destructive Bitbucket commands share one confirmation standard: `repo delete`
  and the remaining unconfirmed deletes now prompt in the same way.
- `bb bulk archive-repos` no longer claims to archive repositories. It disables
  features, is named accordingly, and is gated behind an explicit flag.
- Help text and docs that understated what `bb bulk delete-branches` does were
  corrected.

### Fixed

- Lists that silently truncated now follow pagination: Bitbucket branch, repo,
  workspace, project, commit, webhook and ssh-key lists, and `jira project list`.
- `bb bulk delete-branches` could delete unmerged branches and, with a crafted
  ref name, the wrong ref entirely. Ref names are validated and path-encoded.
- Request paths are guarded at the client rather than at each call site, so a
  parameter cannot restructure the path it is interpolated into. The guard is
  derived from the URL parser rather than from a list of spellings.
- Bitbucket identifiers are percent-encoded rather than blacklisted, including
  every single-segment identifier and the commit revision.
- Two truncation-reporting bugs, and `--limit 0` is honoured.
- `-f`/`--format` is honoured on both `whoami` commands.
- `jira issue move --to-status` no longer guesses its way into terminal statuses.
- Bitbucket repository permission endpoints were wrong and have been corrected.

## [0.8.0] - 2026-09-07

### Added

- XDG-compliant config paths and a movable config directory via `--config-dir`,
  with migration from the legacy location and 0600/0700 permissions (#130).
- `--fields` on `jira issue get` and `jira issue search`: select fields by id or
  display name, rendered as ordered columns (#134).

### Fixed

- `auth whoami`, `auth test` and `auth status` dispatch the user-info endpoint on
  the profile's product instead of always asking Jira (#131).
- A base URL ending in `/wiki` no longer produces a doubled `/wiki/wiki` segment
  (#135).

## [0.7.2] - 2026-08-22

### Security

- `auth login --help` printed the API token passed on the command line. It no
  longer does (#126).

## [0.7.1] - 2026-08-22

### Fixed

- Joining a request path no longer discards a path component of the configured
  base URL, which broke installs served under a subpath (#125).

## [0.7.0] - 2026-08-22

### Added

- Inline pull request comments: `bb pr comment --path`, `--line` and `--side`,
  with a `location` column on `bb pr comments` (#121).
- `confluence` inline comments: list them and their thread replies (#123).

## [0.6.0] - 2026-08-20

### Added

- Reviewer approval status in `bb pr reviewers`, with `--all` (#103).
- Threaded Bitbucket PR comments (#109).
- `bb pr comment resolve` and `reopen` (#113).

### Changed

- `--profile` is a global flag, and `auth login` prompts for what it needs
  instead of failing (#117).

### Fixed

- Jira comments used a project-scoped route where an issue-scoped one was
  required; issue transitions and machine-readable empty lists were corrected in
  the same pass (#118).
- `bb pr reviewers --add` called the wrong endpoint (#104).
- Every example script now parses against the current CLI, with a regression test
  to keep it that way (#112).

### Security

- h2 bumped to 0.4.17 for RUSTSEC-2026-0258 (#116).

## [0.5.1] - 2026-08-10

### Fixed

- Documentation refresh for 0.5.0, a documentation index, and a repair of every
  broken example in the docs (#98).

## [0.5.0] - 2026-08-10

### Added

- `jira attachment`: list, get, download (single, to stdout, or bulk), upload and
  delete (#94).
- `jira api`: raw authenticated REST passthrough, with origin and redirect safety
  (#96).

### Removed

- The served website was removed from this repository and moved to its own
  (#88). Development docs stayed.

## [0.4.3] - 2026-07-13

### Added

- GFM tables in the markdown to ADF conversion used for Jira descriptions and
  comments.

### Fixed

- CSV output is quoted per RFC 4180, and newlines are escaped in markdown tables.
- Confluence attachment download URLs include the `/wiki` context path.
- Confluence comment timestamps are read from `version.createdAt`, footer
  comments missing `createdAt` are tolerated, and comment bodies are included.
- Tables nested inside list items and blockquotes degrade to paragraphs rather
  than producing invalid ADF.

### Security

- Vulnerable transitive dependencies bumped to clear the cargo-deny advisories.

## [0.4.2] - 2026-06-18

### Added

- `--sprint` on `jira issue create` and `update`, and the sprint is shown in
  `jira issue get` (#72).

## [0.4.1] - 2026-06-14

### Added

- Attachments are exposed in `jira issue get`, and `--full` on
  `jira issue comments list`.

### Fixed

- Attachment parsing tolerates the fields Jira omits, and markdown attachments
  are handled.

## [0.4.0] - 2026-06-12

### Added

- `confluence folder`: the v2 Folder API as a command group.
- Markdown is converted to ADF for Jira descriptions and comments.
- `--custom-pipeline` on `bb pipeline trigger`.

### Fixed

- Empty 2xx bodies are treated as success rather than as a parse failure, and
  Confluence list parsing was hardened.

### Security

- rustls-webpki and rand bumped to clear RustSec advisories. The unmaintained
  proc-macro-error2 was dropped by disabling tabled's default features.

## [0.3.3] - 2026-04-14

### Added

- Custom field support on `jira issue create`, `update` and bulk import, via
  `--field KEY=JSON_VALUE`, with discovery documentation. Collisions with the
  dedicated flags are rejected.

### Changed

- release-please was removed; releases are cut by cargo-dist alone.

### Security

- aws-lc-rs, aws-lc-sys and rustls-webpki bumped (RUSTSEC-2026-0049 among them).

## [0.3.2] - 2026-04-02

### Fixed

- Jira bulk search migrated to `/rest/api/3/search/jql` after the old endpoint
  was removed, with HTTP 410 handled explicitly.

## [0.3.1] - 2026-03-14

### Added

- `--timeout` and a `--log` mode on pipeline watch, a trigger column on steps,
  elapsed time, and scope-aware hints on 403 responses.

## [0.3.0] - 2026-03-14

### Added

- Bitbucket pipeline UX work: multi-remote detection, a `--pipeline` flag,
  `--wait`, `--on-complete` and `--envelope`.

## [0.2.9] - 2026-02-20

### Added

- Bitbucket bearer token authentication alongside app passwords, with
  deprecation notices on the older path.

## [0.2.8] - 2026-01-31

### Fixed

- Jira descriptions in ADF format are handled in `issue get` and `issue update`.

## [0.2.7] - 2026-01-31

### Added

- Pipeline variable and secret management commands.
- Automated crates.io publishing on release.

## [0.2.6] - 2026-01-31

### Fixed

- Authentication failures were silent on empty search results and on 403
  responses. They are reported now.

## [0.2.5] - 2026-01-27

### Changed

- `--output` was renamed to `--format`.

### Added

- Build number support in pipeline logs.

## [0.2.4] - 2026-01-25

### Fixed

- whoami updated to 2.0 and its changed `Result` API handled.

## [0.2.3] - 2026-01-19

### Changed

- UX improvements across the auth and pipeline commands.

## [0.2.2] - 2026-01-15

### Added

- JSM, Opsgenie and Bamboo command modules.

## [0.2.1] - 2025-12-26

### Fixed

- Confluence draft pages are published with the correct version.

### Changed

- A broad quality pass across the codebase.

## [0.2.0] - 2025-12-22

### Added

- `bb` as an alias for the `bitbucket` command group.

### Security

- Security hardening across all modules.

## [0.1.9] - 2025-12-17

### Added

- Pipeline enhancements: git context detection, a status command, rerun, and
  variables.

## [0.1.8] - 2025-12-15

### Added

- crates.io metadata on every crate, so the workspace can be published.
- A landing page for the project site.

## [0.1.7] - 2025-11-26

### Changed

- Keychain storage was removed in favour of file-only storage with 0600
  permissions.

## [0.1.6] - 2025-11-26

### Added

- `bitbucket whoami`, plus authentication improvements.

## [0.1.5] - 2025-11-26

### Fixed

- Keyring platform features enabled so tokens persist.

## [0.1.4] - 2025-11-26

### Fixed

- Bitbucket pipeline fixes.

## [0.1.3] - 2025-11-25

### Added

- A separate Bitbucket token, and authentication improvements (#3).

## [0.1.2] - 2025-11-20

### Added

- Query parameterisation for Jira and Confluence search (#2).

## [0.1.1] - 2025-11-20

### Added

- Multi-tier token lookup with an environment variable fallback (#1).
- Confluence integration tests and example scripts for all products.

## [0.1.0] - 2025-11-20

First release: a Rust CLI for Jira, Confluence and Bitbucket, distributed through
cargo-dist with a Homebrew tap and a shell installer.

[Unreleased]: https://github.com/omar16100/atlassian-cli/compare/v0.9.2...HEAD
[0.9.2]: https://github.com/omar16100/atlassian-cli/compare/v0.9.1...v0.9.2
[0.9.1]: https://github.com/omar16100/atlassian-cli/compare/v0.9.0...v0.9.1
[0.9.0]: https://github.com/omar16100/atlassian-cli/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/omar16100/atlassian-cli/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/omar16100/atlassian-cli/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/omar16100/atlassian-cli/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/omar16100/atlassian-cli/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/omar16100/atlassian-cli/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/omar16100/atlassian-cli/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/omar16100/atlassian-cli/compare/v0.4.3...v0.5.0
[0.4.3]: https://github.com/omar16100/atlassian-cli/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/omar16100/atlassian-cli/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/omar16100/atlassian-cli/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/omar16100/atlassian-cli/compare/v0.3.3...v0.4.0
[0.3.3]: https://github.com/omar16100/atlassian-cli/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/omar16100/atlassian-cli/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/omar16100/atlassian-cli/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/omar16100/atlassian-cli/compare/v0.2.9...v0.3.0
[0.2.9]: https://github.com/omar16100/atlassian-cli/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/omar16100/atlassian-cli/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/omar16100/atlassian-cli/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/omar16100/atlassian-cli/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/omar16100/atlassian-cli/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/omar16100/atlassian-cli/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/omar16100/atlassian-cli/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/omar16100/atlassian-cli/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/omar16100/atlassian-cli/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/omar16100/atlassian-cli/compare/v0.1.9...v0.2.0
[0.1.9]: https://github.com/omar16100/atlassian-cli/compare/v0.1.8...v0.1.9
[0.1.8]: https://github.com/omar16100/atlassian-cli/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/omar16100/atlassian-cli/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/omar16100/atlassian-cli/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/omar16100/atlassian-cli/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/omar16100/atlassian-cli/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/omar16100/atlassian-cli/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/omar16100/atlassian-cli/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/omar16100/atlassian-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/omar16100/atlassian-cli/releases/tag/v0.1.0
