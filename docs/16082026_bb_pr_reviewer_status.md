# Bitbucket PR reviewer status and reviewer updates

Status: implemented on branch `fix/bb-pr-reviewers-followup`, on top of PR #103 (merged).

## Problem

`bb pr reviewers <repo> <pr_id>` only ever added reviewers. Two defects followed from that:

1. With no `--add`, the command iterated an empty list and then printed
   `✅ Reviewers added to pull request #N`. A no-op reporting success. There was no way to
   see who the reviewers were or whether they had approved, so scripts had to run
   `pr get --format json | jq '.participants[]'` (issue #102).
2. With `--add`, the command PUT
   `/2.0/repositories/{ws}/{repo}/pullrequests/{id}/default-reviewers/{uuid}`. Bitbucket
   Cloud has no such endpoint: repo-level default reviewers live at
   `/2.0/repositories/{ws}/{repo}/default-reviewers/{user}`, and a pull request's reviewer
   list is replaced by a `PUT` on the pull request itself. Every `--add` was a 404. This is
   the same class of defect as issue #100 (`jira issue comments update`).

## Design notes

**Listing.** The no-`--add` invocation now lists participants
(`crates/cli/src/commands/bitbucket/pullrequests.rs::list_pr_reviewers`). Participants are
embedded in the pull request resource, so the existing PR GET is reused and no second call is
made. Default output is `role == REVIEWER` only; `--all` adds `PARTICIPANT` rows
(commenters and other interactors).

`status` is derived, not returned by the API:

| Participant fields | Status |
| --- | --- |
| `state == "approved"`, or `approved == true` with no `state` | `Approved` |
| `state == "changes_requested"` | `Changes Requested` |
| anything else | `No Response` |

The `approved`-with-no-`state` arm exists because older responses set the boolean without the
string. Filtering and row construction live in a pure `reviewer_rows(&[Participant], bool)`
so they are unit-testable; `list_pr_reviewers` is left with I/O and rendering, matching
`list_pull_requests`.

**Adding.** `add_pr_reviewers` now reads the PR, unions the existing reviewer UUIDs with the
requested ones, and PUTs `{"title": <unchanged>, "reviewers": [{"uuid": …}]}` back. The PR
PUT takes the pull request to update; `title` is sent unchanged rather than omitted, since it
is the one field documented as required when creating a PR and echoing it back cannot change
anything. Reading first is what makes `--add` additive rather than destructive, since the
submitted `reviewers` array replaces the whole list.

UUIDs are normalised to Bitbucket's brace form by `normalize_uuid`, so `--add abc-123` and
`--add '{abc-123}'` behave the same. `create_pull_request --reviewers` was passing raw input
straight through and had the same bare-UUID problem, so it now shares the helper.

`--all` is `conflicts_with = "add"`: it only affects listing, and silently ignoring it on the
add path was how the original no-op bug read to users.

## Limitations

- Reviewers can only be added, not removed. Removal means sending a shorter list on the same
  PUT; no flag exposes it yet.
- The reviewer union is last-write-wins against whatever the PR looked like at GET time. Two
  concurrent `--add` calls can drop one of the additions.
- A participant whose `state` is some value Bitbucket adds later renders as `No Response`
  rather than the raw string.
- `--add` takes account UUIDs only, not usernames or emails.

## Tests

- 12 unit tests in `crates/cli/src/commands/bitbucket/pullrequests.rs`: status derivation
  (four arms), REVIEWER-only vs `--all` filtering, empty participants, participants-without-
  reviewers, empty `participated_on` rendering as a blank cell, UUID normalisation, and
  union/dedupe including empty entries.
- `crates/cli/tests/bitbucket_integration.rs`: the participants payload shape, and
  `test_bitbucket_add_pull_request_reviewers`, which pins the PUT path and body. Like every
  test in that file it drives `ApiClient` directly rather than the command, so it documents
  the wire format but would not catch `add_pr_reviewers` calling a different URL.
- Not covered by tests: the live Bitbucket contract. Nothing here would have caught the
  original wrong endpoint either, since a mock serves whatever path it is given. The `--add`
  path needs a run against a real workspace before it is trusted.
