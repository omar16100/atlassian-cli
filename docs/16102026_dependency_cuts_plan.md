# Dependency cuts, 16 Oct 2026

Status: PR 1 (findings 2, 4, 6 and the `backoff` feature) implemented on branch `chore/dependency-cuts-1`, 16 Oct 2026; PRs 2 to 5 not started. Measured on `main` at `1301e23` plus the rustls 0.23.46 bump
from PR #160. Reviewed by Codex on 16 Oct 2026
(`/Users/macmini/projects/codex/atlassian_cli_dependency_cuts_16oct2026.txt`); its
corrections are folded in and marked **[codex]**.

## Why

The owner asked for opportunities to remove libraries after the rustls advisory round
(PR #160). Fewer crates means fewer advisories, faster CI and a smaller release build.
`Cargo.lock` holds 403 packages. The binary's normal dependency graph on macOS has 237
package versions (232 distinct names), workspace crates included.

## How the numbers were measured

- Direct dependencies: every `crates/*/Cargo.toml` plus `[workspace.dependencies]`.
- Real use: a grep for `name::`, `use name`, `#[name` in each crate's `src/`, `tests/` and
  `benches/`. The workspace has no `build.rs`.
- Graph size **[codex]**: `cargo tree -p atlassian-cli -e normal --no-dedupe --format '{p}'
  --prefix none | sort -u | wc -l`, per target. The earlier `sort -u` without `--no-dedupe`
  double counts entries marked `(*)`.
- TLS backend selection: `reqwest-0.13.5/src/tls.rs`, `impl Default for TlsBackend`, and the
  connector match in `async_impl/client.rs`.
- Every PR records before and after: `grep -c '^name = ' Cargo.lock`, the per-target graph
  count, the inverse tree of each crate claimed removed (`cargo tree -i <crate>` must fail),
  and the release binary size on `aarch64-apple-darwin`.

## Findings, largest first

### 1. Two TLS stacks are compiled, one is used

`[workspace.dependencies] reqwest` enables `native-tls-vendored` on top of the 0.13 default
feature set, and the default set already contains `rustls` (`default-tls = ["rustls"]`).
With both `__native-tls` and `__rustls` on and no `http3`, `TlsBackend::default()` returns
`NativeTls`. Both workspace clients call `Client::builder()` with no backend override. So on
the current request paths the shipped binary uses native-tls, and `rustls`, `hyper-rustls`,
`rustls-platform-verifier`, `aws-lc-rs` and `aws-lc-sys` (a C and assembly build) are
compiled and never selected. The rustls advisories that PR #160 closed were in code the CLI
does not reach.

The feature was added in commit `7c199d2` (Nov 2025) for the musl target, when reqwest
0.12 defaulted to native-tls and the musl build needed vendored OpenSSL.

| Option | Change | Effect | Risk |
| --- | --- | --- | --- |
| A, keep native-tls | `default-features = false`, features `charset, http2, system-proxy, json, multipart, native-tls-vendored` | Drops the rustls, hyper-rustls, platform-verifier and aws-lc subtree. Same backend, same system certificate store, no behaviour change. **[codex]** `rustls-pki-types` stays (reqwest's `__tls` feature needs it), and `webpki-root-certs` only exists on some targets. | Low. The four cargo-dist targets must build, not only plan. |
| B, keep rustls | Remove `native-tls-vendored`, keep the defaults | Drops native-tls and vendored OpenSSL on musl. **[codex]** `security-framework` and both `core-foundation` versions stay on macOS through `rustls-platform-verifier` and `system-configuration`. | Medium. Verifier changes from OpenSSL or Security.framework to the platform verifier; corporate CAs and proxies need a live test. `aws-lc-sys` needs a C compiler and cmake on the musl runner. **[codex]** There is no `rustls-tls-native-roots` feature in 0.13; the default `rustls` feature already uses the platform verifier. |

Recommendation: A now, as its own PR. Revisit B only after a live test behind a proxy with
a custom CA.

**[codex] Drop `stream` too.** No workspace code calls `bytes_stream`, `Body::wrap_stream`,
`Part::file` or `Part::stream`. `Response::bytes()` and `Part::bytes()` do not need it.

reqwest API used by the workspace: `Client::builder`, `redirect::Policy::custom`, `json`,
`multipart`, `header`, `Response`, `Method`, `Error`. No cookies, no gzip, no brotli.
`charset` stays on because `Response::text` uses it for non-UTF-8 bodies.

Option A also lets `deny.toml` drop the three licence allowances that exist only for the
rustls subtree: `ISC` (aws-lc-rs), `OpenSSL` (aws-lc-sys) and `CDLA-Permissive-2.0`
(webpki-root-certs). `todo.md` records that they were added for the reqwest 0.13 bump in
PR #29. **[codex]** A green licence check does not prove removal; the inverse trees do.

### 2. Declared but never used (remove, zero risk)

All counts re-checked by Codex over source, tests, benches and macro forms.

| Crate | Declared in | Uses | Leaves the lock file? |
| --- | --- | --- | --- |
| `async-trait` | api | 0 | yes, with `async-trait` and one `syn 3` user |
| `async-stream` | api | 0 | yes, with `async-stream-impl` |
| `futures` | api | 0 | no, bulk needs it |
| `anyhow` **[codex]** | api | 0 in compiled code | no, shared |
| `tokio` | auth | 0 | no, shared |
| `thiserror` | auth, config | 0 | no, shared |
| `dirs` | auth | 0 | no, config needs it |
| `tracing` | config | 0 | no, shared |

**[codex] Dead file.** `crates/api/src/client.rs` is the only user of `anyhow` in the api
crate, and `lib.rs` never declares `mod client`. It is a leftover from the first prototype
(`AtlassianClient`, user agent `atlassian-cli/0.1.0`). Delete the file with the `anyhow`
declaration.

Shared crates stay in the lock file through other crates, so these removals shrink each
crate's own build and make its manifest honest. Only `async-trait` and `async-stream` leave
the lock file.

### 3. `backoff` (27 crates, unmaintained, two `deny.toml` ignores)

`backoff` is unmaintained (RUSTSEC-2025-0012) and pulls `instant` (RUSTSEC-2024-0384) and
`getrandom 0.2`, one of the duplicate pairs. The workspace already implements its own retry
loops and uses `backoff` mainly as the delay generator.

**[codex] The replacement must preserve two contracts**, read from `crates/api/src/retry.rs`,
`ApiError::is_retryable` and `request_raw`:

- `max_retries = 3` means three total attempts.
- ±10% jitter on each interval. `max_interval` caps the base before jitter, so a 30 s base
  can produce about 33 s. No elapsed-time cutoff.
- Generic requests retry selected API errors, including POST failures. Raw requests retry
  only idempotent methods on 429 and 5xx, keep the final response and honour a numeric
  `Retry-After`. Neither path retries transport errors today.
- `RetryConfig::backoff()` is public and returns `ExponentialBackoff`. Removing it changes the
  published API of `atlassian-cli-api`, so the PR states the compatibility decision and the
  version bump.
- Jitter needs a random source. Keeping `rand 0.8` for it defeats the duplicate removal;
  `fastrand`, already in the tree, is one option.
- Tests to add before the swap: attempt limits, jitter bounds, interval growth, overflow,
  `Retry-After`, final-response preservation. The existing tests at `lib.rs:1385` and
  `lib.rs:1417` cover attempt counts only.

Interim manifest cut with no code change: drop the unused `tokio` feature of `backoff`.

### 4. `tokio` feature `full`

Used modules: `time`, `runtime`, `sync`, `spawn`, `io` (one place), `fs` (one place), the
`main` and `test` macros (72 tests ask for `flavor = "multi_thread"`). Replace `full` with
`["rt-multi-thread", "macros", "time", "sync", "fs", "io-util"]`.

**[codex] What it really removes:** `process`, `signal`, `io-std` and tokio's `parking_lot`
feature, so `signal-hook-registry` can disappear. `net`, `mio` and `socket2` stay because
reqwest, hyper-util and wiremock enable `tokio/net`. `parking_lot` stays through
`string_cache` (htmd). CI tests, not source inspection, prove the set is sufficient.

### 5. `urlencoding` (1 crate, 11 call sites in cli)

All 11 calls are `urlencoding::encode`, and **[codex]** all of them encode query
components, not path identifiers (for example `append_queries` in `commands/api.rs`).
`percent-encoding 2.3.2` is already in the tree through `url`, reqwest and hyper-util.
`urlencoding::encode` keeps only ASCII letters, digits and `-._~`, so the equivalent is:

```rust
const QUERY_COMPONENT: &percent_encoding::AsciiSet =
    &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
// utf8_percent_encode(value, QUERY_COMPONENT)
```

Add `encode_query_component` to `crates/api` next to the path helpers, but keep it separate
from path validation: queries may legitimately contain `/` and `..`. Add equivalence tests
for ASCII punctuation, spaces, `+`, `%`, `/` and Unicode. Do not use the form serializer,
which turns spaces into `+`.

### 6. Small feature cuts with no code change **[codex]**

| Crate | Change | Removes |
| --- | --- | --- |
| `argon2` | `default-features = false, features = ["alloc"]`; `hash_password_into` needs only `alloc` | `password-hash`, `phc` |
| `futures` in bulk | `futures-util` with `default-features = false, features = ["std"]`; bulk uses `StreamExt`, `TryStreamExt`, `buffer_unordered`, `try_collect` only | `futures-executor`, `futures-macro` (and a `syn 3` user) |
| `criterion` (dev) | `default-features = false, features = ["cargo_bench_support", "async_tokio"]`; `html_reports` is an empty feature, the weight is the default `plotters` and `rayon` | `plotters`, `rayon` from `clippy --all-targets` in CI |
| `proptest` (dev) | `default-features = false, features = ["std"]`; no fork, timeout or bit-set config in the tests | subprocess test crates |

Keep the `aes-gcm` random feature: nonce generation needs it. Re-run the key and ciphertext
compatibility tests in `crates/auth` after the `argon2` change.

### 7. Candidates to keep, with the reason

| Crate | Reason to keep |
| --- | --- |
| `colored` | One file, uses `green`, `red`, `yellow`, `bright_red`, `bold`, and the crate handles `NO_COLOR` and `CLICOLOR_FORCE`, which the tests depend on. |
| `htmd` | HTML to markdown for Confluence export; the subtree is `html5ever`. Keep unless a replacement is validated on representative Confluence exports. |
| `aes-gcm` + `argon2` | Credential store encryption. |
| `whoami` + `machine-uid` | Key derivation input in `encryption.rs`. Replacing them changes the derived key and locks every user out of stored credentials. Needs a key migration first. |
| `indicatif` | Progress bars in bulk operations. Replaceable by a line counter, but the UX loss is real. |
| `csv` | Three `csv::Writer::from_path` sites. Hand-written CSV quoting is a known bug source. |
| `rpassword` | Hidden token entry. |
| `serde_yaml` | Deprecated, and the source of the `unsafe-libyaml` advisory (ignored in `deny.toml` by PR #160). **[codex]** `serde_yaml_ng` still depends on `unsafe-libyaml`, and `serde_yml` is unsound and unmaintained (RUSTSEC-2025-0068). A swap needs its own dependency review and compatibility tests on existing config files, config output, YAML output and raw API YAML output. A same-library round trip proves nothing. |

### 8. Duplicate versions in the binary graph **[codex, corrected]**

| Pair | Owners | Fix |
| --- | --- | --- |
| `base64 0.22` / `0.23` | hyper-util / auth and reqwest | None available from the workspace; waits on hyper-util. |
| `getrandom 0.2` / `0.4`, `rand_core 0.6` / `0.10` | `backoff` and `rand 0.8` / `argon2`, `aes-gcm` | Finding 3 removes the `backoff` side. `proptest` keeps `rand 0.9` and `getrandom 0.3` in dev builds. |
| `core-foundation 0.9` / `0.10` | `system-configuration` / native-tls and the rustls platform verifier | Neither option of finding 1 removes it; waits on `system-configuration`. |
| `syn 2` / `3` | `syn 3`: `serde_derive`, `async-trait`, `clap_derive`, `futures-macro`, `thiserror-impl` | Findings 2 and 6 remove two `syn 3` users; `syn 2` stays through other derives. |

## Proposed order

1. PR: finding 2 (unused declarations, dead `client.rs`), finding 4 (tokio features),
   finding 6 (feature cuts), and the `backoff` `tokio` feature. Manifest edits plus one file
   deletion. `make pre-commit` proves the tokio set.
2. PR: finding 1 option A, with `stream` dropped and the three licence allowances removed.
   **[codex]** The release workflow's `plan` job does not compile anything; require real
   artifact builds for all four `dist-workspace.toml` targets (a dry-run tag on a fork, or
   `dist build` per target) and one live request against a real tenant on macOS and Linux.
3. PR: finding 3 (replace `backoff`), with the contract tests above, the two `deny.toml`
   ignores removed, and the public API decision recorded.
4. PR: finding 5 (`urlencoding` to `percent-encoding`) with `encode_query_component`.
5. Separate review: `serde_yaml` replacement.

## PR 1 result (16 Oct 2026)

Measured on `aarch64-apple-darwin`, normal edges, `--no-dedupe`:

| Metric | Before (`7a298e7`) | After |
| --- | --- | --- |
| `Cargo.lock` packages | 403 | 386 |
| Binary graph, distinct packages | 237 | 227 |
| Release binary bytes | 25182160 | 25139232 (0.2% smaller; the removed crates were mostly dev or already linked out) |

Gone from the lock file: `async-trait`, `async-stream`, `async-stream-impl`,
`signal-hook-registry`, `plotters`, `rayon` and their private dependencies. `password-hash`
stays listed in `Cargo.lock` because the lock file records optional dependencies, but no
target compiles it (`cargo tree -i password-hash --target all` finds no dependents).
`futures` (the umbrella) left the release graph; it stays in dev builds through wiremock.

## Verification

- `make pre-commit` and `cargo deny check` green on each PR.
- `cargo tree -i <crate>` fails for every crate the PR claims to remove.
- `cargo tree -p atlassian-cli -e normal -d` shows the duplicate removed where claimed.
- For the TLS PR: four-target artifact builds plus `atlassian-cli jira issue get <KEY>`
  against a real tenant on macOS and Linux.
