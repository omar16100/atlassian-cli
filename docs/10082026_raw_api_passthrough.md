# Raw API passthrough (`jira api`)

Status: implemented on `feat/jira-api-passthrough`.

The "Alternatives Considered" of issue #93, modelled on `glab api` and `gh api`:
call any Jira REST endpoint with the credentials `atlassian-cli` already manages,
instead of hand-rolling curl with a token. Ships separately from the
`jira attachment` group because it changes the shared `atlassian-cli-api` public
surface and carries its own security decisions.

## Surface

```
atlassian-cli jira api <PATH> [-X METHOD] [-d BODY] [-H NAME:VALUE]...
                              [--query KEY=VALUE]... [-i] [--output FILE]
                              [--dry-run] [--force] [--timeout SECS]
```

- `-X/--method` is a closed set (`get|post|put|patch|delete|head`). The default
  is GET with no body and **POST when `-d` is given**, matching gh/glab, which
  avoids the trap of a body being silently dropped on a GET.
- `-d/--data` takes inline JSON, `@file`, or `-` for stdin. The bytes go on the
  wire verbatim: no parse-and-reserialize, because some endpoints care about the
  exact payload and users expect to send what they typed.
- `--query k=v` is percent-encoded for you. Without it, JQL has to be written as
  `project%20%3D%20TEST`.
- `-i/--include` prints the status and response headers to stderr, with
  `set-cookie` redacted since this output gets pasted into bug reports.
- `--output FILE` writes the body verbatim, refusing to clobber an existing file
  without `--force` and using `create_new` so it will not follow a planted
  symlink.
- `--dry-run` prints the resolved request and sends nothing.
- `--timeout SECS` overrides the client-wide 30s total-request timeout.

Deliberately deferred: `--field`/`--raw-field`, which would collide semantically
with `jira issue create --field` (that one requires strict JSON via
`parse_custom_fields`, so two flags of the same name would follow different
rules); and `--paginate`, because Jira Cloud has at least four pagination shapes
and three result keys.

## Output contract

`OutputRenderer::render` is bypassed entirely. JSON bodies are pretty-printed,
`--format yaml` converts, and `--format table/csv/markdown/quiet` are ignored.
That last part is the important call: a top-level JSON array of objects would
render as a perfectly plausible table while silently dropping every nested field,
which is exactly wrong for a raw API dump. Non-JSON text passes through
untouched. A binary body goes to `--output`, or to stdout when stdout is not a
terminal; on a terminal it errors and points at `--output`. An empty body prints
nothing, unlike `ApiClient::request`, which coerces it to JSON `null`.

## New public API

`ApiClient::request` cannot back this: it discards the status and headers, maps
non-2xx to `ApiError` (throwing away Jira's `{"errorMessages": [...]}` body,
which is the whole point of a passthrough), has no hook for arbitrary request
headers, and forces `.json(body)`. `safe_join` was also private, so the CLI could
not reproduce the same-origin check.

```rust
pub struct RawRequest<'a> { method, path, headers, body, timeout }
pub struct RawResponse { status: u16, headers: Vec<(String, String)>, body: Vec<u8> }
impl ApiClient {
    pub fn resolve_url(&self, path: &str) -> Result<Url>;
    pub async fn request_raw(&self, req: RawRequest<'_>) -> Result<RawResponse>;
}
```

`request_raw` keeps auth, rate limiting and the same-origin check, returns
non-2xx as `Ok`, and retries 429/5xx **only for idempotent methods**. The
existing `request` retries POSTs, which can double-create; a raw passthrough must
not inherit that. It cannot use `retry_with_backoff`, whose closure has to signal
a retryable outcome as `Err`, which would discard the `RawResponse` needed on the
final attempt, so it drives `RetryConfig::backoff()` in a local loop.

## Safety

- `safe_join` is the boundary and it holds: other hosts, scheme downgrades,
  `https://site@evil.com/` and `\\evil.com/x` are all rejected.
- No `/rest/` prefix requirement. It would break `/secure/attachment/...`,
  `/download/...`, `/wiki/api/v2/...` and `/gateway/api/...`, and adds nothing
  over same-origin.
- `--force` gates DELETE, and overwriting an existing `--output` file. Requiring
  it for every write would be wrong, since several Jira read endpoints are POST. This stays non-interactive so the command
  is pipeline-safe, and it exits 1 rather than returning `Ok(())` the way
  `jira issue delete` does: a passthrough that silently no-ops inside a script is
  worse than one that fails.
- `Authorization` in `-H` is refused and points at `atlassian-cli auth`.
- Non-2xx prints the body, writes one line to stderr, and exits 1. Per-status
  exit codes would mean reworking `main`'s error handling for every command, so
  they are out of scope.

## Placement

`crates/cli/src/commands/api.rs`, not under `jira/`. Every product's `execute`
already receives the same `(ApiClient, &OutputRenderer)` pair and `main` already
owns per-product client construction, so the handler is product-independent.
Adding `confluence api` or `bb api` later is a variant plus a dispatch arm. A
top-level `atlassian-cli api` was rejected: it would need a `--product` selector
duplicating `main`'s profile resolution, and it would appear to work for
Jira/Confluence/JSM (same origin) while being wrong for Bitbucket, Opsgenie and
Bamboo.

## Tests

- 26 unit tests in `commands/api.rs` covering `default_method`, `append_queries`
  (encoding, first-`=` split, separator choice), `parse_headers` (trimming,
  duplicates, `Authorization` refusal, CRLF), `read_body` (verbatim, `@file`,
  stdin) and `format_body`, including a regression guard that a JSON array stays
  JSON rather than becoming a table.
- 5 wiremock tests in `crates/api/src/lib.rs` for `request_raw`: non-2xx returned
  as data with headers, headers and body applied, 5xx retried for GET, POST never
  retried, cross-host rejected, plus `resolve_url` origin pinning.
- 11 end-to-end tests in `crates/cli/tests/jira_api_e2e.rs` driving the binary:
  default GET, query encoding, verbatim body, custom headers, 404 body surfaced
  with a non-zero exit, 204 silent, binary to `--output`, DELETE and `--dry-run`
  both sending nothing, cross-host rejected, and `Authorization` refused.
