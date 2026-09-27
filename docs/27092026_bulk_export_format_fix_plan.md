# Export commands' `--format` clash, and the orphaned `uv.lock` (plan)

Status: shipped in v0.9.3 (PR #146).

## Context

**Bug 1: the export commands panic.** Found during the README accuracy pass
(`docs/27092026_readme_accuracy_plan.md`, "Found, not fixed here").
`jira bulk export`, `jira audit export` and `confluence bulk export` each
declared their own `format: String` argument (default `json`). `main.rs`
declares a global `--format` (`OutputFormat`). clap keys arguments by id and does
not propagate a global into a subcommand that already has an argument with that
id, so in those three subcommands the local `String` replaced the global, and
building `Cli` from the matches panicked:

```
Mismatch between definition and access of `format`. Could not downcast to
atlassian_cli_output::OutputFormat, need to downcast to alloc::string::String
```

Reproduced on 27 Sep 2026 with a debug build of `e468dff` (all three commands,
with and without `--format`, exit 101 before any request) and with the 0.2.8
release binary from Homebrew (same panic). `git log` shows the cause arrived in
`c6e1800`, first released in 0.2.5, which made `--format` global.

Existing tests missed it: `docs_examples.rs` and `docs_examples_scripts.rs`
treated only exit code 2 (a clap usage error) as a failure, and a panic exits
101. `Command::debug_assert()` would not have caught it either (checked against
clap_builder 4.6.7): `_propagate_global_args` skips a subcommand that already
has the id, silently, so there is no duplicate for the asserts to see.

**Bug 2: Dependabot `uv` jobs fail on every run.** The repository root carries a
`uv.lock` (590 lines, a virtual `atlassian-cli` 1.0.0 package depending on
`atlassian-python-api`, `click`, `jira`, `rich`, `pytest`) with no
`pyproject.toml`. `git log --follow` shows one commit, the initial commit
`fceb227`; no Python source, `pyproject.toml` or requirements file has ever
been tracked, and nothing in `Makefile`, `justfile`, CI or the docs refers to it.
It is a leftover of a Python prototype. GitHub's dependency graph still parses
it: 13 open Dependabot alerts (pip: urllib3, requests, idna, soupsieve,
python-dotenv, pytest, Pygments) point at `uv.lock`, and each push to `main`
queues a "uv in /." security update per alert that fails, because there is no
project to update. `.github/dependabot.yml` has no `uv` or `pip` entry, so there
is nothing to remove there.

## Approach

**Bug 1.**

- Rename the subcommand argument to `--export-format` (id `export_format`), an
  optional `ExportFormat` value enum (`json`, `csv`, case-insensitive as the old
  string match was). A hidden `--format` alias is impossible: that long name
  belongs to the propagated global. The fallback below covers it instead: a
  pre-0.2.5 command line with `--format json` or `--format csv` writes the same
  file it used to.
- Resolution (`commands::common::resolve_export_format`): `--export-format`
  wins; otherwise a global `--format json` or `--format csv` picks the file
  format; anything else (the default table, yaml, markdown, quiet) writes JSON,
  the previous default. This keeps the documented `--format json` command lines
  (README, `docs/examples/confluence/backup-space.sh`) working, and makes
  `--output issues.csv --format csv` write CSV instead of JSON into a `.csv`
  file. The global `--format` keeps its job of shaping stdout where the command
  renders through it: `confluence bulk export` prints its summary with
  `render_success`, while the two Jira commands print fixed `println!` lines
  whatever the format (unchanged, noted under Limitations).
- The three identical `ExportFormat` enums in `jira/bulk.rs`, `jira/audit.rs`
  and `confluence/bulk.rs` become one in `commands/common.rs`.
- Guards, in `main.rs` (`cli_definition_tests`):
  - `Cli::command().debug_assert()` over the whole tree. It catches a reused
    long or short flag name (checked by temporarily declaring
    `#[arg(long = "format")] export_format`: "Long option names must be unique").
  - A walk over the unbuilt command tree that fails when a subcommand declares
    an argument with a global's id but a different value type
    (`ValueParser::type_id`). Before the fix it reported exactly the three
    export commands. Same-type reuse is deliberate and left alone: `auth`
    subcommands declare `--profile` and the `bb` subcommands a positional
    `repo`, and those values flow up to the globals.
  - A parse test of all three commands with and without the global format.
- `docs_examples.rs` and `docs_examples_scripts.rs` now also fail on exit code
  101, and report the first non-empty stderr line (a panic starts with a blank
  line). Before the fix they failed on the README's two export examples and the
  two `backup-space.sh` invocations.
- New `crates/cli/tests/bulk_export_e2e.rs`: the built binary against wiremock
  for all three commands. JSON by default, CSV via `--export-format`, CSV via a
  global `-f csv`, the old documented `--format json` lines, file and stdout
  formats kept apart (`--export-format csv --format json`), and an unknown value
  rejected as a usage error before any request.
- README and `backup-space.sh` examples use `--export-format`.
  `docs/c4model.md` documents `common.rs` and the global-argument rule, and its
  output-crate diagram says `--format` instead of the pre-0.2.5 `--output`.
- CHANGELOG, Unreleased: the flag change under Changed, the panic under Fixed.

**Bug 2.** Delete `uv.lock`. No `pyproject.toml` is added, because there is no
Python tooling to describe. No CHANGELOG entry: nothing a CLI user can observe,
and none of the 13 alerts concerns code this project ships.

## Limitations

- `jira bulk export` and `jira audit export` print their progress and summary
  with `println!`, and `confluence bulk export` prints a "Found N pages" line
  before its rendered summary, so `-f json` or `-f quiet` does not give clean
  machine-readable stdout for these commands. Pre-existing and left alone here.
- No paging: `jira bulk export` sends one search for up to 1000 issues, and
  `jira audit export` one request. Section 3 of
  `08092026_remaining_hardening.md` lists `audit` but not `bulk export`.
- The shadowing test compares value types only. Two same-typed arguments that
  differ in action (a flag against a value) would pass it.

## Verification

- Red before the fix: `cli_definition_tests` (2 of 3 failed, `debug_assert`
  passed as predicted), `docs_examples` (2 of 140 README commands), and
  `docs_examples_scripts` (2 invocations), all on the export commands.
- Green after: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --workspace --no-fail-fast` on macOS: 927 passed, 0 failed,
  1 ignored (912 on `e468dff`, plus 15 new: 3 definition, 3 resolver, 9 e2e).
- Manual: `jira bulk export --help` lists `--export-format` with
  `[possible values: json, csv]`; `--export-format xml` exits 2.
- After merge: the "Dependabot Updates" run list should show no new
  "uv in /." jobs, and the 13 pip alerts should close once `uv.lock` is gone
  from the default branch.
