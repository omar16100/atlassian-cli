# A `/wiki` base URL no longer doubles the segment

Status: in progress on `fix/wiki-base-url` (issue #133).

## Problem

`ApiClient` appends request paths to the base URL rather than replacing the
base's path: `normalize_base_url` gives the base a trailing slash and
`safe_join` strips the leading slash off the path before `Url::join`. Every
Confluence command spells `/wiki` itself, and every Jira command spells
`/rest/api/3`.

So a profile stored as `https://site.atlassian.net/wiki` requested:

```
GET /wiki/wiki/api/v2/pages          confluence page list
GET /wiki/rest/api/3/issue/DEV-1     jira issue get DEV-1
```

Both 404. Writing `/wiki` into the base is an easy mistake, because it is how
the Confluence REST documentation spells the full URL.

PR #131 hit this while fixing Confluence auth dispatch and compensated for the
three `auth` commands only, by returning an unprefixed user endpoint for such a
base. Every other command stayed broken, and #133 was filed rather than widening
that PR. There was also no test anywhere in the repo using a `/wiki` base, which
is why the fault survived this long.

## The fix

Strip one trailing `/wiki` from the base when building a client, and **read the
product hint from the base as the user wrote it**.

```rust
// crates/config/src/lib.rs
pub fn site_base_url(base_url: &str) -> &str;
impl Profile { pub fn site_base_url(&self) -> Option<&str>; }
```

`strip_suffix` rather than `trim_end_matches`, so a `/wiki/wiki` typo loses one
segment and stays visibly odd instead of silently resolving to the site root,
and so a path merely ending in the letters "wiki" (`/mywiki`) is untouched.

Applied at four places, which is every site that builds a client from a profile:

| File | Site | Covers |
| --- | --- | --- |
| `crates/cli/src/main.rs` | `resolve_profile_for_product` | Jira, Confluence and JSM, which all take their client from `build_product_client` |
| `crates/cli/src/commands/auth.rs` | `whoami`, `test_site_auth`, `auth_status` | each builds its own client |

**Bamboo is deliberately excluded.** It has its own resolver, and it is a Server
product where a context path in the base is legitimate and already pinned by
`test_resolve_url_keeps_a_context_path`. Bitbucket and Opsgenie never read the
profile's base URL at all.

`crates/api` is untouched. `normalize_base_url` is `pub` in a published crate,
and teaching a generic HTTP client about one product's path segment would be the
wrong layer.

### The trap: detection must read the base as written

A trailing `/wiki` is the **only** signal a Confluence-only profile on a plain
site gives. `user_info_path` reads it to decide whether `auth whoami`,
`auth test` and `auth status` should call Jira's `/rest/api/3/myself` or
Confluence's `/wiki/rest/api/user/current`.

Normalising the value that detection sees would therefore have turned every
Confluence profile back into a Jira one and reintroduced the 401 that PR #131
existed to fix. So the two are split:

- **detection** reads `profile.base_url`, unchanged;
- **the client** is built from `site_base_url(...)`.

They compose: the raw base still says Confluence, so the prefixed
`/wiki/rest/api/user/current` is chosen, and the client's base no longer carries
`/wiki`, so it resolves exactly once. `a_wiki_base_is_still_recognised_as_confluence`
in the e2e suite fails if anyone later "tidies" this by normalising before
detection.

That also makes `CONFLUENCE_CURRENT_USER_PATH_UNPREFIXED` and its branch dead,
and they are deleted. `user_info_path` returns to a single Confluence constant.

### What `auth login` does, and deliberately does not do

It does **not** rewrite the value. Storing the site root would look tidier and
would destroy the product hint for exactly the users PR #131 was for. Instead it
prints a note when the URL carries the suffix, saying `/wiki` is added
automatically for Confluence requests, that it is not needed, and that keeping it
marks the profile as Confluence.

Existing configs need no migration: the normalisation happens on read, in memory,
and the user's file is never rewritten behind their back.

## Verification

The bug was reproduced against a local logging server before anything was
edited, and each test was confirmed to fail against the pre-fix code.

Every base-URL shape, one run each, showing the path the server actually
received:

| base_url | `auth whoami` | Confluence | Jira |
| --- | --- | --- | --- |
| `<host>` | Jira | `/wiki/api/v2/pages` | `/rest/api/3/issue/DEV-1` |
| `<host>/wiki` | Confluence | `/wiki/api/v2/pages` | `/rest/api/3/issue/DEV-1` |
| `<host>/ex/confluence/cid` | Confluence | `/ex/confluence/cid/wiki/api/v2/pages` | `/ex/confluence/cid/rest/api/3/issue/DEV-1` |
| `<host>/ex/confluence/cid/wiki` | Confluence | `/ex/confluence/cid/wiki/api/v2/pages` | `/ex/confluence/cid/rest/api/3/issue/DEV-1` |

Tests:

- Unit, `crates/config`: `/wiki` and `/wiki/` stripped; a site root untouched;
  `/mywiki` and a `wiki.` hostname untouched; `/wiki/wiki` loses one segment
  only; the gateway form reduces to its cloud-id root; an unrelated context path
  survives; and the `Profile` accessor follows the same rule.
- Unit, `crates/cli/src/commands/auth.rs`: a `/wiki` base now selects the
  prefixed Confluence constant, and the `resolved()` helper applies the same
  normalisation the commands do, so the resolved-URL assertions test the real
  composition rather than a compensating constant.
- e2e, `crates/cli/tests/wiki_base_url_e2e.rs`: with a `/wiki` base,
  `confluence page list` hits `/wiki/api/v2/pages`, the `--space` filter resolves
  both of its requests, `jira issue get` hits `/rest/api/3/issue/DEV-1` with no
  prefix, and `auth whoami` still reports `Product: Confluence`. Each test also
  mounts the doubled path expecting zero hits, so a regression fails loudly here
  instead of becoming a 404 for a user. Four of the five fail against the pre-fix
  code.

## Limitations

A self-hosted Confluence whose context path is genuinely `/wiki` would now be
unreachable. That is not a regression in practice: this CLI targets Atlassian
Cloud (Jira v3, Confluence v2), Confluence Server is not supported anywhere in
the codebase, and such an install was already broken by the doubling. Bamboo,
the one Server product supported, keeps its context path.
