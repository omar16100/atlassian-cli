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

## Limitations

- The request budget of 50 requests limits `--limit 0` to 12,500 items (50
  requests of 250). The warning says that the result is incomplete.
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

Unit tests in `crates/cli/src/commands/confluence/utils.rs` cover
`v2_page_size`.
