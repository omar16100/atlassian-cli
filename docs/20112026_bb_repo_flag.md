# `--repo` and the Git remote on all Bitbucket repository commands

Status: in progress on fix/bb-repo-flag (PR #PR)

## Problem

Callers passed `--repo` to pull request, commit and branch commands, and clap
rejected it:

```text
$ atlassian-cli bb pr get --repo web_app 42
error: unexpected argument '--repo' found
```

`--repo` is a global option of `bb`, and the pipeline commands accept it since
PR #153. The other command groups declared a required positional argument with
the clap id `repo`. clap does not copy a global option into a subcommand that
has an argument with the same id. Thus the flag did not exist on those
commands. `bb --repo web_app pr get 42` failed too: clap read `42` as the
repository and then asked for `<PR_ID>`.

## What changed for users

The repository of each command below can be the first argument (as before),
`--repo` (before or after the subcommand), or the Git remote of the current
directory.

| Group | Argument, `--repo` or Git remote | Argument or `--repo` only |
| --- | --- | --- |
| `bb repo` | `get`, `update` | `create`, `delete` |
| `bb branch` | `list`, `get`, `create`, `protect`, `restrictions` | `delete`, `unprotect` |
| `bb pr` | `list`, `get`, `create`, `update`, `merge`, `decline`, `approve`, `unapprove`, `diff`, `comments`, `comment`, `resolve-comment`, `reopen-comment`, `reviewers` | |
| `bb webhook` | `list`, `create` | `delete` |
| `bb ssh-key` | `list`, `add` | `delete` |
| `bb permission` | `list`, `grant` | `revoke` |
| `bb commit` | `list`, `get`, `diff`, `browse` | |
| `bb bulk` | | `delete-branches` |

Examples, all with the same result:

```bash
atlassian-cli bb pr get web_app 42
atlassian-cli bb pr get --repo web_app 42
atlassian-cli bb --repo web_app pr get 42
atlassian-cli bb pr get 42            # in a checkout of web_app
```

The rules are those of the pipeline commands:

- The order of precedence is the argument, then `--repo`, then the Git remote.
- An argument that disagrees with `--repo` is an error:
  `Repository given twice: 'web_app' as an argument and 'other_repo' with --repo.`
- No repository from any source is the error `Repository required for '<command>'.`

## Design notes

**One resolver.** `positional::choose_repo` and `require_repo` did the work for
the pipeline commands. `RepoResolver` in `bitbucket/mod.rs` holds the `--repo`
value and the Git remote, and calls those two functions for each command. The
pipeline commands use the same `RepoResolver::any`.

**Count decides.** A command such as `pr get` takes `[REPO] <PR_ID>` as one
list of one or two values. The arguments after the repository are all
required. Thus one value more than the required count means that the first
value is the repository. `positional::split_repo_and_required` does this. It
does not look at the shape of a value, so a branch named `web_app` and a
repository with a numeric slug are not a problem. The pipeline commands keep
`split_leading_repo`, because their identifiers are optional.

**The clap id is not `repo`.** The arguments are `repo_arg` (commands that take
only a repository) and `targets` (commands that take more). The global `--repo`
is thus available on each command.

**Numbers are read after the split.** `PR_ID`, `COMMENT_ID` and
`RESTRICTION_ID` come out of the list as text. The dispatch reads them as
numbers before it looks for a repository. `bb pr get web_app` gives
`Invalid PR_ID 'web_app': expected a number. Arguments: [REPO] <PR_ID>.` and
not an error about a missing repository.

**Usage lines.** clap cannot show that only the first value of a list is
optional. Each command with a `targets` list has an `override_usage`, for
example `atlassian-cli bitbucket pr get [OPTIONS] [REPO] <PR_ID>`. A test
compares each override with the command path and the required options.

**Commands that delete do not read the Git remote.** `branch delete`,
`branch unprotect`, `webhook delete`, `ssh-key delete`, `permission revoke`,
`repo delete` and `bulk delete-branches` accept the argument and `--repo`, but
not the Git remote. Before this change they needed an explicit repository, and
four of them have no confirmation prompt. With the Git remote,
`bb webhook delete <uuid>` in the wrong directory would delete from the wrong
repository. The error names the repository of the Git remote, so the user can
add `--repo`. `repo create` has the same rule for a different reason: the slug
of a new repository must not come from an existing checkout.

`pr merge` and `pr decline` do read the Git remote. They change state but
delete nothing, and the pipeline commands `stop` and `trigger` set the
precedent.

**Testable dispatch.** `execute` reads the Git context and calls `execute_in`
with it. A test can thus give a Git context. A test cannot control the remotes
of the directory that `cargo test` runs in.

## Limitations

- A missing repository is now the error `Repository required for '<command>'`
  from the command (exit code 1), not a clap usage error (exit code 2). The
  same is true for an ID that is not a number, and for a list with too few
  values when clap accepts the count.
- The `Arguments` section of `--help` shows `[REPO] [PR_ID]...` for a list, as
  it does for the pipeline commands. The usage line above it and the
  description are correct. A command with no value at all gives
  `Missing argument: expected [REPO] <PR_ID>.` with exit code 1.
- In a checkout, `bb branch get web_app` reads `web_app` as a branch name of
  the repository in the Git remote. Before, it was an error for a missing
  argument.
- The workspace still comes from `--workspace`, then the Git remote, then the
  profile. This change does not alter that order.
- Bitbucket commands always call `https://api.bitbucket.org`, so no test of the
  built binary can send a Bitbucket request to a mock server. The tests of the
  built binary cover the paths that stop before a request. The in-process
  tests cover the requests.
- Request paths of read commands (for example `pr get`) do not encode the
  repository slug. That is not new and this change does not alter it.

## Tests

- `crates/cli/src/commands/bitbucket/repo_argument_tests.rs`: for each command
  in the table, a command line goes through clap and the dispatch to a
  `wiremock` server. The tests check the request path for the argument form,
  `--repo` in three positions, and the Git remote. They also check the errors:
  no repository, a repository given twice, a command that deletes with only a
  Git remote, a slug where a number must be, and a wrong count of values.
- `crates/cli/src/commands/bitbucket/positional.rs`: unit tests of
  `split_repo_and_required`, and clap parse tests of each shape of
  `bb pr get`.
- `crates/cli/tests/bb_repo_flag_e2e.rs`: the built binary, in a directory
  with and without a Bitbucket Git remote, for the paths that send no request.
