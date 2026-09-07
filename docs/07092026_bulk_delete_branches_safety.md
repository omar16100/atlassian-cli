# `bb bulk`: honest naming and confirmation gates

Status: in progress on `feat/cli-feedback-remediation`.

## Problem

`bb bulk delete-branches` was advertised as "Delete merged branches". It never
checked merge status.

The entire selection rule was a name filter:

```rust
let protected = ["main", "master", "develop", "development"];
let is_protected = protected.contains(&branch.name.as_str())
    || exclude_patterns.iter().any(|p| branch.name.contains(p));
if !is_protected { /* DELETE */ }
```

Every branch that was not one of four hardcoded names, and not matched by
`--exclude`, was deleted regardless of whether it had ever been merged. Live
feature branches, release branches and long-running integration branches all
qualified. A user who read the help text and ran the command on a busy
repository would lose unmerged work.

The defect is present in released v0.7.2, verified with
`git show v0.7.2:crates/cli/src/commands/bitbucket/bulk.rs`. It is not a
regression on `main`.

It was found while planning a fix for a *different* defect: the same function
requests `?pagelen=100` and never follows Bitbucket's `next` link, so it only
ever saw the first 100 branches. That truncation bug was the only thing bounding
the blast radius. The original remediation plan proposed fixing pagination
first, which would have removed the accidental cap and let the command delete
every branch in the repository. Sequencing mattered more here than anywhere
else in that plan.

## Design notes

**The selection behaviour is unchanged.** Deriving a real merged set was
considered and rejected. Two approaches were available: cross-reference
`state=MERGED` pull requests, or ask Bitbucket whether each branch head is an
ancestor of the default branch. Both would have silently narrowed a command
people may already depend on, turning a data-loss bug into a
does-nothing-anymore bug for anyone using it as a name-based cleaner. The
decision was to make the command honest and hard to trigger by accident
instead.

What changed:

- `delete_merged_branches` is renamed `delete_branches`. The function name no
  longer asserts something the body does not do.
- The help text states that merge status is not checked and that unmerged
  branches are deleted. Both the short and long forms say so, because the short
  form is what appears in the parent command's listing.
- **Listing is the default.** `--execute` is required to delete anything.
- `--execute` requires typing the repository slug, or `--yes` to skip the
  prompt. The prompt goes to stderr so a piped `-f json` listing stays
  machine-readable.
- With no terminal and no `--yes`, the command **refuses** rather than
  proceeding or hanging. A scheduled job that never had a chance to answer must
  not delete.
- The confirmation happens after the branch list is fetched and filtered but
  before the first delete, so the count shown to the user is the count that will
  be deleted, and no partial deletion precedes the prompt.

**`--dry-run` is retained, hidden, and honoured as a veto.** It is redundant now
that listing is the default, but `execute && !dry_run` means an existing script
that passed it can never start deleting. Removing the flag would have broken
those invocations outright; ignoring it would have been worse, because the flag's
whole purpose was to prevent deletion.

The selection rule is extracted as `is_protected`, so it can be tested without
HTTP.

### A second deletion bug, found in review

Review of the change above surfaced a worse defect than the one it was fixing.

Git permits `#` in a branch name, and `#` begins a URL fragment. The branch name
was interpolated raw into the delete path, so for a branch called `main#old` the
client resolved `.../refs/branches/main#old` to the path
`/2.0/.../refs/branches/main` with `old` as a fragment, and fragments are never
transmitted. **Deleting `main#old` deleted `main`** — through the protected-name
check, which had correctly decided `main#old` was not protected, and while the
confirmation prompt displayed `main#old`.

Verified directly: `git check-ref-format --branch 'main#old'` accepts the name,
and resolving that path yields `/2.0/repositories/w/r/refs/branches/main`.

`encode_ref_path` in `bitbucket/utils.rs` percent-encodes everything outside the
RFC 3986 unreserved set. `/` is deliberately preserved, because `feature/login`
is an ordinary branch name and Bitbucket expects those slashes as real
separators. `%` is encoded, so a branch literally named `foo%23` cannot be
re-decoded by the server into a different ref.

The same defect was present in `bb branch delete`
(`bitbucket/branches.rs`), which is a single-branch delete with the same
consequence, and in `bb branch get`. All three sites now encode.

### Partial failures are reported

Under `--execute`, a failed delete used to abort with `?`, discarding the
accumulated rows. Failing on branch 5 of 40 left four branches deleted and
reported nowhere except a `tracing::info` invisible at default verbosity, with
an error naming only the branch that failed.

The loop now records the failure, breaks, renders what was already deleted, and
then returns the error. For a change whose purpose is preventing unexpected
data loss, the deletions that did happen are the most important thing to print.

## Limitations

- **Unmerged branches are still deleted** when the user asks for it. This is the
  chosen behaviour, not an oversight. The command is a name-based cleaner and
  now says so.
- Protection is exact for the four built-in names and substring-based for
  `--exclude` patterns. `maintenance` is not protected by the built-in `main`
  entry; `--exclude main` would protect it.
- `confirm_destructive`'s terminal-detection branch is not covered by a unit
  test. Exercising it would risk a hang when `cargo test` runs from a terminal,
  since stdin is inherited. The comparison logic is split into
  `confirmation_matches` and tested directly; the terminal branch is verified
  by hand.
- The pagination defect is untouched. The command still sees only the first 100
  branches. That is deliberate sequencing: the safety gate lands first, and
  pagination follows in a later step of the remediation plan.

## The same class, in the same file: `archive-repos`

`archive_stale_repos` reported `"archived"` and archived nothing. Bitbucket
Cloud has no repository archive API; the `PUT` set only `has_issues: false` and
`has_wiki: false`. The label named an operation that never happened, while the
operation that did happen — turning off two features, making any issues and wiki
pages filed in them inaccessible — went unnamed.

Treated the same way, for the same reason:

- Renamed to `disable_features_on_stale_repos`. The `archive-repos` subcommand
  name is kept, because renaming the CLI surface is a breaking change; its help
  text now opens with "Despite the command name, this does NOT archive".
- The reported action is `issues and wiki disabled`, not `archived`.
- Listing is the default; `--execute` applies; `--execute` needs the workspace
  typed back or `--yes`.
- Partial failures are rendered before the error, as above.

Two further bugs fixed while there:

- The empty message was a plain string containing a literal `{days_threshold}`,
  never a `format!`, so it printed the placeholder verbatim.
- Staleness is extracted as `is_stale`, which now treats a **missing
  `updated_on` as not stale**. The old inline logic did the same by accident,
  via nested `if let`, but nothing recorded the intent or tested it. Acting on
  an absent field would mutate repositories on no evidence.

Renaming the `archive-repos` subcommand to match what it does is a candidate
for the next breaking release.

## Tests

20 new tests. **814 pass across 29 suites, against a 794-test baseline on
`main`.** `cargo clippy --workspace --all-targets -- -D warnings` is clean.

Selection rules, in `bitbucket/bulk.rs` — fast, no HTTP:

- `missing_updated_on_is_never_stale`
- `staleness_compares_against_the_threshold`

- `protected_branches_are_never_selected`
- `exclude_patterns_match_as_substrings`
- `unmerged_feature_branches_are_still_selected` — pins the retained hazard, so
  the rename cannot later be mistaken for merge detection by someone reading
  only the function name
- `protection_is_exact_for_names_and_substring_for_patterns`

The safety gate, in `bitbucket/bulk.rs` — over a `wiremock` server, following
the existing pattern in `bitbucket/pullrequests.rs`:

- `listing_is_the_default_and_deletes_nothing` — the one that matters. Asserts
  zero `DELETE` requests are issued without `--execute`.
- `execute_with_yes_deletes_only_unprotected_branches`
- `exclude_patterns_are_honoured_under_execute`
- `a_hash_in_a_branch_name_cannot_delete_the_protected_ref`
- `stale_repo_listing_mutates_nothing` — the `archive-repos` equivalent: no
  `PUT` without `--execute`
- `execute_disables_features_only_on_stale_repos`

Path encoding, in `bitbucket/utils.rs`:

- `hash_in_a_branch_name_is_encoded`
- `slashes_are_preserved_as_path_separators`
- `percent_is_encoded_so_the_server_cannot_re_decode_it`
- `ordinary_names_are_unchanged`
- `non_ascii_is_utf8_percent_encoded`
- `encoded_path_resolves_to_the_intended_ref` — guards the consequence rather
  than the string transform, by resolving the path and asserting the target ref

Confirmation, in `commands/common.rs`:

- `confirmation_requires_the_exact_resource_name` — a generic "yes" must not
  stand in for naming the resource
- `empty_expectation_never_confirms` — an empty expectation must not
  auto-confirm, or a caller that forgot to pass a name would skip the gate

### Both critical tests were verified to fail against the unfixed code

A test that cannot fail proves nothing, so each was checked against a
deliberately reverted tree:

- Replacing `encode_ref_path(&branch.name)` with `branch.name` makes
  `a_hash_in_a_branch_name_cannot_delete_the_protected_ref` fail — it catches
  the real deletion of `main`.
- Replacing `if execute` with `if true` in the delete loop makes
  `listing_is_the_default_and_deletes_nothing` fail.

Both pass again once restored.

Help output was checked against the built binary rather than inferred from the
source.
