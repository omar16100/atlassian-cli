# Auth dispatch by product, and three fixes found alongside it

Status: in progress on `fix/auth-confluence-product-dispatch` (PR #131, by @shohi).

## Problem

`auth whoami`, `auth test` and `auth status` all called Jira's
`/rest/api/3/myself`, whatever the profile pointed at. On a Confluence profile
that built `https://api.atlassian.com/ex/confluence/<cloudId>/rest/api/3/myself`,
which always returns 401: the path does not exist on the Confluence gateway, so
no token and no scope could make it work.

It was worst when a Confluence profile was the default, because the first
command a new user ran failed with a *Jira* error, aiming the investigation at
the one thing that was not broken.

Three adjacent faults surfaced in the same debugging session and are fixed here
rather than left for later.

## The dispatch rule

The endpoint is derived from the profile's `base_url`:

| base_url shape | product | path sent |
| --- | --- | --- |
| `https://site.atlassian.net` | Jira | `/rest/api/3/myself` |
| `https://api.atlassian.com/ex/jira/<cloudId>` | Jira | `/rest/api/3/myself` |
| `https://api.atlassian.com/ex/confluence/<cloudId>` | Confluence | `/wiki/rest/api/user/current` |
| anything ending in `/wiki` | Confluence | `/rest/api/user/current` |

Both products return `accountId`, `displayName` and an email, so callers read
the same fields either way. Anything unrecognised is treated as Jira, which
keeps every existing profile working.

`auth status` shares the dispatch, so it covers Confluence for the first time
and names the product it actually tested rather than the ambiguous
`Jira/Confluence`.

### Why Confluence v1 and not v2

The v2 form, `/wiki/api/v2/users/current`, requires the `read:user:confluence`
scope, which a read-only scope set omits. Using it would reintroduce the same
401 by another route, on tokens that work everywhere else. The v1 path needs no
extra scope and returns the same fields. A test pins the constant so it cannot
be quietly modernised.

### The `/wiki` base URL trap

A base URL ending in `/wiki` needs the path *without* the `/wiki` prefix.
`ApiClient` appends to the base rather than replacing its path:
`normalize_base_url` gives the base a trailing slash and `safe_join` strips the
leading slash off the path, so the two concatenate. Prepending `/wiki` to a base
that already carries it asks for `/wiki/wiki/rest/api/user/current`, which 404s.

The tests assert on the **resolved URL** rather than on which constant the
dispatch returned. Asserting the constant proves nothing here, since the whole
fault lives in how the path and the base combine.

Note that a `base_url` ending in `/wiki` is still wrong for every other
Confluence command, which build `/wiki/api/v2/...` themselves and hit the same
doubling. Normalising a trailing `/wiki` off the base at `auth login` time is
worth doing separately.

## `confluence page list --space` was silently ignored

`/wiki/api/v2/pages` filters on a numeric `space-id`. It has no `space-key`
parameter, and it drops unknown parameters rather than rejecting them, so the
filter did nothing and **two different space keys returned byte-identical
site-wide results**. That is worse than an error: it looks like a space listing.

The key is now resolved to an id first, through the existing `resolve_space_id`
helper that `confluence folder list` already uses. Two consequences worth
stating:

- `--space` costs one extra request, and needs read access to the spaces
  endpoint. A listing without `--space` makes no extra request.
- An unknown key is now an error naming the key, rather than a plausible-looking
  listing of the whole site.

## 401s keep the server's reason

All four `UNAUTHORIZED` arms in `ApiClient` reported `Invalid or expired
credentials` and discarded the body, so a missing scope was indistinguishable
from an expired token, while the gateway body plainly said
`{"code":401,"message":"Unauthorized; scope does not match"}`. That masked
message cost a full debugging session.

The reason is now quoted after the generic wording, and the suggestion points at
scopes rather than at re-issuing a token when the message mentions one. The
parser reads `message`, `error_description`, `error`, a nested
`error.message`, and Jira classic's `errorMessages` array, falling back to the
body itself.

### What is not printed

- **HTML bodies are dropped.** A login page's markup tells the user nothing.
- **The detail is capped at 200 characters**, counted in characters, so
  multi-byte text cannot panic the slice.
- **Credential-shaped substrings are redacted.** A body we cannot parse is
  quoted whole, so a proxy that echoes the request back would otherwise put an
  `Authorization` value on the terminal, into shell history and into any CI log
  scraping stderr. `Bearer`/`Basic` followed by a base64 or JWT shaped token
  becomes `Bearer <redacted>`.

  The shape test matters: `Basic auth is not allowed` is a sentence servers
  really send, and redacting the word after the scheme would destroy the message
  to protect nothing. A token counts as credential-shaped only if it uses the
  base64 alphabet *and* is either 16 characters or longer, or carries one of the
  punctuation marks base64 and JWTs use and prose does not.

This is not a new class of exposure. The `FORBIDDEN`, `BAD_REQUEST`, `GONE` and
5xx arms have always surfaced the response body; 401 was the lone outlier
discarding it. Only the body is read, never headers, so `WWW-Authenticate` is
never printed.

## `auth test` hid the error source

It formatted the error with `{}`, which prints only the outermost context, so
the reason the request actually failed never appeared. Now `{:#}`, matching the
Bitbucket paths and `auth status`.

## Output changes

`whoami` gained a `Product:` line. The field lookup widened because Confluence
spells the email field `email` rather than `emailAddress` and can withhold
`displayName` in favour of `publicName`. `Active:` prints only when the API
reports it: Confluence omits it, and the previous `unwrap_or(false)` would have
declared every Confluence account disabled.

## Limitations

A Confluence-only profile whose `base_url` is the plain site
(`https://site.atlassian.net`, with no `/wiki`) is indistinguishable from a Jira
profile and is still dispatched to Jira. For a user without Jira access that
still 401s. The heuristic cannot tell them apart; an explicit product setting on
the profile would be needed, which is out of scope here.

## Tests

- Unit, dispatch: plain site, both gateway forms, `/wiki` bases with and without
  a trailing slash, mixed case, and the v1-path guard.
- Unit, **resolved URL**: the plain-site, `/ex/confluence/<cloudId>` and `/wiki`
  bases each resolve to exactly one `/wiki` segment. This is the test that fails
  when the prefix is doubled.
- Unit, `whoami` field lookup: Jira payload, Confluence payload, `publicName`
  fallback.
- Unit, 401 parsing: empty body, gateway scope body, nested `error.message`,
  Jira `errorMessages`, OAuth `error_description`, plain text, HTML, truncation
  including multi-byte, redaction of an echoed header, redaction of a JWT inside
  JSON, every occurrence redacted, and prose left intact.
- Integration, `crates/cli/tests/confluence_page_list_e2e.rs`: the command,
  not `ApiClient`, is what runs. It asserts the key is resolved and the pages
  request carries `space-id` and no `space-key`, that two keys return their own
  pages, that an unknown key errors without listing anything, and that a listing
  without `--space` makes no lookup at all. Three of the four fail against the
  pre-fix code.
- Integration, scope mismatch end to end through the existing wiremock harness.
