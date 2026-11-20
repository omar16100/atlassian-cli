# Help: a heading for global options, and two aliases

Status: in progress on `feat/help-global-options-and-aliases` (PR #185).

## Problem

**Global options in each option list.** `--profile`, `--config`,
`--config-dir`, `--format`, `--envelope` and `--debug` are clap globals
(`crates/cli/src/main.rs`). clap printed them in the `Options:` section of each
command, mixed with the options of the command itself. A command with one
option of its own showed seven, so the reader had to find the one that
mattered. Callers ask for `--help` very often, so this cost is paid on a large
share of all runs.

**Singular names rejected.** Callers typed `jira issue comment` and
`jira issue link`. The subcommands are `comments` and `links`, and clap
rejected the singular forms.

**`confluence search content`.** One caller typed this. The subcommands of
`confluence search` are `cql`, `text`, `in-space` and `params`.

## What changed for users

- `--help` and `-h` print the global options below a `Global options:` heading,
  after the `Options:` section of the command. The top-level help has the same
  heading.
- `jira issue comment` is a visible alias of `jira issue comments`, and
  `jira issue link` is a visible alias of `jira issue links`. The help of
  `jira issue` shows them as `[alias: comment]` and `[alias: link]`.
- No option changed its name, its position rules or its meaning.

## Design notes

Each global argument has `help_heading = "Global options"`. clap prints
arguments that have a heading in a section of their own, after `Options:`, and
the heading goes with the argument into each subcommand.

`--debug` is the sixth global option. It is in the same section, because it has
the same property: it is accepted by each command.

The `auth` commands declare a `--profile` of their own, which replaces the
global one there. That option stays in the `Options:` section of those
commands. This is correct: it is their own option, with its own help text.

`--help` and `--version` stay in `Options:`. clap generates them, and they are
not globals of this program.

**Decision on `confluence search content`: no alias.** The reasons:

1. The intent is not known. `content` can mean a text search, or it can be a
   guess from the REST path `/content/search`, which takes a CQL query.
2. An alias to `text` would run a CQL string as a text search, and an alias to
   `cql` would reject plain text or search for the wrong thing. A wrong result
   with exit code 0 is worse than the current error.
3. The current error is immediate and correct: clap rejects `content` and
   prints the usage line of `confluence search`, and `confluence search --help`
   lists the four subcommands.

The two Jira aliases do not have this problem. The singular and the plural name
mean the same subcommand.

## Limitations

- A script that parses `--help` output sees the global options in a new
  section.
- The heading applies to the six globals of `main.rs`. `bitbucket` has two more
  group-level globals (`--workspace` and `--repo`), which stay in `Options:` of
  the Bitbucket commands. They are options of that product, not of the program.
- Other singular or plural names (for example `jira issue watcher`) have no
  alias. Only the two names that callers typed were added.

## Tests

- `crates/cli/src/main.rs`:
  `help_lists_global_options_below_their_own_heading` renders the long help of
  four commands from the clap tree and checks that each global is below the
  heading and not above it, that `Options:` comes first, that an option of the
  command stays in `Options:`, and that the `--profile` of `auth whoami` stays
  with the command. `jira_issue_comment_and_link_are_aliases` parses the
  singular and plural forms and checks that the help shows the aliases.
- `crates/cli/tests/cli_integration.rs` (built binary):
  `test_help_separates_global_options` and `test_jira_issue_singular_aliases`.
- Each of the two unit tests failed before the change, the first on the
  missing heading and the second on the rejected subcommand.
