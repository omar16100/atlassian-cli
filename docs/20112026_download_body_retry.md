# Downloads: a second try when the body stops, and a longer limit

Status: in progress on `fix/download-body-retry` (PR #196).

## Problem

`bb pipeline logs` failed at times on a large step log, with nothing on stdout.
The owner saw this on releases 0.9.3 and 0.13.0: for one log, some runs failed
and the other runs gave the full log. One failed run showed "Failed to read
text response: error decoding response body", 30 seconds after the start.

## Cause

This is the probable cause. The mechanism is verified in the reqwest source and
by a test that makes the same error text. That the body of the log stopped on
the real site is not verified, and no test in the repository shows the redirect
to the file service.

`ApiClient::get_text` (`crates/api/src/lib.rs`) gets the log. Two things in it
turn a body that stops into a failed command:

1. **The timeout is a limit for the full request.** The client was built with
   `timeout(30 s)`. In reqwest that limit includes the read of the body. When
   the data stops in the middle of the body, the read ends at the limit with
   "error decoding response body". The 30 seconds of the report agree with
   this.
2. **The body read had no retry.** The error became `ApiError::InvalidResponse`,
   which is not retryable. The request is a GET, so a second try is safe, and
   the report shows that a second run usually gives the log.

The log of a step comes through a redirect to a file service, so its body
comes on a different connection from the API calls.

`get_bytes` (attachment and artifact downloads, and file content) has the same
two properties.

## What changed for users

- When the body of a download stops (the connection closes, or no data comes
  for 30 seconds), the command tries again, up to the usual number of tries. A
  warning on stderr names each new try, as for the other retries.
- A download has a limit of 300 seconds for the full request, where it had 30
  seconds. An ordinary API call keeps the limit of 30 seconds.
- When each try fails, the error is "Failed to read the response body: ...",
  with a hint, where it was "Invalid response format: ...".
- This applies to `bb pipeline logs`, `jira attachment download`,
  `confluence attachment download`, the Bamboo log and artifact commands, and
  the Opsgenie schedule export: each command that uses `get_text` or
  `get_bytes`.

## Design notes

- `HttpTimeouts` (crate-private) has three values: `request` (30 s), `read`
  (30 s) and `download` (300 s). The two clients are built with `timeout`,
  `connect_timeout` and `read_timeout` from it. `get_text` and `get_bytes` set
  the request limit to `download`.
- `read_timeout` is the longest time with no data in a body. It ends a
  download that stops, long before the download limit. In reqwest it is also a
  hard limit on the wait for the response headers, and a timeout on one request
  does not override it. For an ordinary call that changes nothing, because the
  full request limit has the same value. The client of `request_raw` does not
  get a read timeout, because `api --timeout` can ask for more than 30 seconds.
- The error text does not have the URL. After a redirect, the URL is the
  address of the file service, and the retry loop prints the text in a warning.
- The new error `ApiError::BodyReadFailed` is retryable. Only the two download
  functions make it. `InvalidResponse` stays not retryable, because the JSON
  calls use it for a body that arrived and cannot be parsed.
- `ApiClient::new` is unchanged for callers. `new_with_timeouts` is
  crate-private and exists for the tests.

## Limitations

- Not verified on a real site. The failure of the report occurs only at times,
  and this work makes no request to a real site.
- The worst case is longer. A body that stops costs 30 seconds for each try.
  A body that continues to arrive, but needs more than 300 seconds, fails at
  the limit and is tried again, so it can run three times (about 15 minutes)
  before the error. `bb pipeline logs` gets the log of each step in turn.
- The headers of a download must still arrive in 30 seconds. A timeout before
  the headers is not retried, as before.
- A download is still held in memory.
- A download that needs more than 300 seconds fails, also when data continues
  to arrive.
- `bb pipeline logs` collects the logs of all steps before it prints. When one
  step fails on each try, stdout is still empty.
- A new variant in the public `ApiError` enum can break a caller of the API
  crate that matches the enum with no wildcard arm.

## Tests

`crates/api/src/download_tests.rs`, with a small socket server that can stop in
the middle of a body (wiremock sends a response in one piece):

- `get_text_tries_again_when_the_body_is_cut` and
  `get_bytes_tries_again_when_the_body_is_cut`: the first connection closes in
  the middle of the body, the second gives it all.
- `a_body_that_is_cut_each_time_is_an_error_after_the_last_try`: three
  requests, then `BodyReadFailed`.
- `get_text_tries_again_when_the_body_stalls`: the first connection sends half
  of the body and then nothing; the read timeout ends the try.
- `a_download_that_continues_is_not_cut_by_the_request_timeout`.
- `an_ordinary_request_keeps_the_request_timeout`.
- `get_bytes_tries_again_when_the_body_stalls`.
- `request_raw_can_wait_longer_than_the_read_timeout`.
- `the_body_error_does_not_show_the_url`.
- `a_body_read_failure_is_retryable_and_has_a_hint`.

Before the fix, the cut and stall tests failed with `InvalidResponse("error
decoding response body ...")`, the same text as in the report.
