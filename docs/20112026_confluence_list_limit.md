# Confluence `space list`, `page list` and `blog list`: `--limit` is a total

Status: in progress on fix/confluence-space-list-limit (PR #PR)

## Problem

`confluence space list --limit 300` failed. The command sent `--limit` as the
`limit` query parameter of one request to `GET /wiki/api/v2/spaces`. The server
refused the value because it was greater than the maximum that the endpoint
permits.

The Confluence Cloud REST API v2 specification gives the same range for the
`limit` parameter of `GET /spaces`, `GET /pages` and `GET /blogposts`:
`"default": 25, "minimum": 1, "maximum": 250`. Source:
<https://dac-static.atlassian.com/cloud/confluence/openapi-v2.v3.json>.

An audit of the Confluence list commands with a `--limit` option found the
same cause in three commands:

| Command | Before | Now |
| --- | --- | --- |
| `confluence space list` | value sent unchanged, one request | corrected |
| `confluence page list` | value sent unchanged, one request | corrected |
| `confluence blog list` | value sent unchanged, one request | corrected |
| `confluence search cql`, `text`, `in-space`, `params` | page size clamped to 250, next pages followed (0.13.0) | no change |

`confluence bulk` commands have no `--limit` option. They use a fixed search
limit and are not part of this change.

The three commands also read only one server page. A limit of 250 or less
returned the correct number, but no command could return more than 250 items.
`--limit 0` sent `limit=0`, which is below the minimum of 1.

## Design notes

- **`--limit` is a total.** This is the convention of `confluence search` and of
  the Bitbucket list commands (`bitbucket/utils.rs` `page_size`).
- **Clamp.** `v2_page_size` in `confluence/utils.rs` gives the page size of the
  first request: the limit, or 250 when the limit is larger or is 0.
- **Next pages.** `fetch_v2_list` calls the shared driver `fetch_paged` with
  `ConfluencePage<T>`. The driver follows `_links.next` until it has the total,
  the collection ends, or the request budget (50 requests) is used.
- **`--limit 0` means all**, within the request budget.
- **No flag.** The total is 25, which is the server default. The command sends
  `limit=25` and makes one request, as before.
- **Warning.** A result that stops before the end of the collection writes one
  line to stderr:
  `warning: showing N spaces; more exist. Raise --limit, or use --limit 0 for all.`
  When the request budget stops the run, the line says
  `pagination stopped before completion`. Stdout does not change, so a piped
  `-f json` result stays valid.
- **Output keys.** The rows and their keys do not change.

What a script can see:

- A run without `--limit` now sends `limit=25` in the request, and writes the
  warning to stderr when the site has more than 25 items.
- `--limit 0` returned an error before. It now returns all items.
- A limit above 250 returned an error before. It now returns up to that total.
- An explicit small limit (for example `--limit 10`) also writes the warning to
  stderr when more rows exist. Before, it wrote nothing.
- `blog list` gives an empty `title` or `status` for a blog post where the
  server gives null or no value. Before, such an item made the command fail.

## Limitations

- The first request has the page size and the filters (`type`, `space-id`).
  Each next request uses the `_links.next` link of the server unchanged. The
  CLI adds no `limit` and no filter to it. The specification describes the
  link as "the relative URL for the next set of results, using a cursor query
  parameter", and does not say that the link keeps `limit` or the filters. So
  later pages have the size and the filters that the server puts in the link.
  It is not verified on a live site that the cursor keeps the filters.
- A link with no `limit` gives pages of the server default, 25. `--limit 0`
  then stops at 50 requests of that page size, with the warning that the
  result is incomplete. With pages of 250 the request budget of 50 requests
  gives 12,500 items at most.
- The CLI does not add a missing `limit` to the link. That needs a change in
  the shared pagination driver or a second loop, and the effect of a changed
  cursor link on a live site is not known.
- `confluence search` has its own default of 25 and its own clamp to 250 in
  `confluence/search.rs`. It uses the v1 search endpoint, and the constants in
  `confluence/utils.rs` are the values of the v2 specification, so search does
  not use them.
- `page versions`, `page comments` and `attachment list` have no `--limit`
  option and are not changed. `page comments` follows the next pages with its
  own loop. `page versions` and `attachment list` read one server page.
- The behaviour is verified against a mock server and against the public REST
  specification. It is not verified against a live Confluence site.

## Tests

`crates/cli/tests/confluence_list_limit_e2e.rs` runs the built binary against
`wiremock`:

- `space_list_above_the_maximum_clamps_and_follows_next`: `--limit 251` sends
  `limit=250`, then follows `next`. Two requests, 251 rows.
- `space_list_small_limit_is_one_request`: `--limit 5` with `--space-type`.
- `space_list_warns_when_the_limit_cuts_the_result`.
- `space_list_limit_zero_returns_all`.
- `space_list_without_a_limit_asks_for_one_page_of_25`.
- `page_list_above_the_maximum_clamps_and_follows_next`.
- `blog_list_above_the_maximum_clamps_and_follows_next`.
- `space_list_with_a_type_filter_follows_the_next_link_unchanged`: asserts the
  query of the second request.
- `page_list_with_a_space_follows_the_next_link_unchanged`: asserts the query
  of the second request.
- `page_list_with_a_small_limit_warns_when_more_exist`.
- `blog_list_accepts_a_null_title_on_a_followed_page`.

Unit tests in `crates/cli/src/commands/confluence/utils.rs` cover
`v2_page_size`.
