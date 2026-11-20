# `bb pr list`: page size of 50 at most

Status: shipped in v0.14.0 (PR #191).

## Problem

`bb pr list --limit 0` and each `--limit` above 50 failed with
"Invalid pagelen". `--limit 50` and lower values passed. The owner saw this on
release 0.13.0.

`list_pull_requests` (`crates/cli/src/commands/bitbucket/pullrequests.rs`) sent
`pagelen` from `page_size(limit)`, which gives up to 100. That is the general
limit of Bitbucket. The Bitbucket API description says "The default value is 10
with 100 being the maximum allowed value. Individual APIs may enforce different
values." The pull request list has a lower limit of its own: 50.

## What changed for users

- `bb pr list --limit 0` returns all pull requests (within the request budget
  of the shared pagination driver).
- `bb pr list --limit N` with N above 50 returns up to N rows.
- A limit of 50 or less sends the same request as before.

## Design notes

`bitbucket/utils.rs` has `page_size_capped(limit, max)` and the constant
`PULL_REQUEST_MAX_PAGE_SIZE` (50). `page_size` calls it with the general limit
(100), so the other list commands send the same requests as before.

The rows after the first page come through the `next` link, which
`fetch_paged` already follows. No other code changed.

**Other Bitbucket list commands.** The task was to check them for the same
limit. The API description that is saved for this work does not give a limit
for each endpoint, and this work makes no request to a real site. So the limit
of the other endpoints is not known here. They are not changed. These are the
page sizes that the commands send, for a check on a real site:

| Command | `pagelen` sent |
| --- | --- |
| `bb pr list` | up to 50 (this change) |
| `bb repo list`, `bb branch list`, `bb commit list`, `bb workspace list`, `bb project list` | the limit, up to 100 |
| `bb pr comments`, `bb pr create --default-reviewers` (default reviewers) | 100 |
| `bb pipeline list`, `bb pipeline steps`, `bb pipeline logs` (steps) | 100 |
| `bb pipeline var list`, `bb pipeline env list` | 100 |
| `bb webhook list`, `bb ssh-key list`, `bb permission list` | 100 |
| `bb bulk` (repositories, branches) | 100 |

## Limitations

- The value 50 comes from the behaviour that the owner saw on a real site. The
  saved API description does not state it.
- If one of the other endpoints has a limit below 100, its command fails in the
  same way for a large limit. A check on a real site must find that.
- `--limit 0` stops at the request budget of the pagination driver (50
  requests), with a warning on stderr.

## Tests

- `crates/cli/src/commands/bitbucket/pr_list_page_size_tests.rs` (wiremock; the
  mock refuses each `pagelen` that is not the expected one with the
  "Invalid pagelen" answer): `limit_zero_asks_for_pages_of_50_and_follows_the_next_page`,
  `a_limit_above_50_asks_for_pages_of_50_and_follows_the_next_page` (51 and
  100), `a_limit_of_50_or_less_is_one_request_of_that_size` (50, 25, 1).
- `bitbucket/utils.rs`: `a_capped_page_size_stays_below_the_cap_of_the_endpoint`.
- The first two tests failed before the fix with "Invalid pagelen". The third
  passed before: it is a guard for the unchanged requests.
- No built-binary test sends this request, because the Bitbucket base URL is a
  constant. Not verified against a live Bitbucket site in this work.
