# Bitbucket: output keys in `--help`, and the full commit hash

Status: in progress on `docs/bb-json-output-keys` (PR #186).

## Problem

The `-f json` rows of `bb pr get`, `bb pr list`, `bb pipeline get`,
`bb pipeline list` and `bb pipeline steps` are flat. `source`, `destination`,
`author` and `state` are strings. The Bitbucket REST API returns objects for
the same names.

No help text listed the keys. Scripts that expected the API shape called
`.get()` on a string and failed. Other callers did not try the commands at all
and used raw `bb api` calls for pull requests and pipelines.

One value was not available at all: `commit` in the pipeline output has the
first 7 characters of the hash, and no key had the full hash.

## What changed for users

- `--help` (the long help) of the five commands lists the output keys of
  `-f json` and `-f yaml`: the name, the JSON type and a short note. A `*`
  marks a key that is not in each object.
- Each of these help texts says that the output is flat, and gives the
  `bitbucket api` path that returns the unchanged API response.
- `bb pipeline list`, `bb pipeline get`, `bb pipeline latest` and the final
  output of `bb pipeline watch` have a new key `commit_hash` in JSON and YAML:
  the full commit hash. `commit` is unchanged.
- `-h` (the short help) is unchanged: one line.
- No existing key changed its name, its type or its value.

## Design notes

**One table for each output.** `crates/cli/src/commands/bitbucket/output_keys.rs`
has a table of keys for each of the five outputs (and one for the entries of
`reviewers`). The long help is built from the table.

**The help cannot go out of date without a failure.** Tests serialise the real
row types and compare the keys with the table in the two directions: each key
of the output must be in the help, and each key of the help must be in the
output (optional keys only in a full row). To make this possible, the row
types of `pr list` and `pr get` moved from the inside of their functions to
module level (`PrListRow`, `PrView`), with builder functions. Their fields did
not change.

**`commit_hash` is a new key, not a longer `commit`.** A script that compares
`commit` with a 7 character value must continue to work.

**`commit_hash` is in JSON and YAML only.** The table, markdown and CSV
formats build their columns from the sorted keys of the rows. A new key
between `commit` and `created` would move the columns of a CSV that a script
reads by position, and would make the table wider. JSON and YAML readers use
names, so a new key does not disturb them.

**Line width.** clap wraps a help line that is wider than the terminal, and a
wrapped line breaks the columns of the key table. The notes are short, and a
test keeps each line of these help texts at 80 characters or less.

**No raw mode.** A `--raw` option on each command would be a second output
contract for each command. `bitbucket api <path>` already returns the
unchanged API response, so the help points to it.

## Limitations

- The key lists are in the help of these five commands only. The other
  commands (`bb pr comments`, `bb branch list`, the Jira and Confluence
  commands) have no list.
- The notes for `state` and `status` give examples, not a complete list.
  Bitbucket can add values.
- `commit_hash` is the only new key. The evidence named the full commit hash.
  Other values that callers got from raw API calls are not known here, so none
  was added.
- `bb pipeline status` has its own output type and is not in this change. Its
  `commit` has 7 characters and it has no `commit_hash`.
- With `--envelope`, the list is in `data`. The help of the three list commands
  says so in one line.

## Tests

- `crates/cli/src/commands/bitbucket/output_keys.rs`:
  `pr_list_help_lists_exactly_the_keys_of_a_row`,
  `pr_get_help_lists_exactly_the_keys_of_the_view` (with and without
  reviewers, and the keys of a reviewer entry),
  `each_help_text_lists_each_key_of_its_table`,
  `no_help_line_is_wider_than_the_limit`,
  `optional_keys_have_a_mark_and_a_legend`,
  `key_lines_are_aligned_and_have_no_trailing_space`.
- `crates/cli/src/commands/bitbucket/pipelines/rows.rs`:
  `pipeline_list_help_lists_exactly_the_keys_of_a_row`,
  `pipeline_get_help_lists_exactly_the_keys_of_the_view`,
  `pipeline_steps_help_lists_exactly_the_keys_of_a_step`,
  `the_full_commit_hash_is_in_json_and_yaml_only`.
- `crates/cli/tests/bb_output_keys_help_e2e.rs` (built binary): the key names
  in the help of each command, the reviewer keys, no wrapped table line, and
  the unchanged short help.
- The three pipeline key tests and the commit hash test failed before
  `commit_hash` existed. The pull request key tests passed at once, because
  they describe output that did not change. They are guards for the future.
- Not verified against a live Bitbucket site. The full hash is the `hash`
  value that the pipeline response already has in `target.commit`.
