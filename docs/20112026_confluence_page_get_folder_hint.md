# `confluence page get`: a 404 hint that names the cause

Status: shipped in v0.14.0 (PR #192).

## Problem

`confluence page get <id>` with the ID of a folder failed with these two lines:

```
Hint: Check if the resource ID is correct
Error: Failed to get page <id>: Resource not found: /wiki/api/v2/pages/<id>
```

The ID was correct. It was the ID of a folder, and folders have their own
command (`confluence folder get <id>`). The hint sent the caller to check an
ID that had no fault.

A 404 from `GET /wiki/api/v2/pages/{id}` has three probable causes: the ID is
wrong, the profile has no permission to view the page, or the ID belongs to
content that is not a page. The response does not say which one applies.

## Design notes

- **One probe, only after a 404.** When the page GET returns 404, the command
  sends one request to `GET /wiki/api/v2/folders/{id}`. This is the same
  endpoint that `confluence folder get` uses. A successful page GET sends no
  second request. An error that is not a 404 (401, 403, 5xx, network) sends no
  second request.
- **No retry for the probe.** `ApiClient::get` tries a 5xx or a 429 again with
  waits, and writes a `warn!` line for each try. That would delay the page
  error. The probe uses a copy of the client with one attempt
  (`with_retry_config`) and `request_raw`, so it is sent one time and writes
  no warning. No new method was added to the API crate.
- **Folder found.** The hint is
  `<id> is a folder, not a page. Use: atlassian-cli confluence folder get <id>`.
- **Folder not found, or the probe fails.** The hint names the three probable
  causes and the `confluence blog get` command. The probe error is written only
  to the debug log (`--debug`).
- **The first error stays.** The `Error:` line is the same as before
  (`Failed to get page <id>: Resource not found: ...`). Only the `Hint:` line
  changes. The exit code stays 1.
- **Match on the error type.** The code matches `ApiError::NotFound`. It does
  not read the message text.
- **`HintedContext`.** `main.rs` printed the hint from
  `ApiError::suggestion()`, which knows the HTTP status but not the command.
  `commands/common.rs` now has `HintedContext`, an `anyhow` context with a
  `message` and a `hint`. `main.rs` prints its hint when one is in the error
  chain, and uses `ApiError::suggestion()` when there is none. Other commands
  can use the same type.
- **Commands covered.** `page get`, `page update` and `page publish` all read
  `GET /wiki/api/v2/pages/{id}` first. They now share one helper
  (`fetch_page` in `confluence/pages.rs`), so all three give the hint.
- **Path encoding.** `page get` now sends the ID through
  `encode_path_segment`, as `update` and `publish` did before. An ID that is
  empty, `.`, `..` or contains `/` is refused before a request is sent. Other
  characters are percent-encoded, so a value such as `123?version=2` is one
  literal path segment and no longer reaches the query string.

## Limitations

- The probe checks only the folder endpoint. A blog post ID, a whiteboard ID or
  a database ID gets the general hint.
- `page delete`, `page versions`, `page comments`, `page add-label`,
  `page remove-label`, `page add-comment` and the three restriction commands
  use other endpoints and do not use the helper. A folder ID given to them
  still gets the general hint of `ApiError::suggestion()`.
- The probe obeys the rate limiter of the client. If the limiter tells the
  client to wait, the probe waits too.
- A folder that the profile cannot view gives 404 or 403 on the probe, so the
  general hint is shown.
- The behaviour is verified against a mock server and against the public REST
  document. It is not verified against a live Confluence site.

## Tests

`crates/cli/tests/confluence_page_get_hint_e2e.rs` runs the built binary
against `wiremock`:

- `a_successful_page_get_makes_no_folder_request`: the folder mock expects 0
  requests.
- `a_folder_id_names_the_folder_command`: page 404, folder 200.
- `an_unknown_id_names_the_probable_causes`: page 404, folder 404.
- `a_forbidden_folder_probe_keeps_the_page_error`: page 404, folder 403.
- `a_forbidden_page_makes_no_folder_request`: page 403, the folder mock expects
  0 requests.
- `page_update_on_a_folder_id_names_the_folder_command`: no PUT is sent.
- `page_publish_on_a_folder_id_names_the_folder_command`: no PUT is sent.
- `a_failing_folder_probe_is_not_retried`: page 404, folder 503, two requests
  in total and no warning.
- `an_id_with_a_slash_sends_no_request`.
- `an_id_with_a_query_marker_is_one_path_segment`.

The success test matches the query `body-format=storage`.

Unit tests in `crates/cli/src/commands/confluence/pages.rs`
(`page_not_found_tests`) cover the two hint texts and show that
`HintedContext` keeps the `ApiError` as the source of the error.

A probe that fails with a network error has no test of its own.
