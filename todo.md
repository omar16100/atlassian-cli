# Changes Made

## 2026-08-18 — Fixed docs/examples/**/*.sh CLI invocations (closes #105)

### Context
[Issue #105](https://github.com/omar16100/atlassian-cli/issues/105) reported that every
`atlassian-cli` invocation in `docs/examples/bitbucket/pr-automation.sh` failed to parse.
Auditing the fix turned up a much larger, pre-existing problem: all 10 scripts under
`docs/examples/**/*.sh` placed `--profile "$PROFILE"` after the product subcommand (e.g.
`bitbucket`, `jira`, `confluence`) instead of before it. `--profile` is a top-level `clap`
field on the `Cli` struct without `global = true` (unlike `--format`/`--envelope`, which do
have it), so it is only accepted before the subcommand word. In practice this meant literally
none of the 10 example scripts worked as shipped, not just the one the issue named.

### Additional bugs found during the audit
Three of the Bitbucket scripts — `bitbucket/pr-automation.sh`, `bitbucket/repo-audit.sh`, and
`bitbucket/branch-cleanup.sh` — also passed the Bitbucket workspace as a bare positional
argument instead of via the `--workspace` flag, so they had two independent reasons to fail
parsing. `jira/project-cleanup.sh` was separately missing the required `--action` flag on one
of its `jira bulk label` calls. `jira/bulk-transition.sh` referenced a `--comment` flag on
`jira bulk transition` that the CLI has never actually supported; that dead option was removed
from the script rather than implemented, since nothing downstream depended on it.

### `pr-automation.sh`'s `get_approval_count()`, the function issue #105 centered on
The specific function the issue reported queried a `.participants[]` field on `pr get`'s JSON
output that doesn't exist. It now calls `pr reviewers --format json` and filters on the real
`status` field instead, which is the data Bitbucket actually returns for reviewer approval
state. The same script's `merge_pr()` also passed `--strategy merge`, but the CLI (and the
underlying Bitbucket API) only accept `merge_commit`, `squash`, or `fast_forward`; corrected to
`merge_commit`.

### Regression test
`crates/cli/tests/docs_examples_scripts.rs` now statically extracts every `atlassian-cli`
invocation out of `docs/examples/**/*.sh` — following backslash line continuations, bash array
assignment/append/expansion, and variable substitution — and checks that each one parses via
clap against the real binary. Before any of the fixes above it failed with 31 "unexpected
argument '--profile'" errors; the full suite (`cargo test -p atlassian-cli`, including this new
test and the existing `docs_examples::every_readme_command_parses`) is now green, and
`cargo clippy -p atlassian-cli --tests --all-features -- -D warnings` reports no warnings. This
closes #105 and should keep this whole class of bug from silently regressing again.

## 2026-07-13 — Website separated into private repo; removed from public CLI repo

### Context
The website was split out into its own private repo, `omar16100/atlassiancli-site`, deployed via
Cloudflare Pages (`atlassiancli.com` now resolves to Cloudflare via Porkbun nameservers). The public
`atlassian-cli` repo still carried a stale duplicate of the served site under `docs/`, with GitHub Pages
enabled from `main/docs` and a `docs/CNAME` claiming `atlassiancli.com`. PR #87 had bumped that stale
copy to 0.4.3 by mistake; the live private site was still on 0.4.2.

### Live site fix (private repo, direct to main)
Ported the vetted 0.4.3 changelog entry and version bumps (footer badge, `softwareVersion` JSON-LD,
install prose, `llms.txt`; JSON-LD ItemList renumbered) into `atlassiancli-site` and pushed to `main`
so Cloudflare Pages redeploys. 83 files, version-string only.

### Public repo cleanup (chore/remove-public-site)
Removed the 92 served-website files from `docs/` (all HTML/CSS/JS/assets, `CNAME`, `.well-known`,
`robots.txt`, `sitemap.xml`, `llms.txt`, blog/changelog/install/runbooks/product pages). Kept the 23
genuine CLI dev docs (`c4model.md`, `status.md`, `vision.md`, `plan.md`, `todo.md`, dated dev logs,
`examples/*.sh`). Disabled GitHub Pages on the public repo so it stops building from `main/docs` and
releases the orphaned `atlassiancli.com` CNAME claim. README/SECURITY links to `atlassiancli.com/*`
kept as-is (still served by Cloudflare).

### SEO plan relocated + scrubbed from public history
`docs/seo_traffic_growth_plan.md` (website SEO strategy) already existed byte-identical in the private
repo at `research/seo_traffic_growth_plan.md`, so no copy was needed. Removed it from the public repo's
entire git history with `git filter-repo --invert-paths`, then force-pushed the rewritten `main` and all
28 tags. Also deleted two stale merged-PR branches (`chore/deps-production-group` #84,
`fix/confluence-comments-and-attachments` #85) that still carried it, and cleaned 22 stale local
branches. Verified: a normal `git clone` (branches + tags) has zero references to the file and every
release kept its 14 assets. Side effects, accepted by the user: the rewrite stripped GPG signatures
from all 224 signed commits (Verified badges lost repo-wide; no content/author/date/message changed),
and the file still exists in GitHub's read-only `refs/pull/*` PR-snapshot refs, which git cannot rewrite
(purging those requires GitHub Support). Pre-scrub backups saved to the session scratchpad
(`atlassian-cli-pre-scrub.bundle`, `local-branches-backup.bundle`).

### .wrangler + tmp gitignored and scrubbed
Added `.wrangler/` (Cloudflare Pages deploy cache) and `tmp/` (stray Playwright screenshots) to
`.gitignore`, dropped both from tracking at the tip, then ran a second `git filter-repo --invert-paths`
to remove them from all history and force-pushed `main` + tags (only `v0.4.3` contained them, so only
that tag moved; its 14 release assets intact). `.wrangler/cache/wrangler-account.json` had exposed the
Cloudflare account id and account name (no credential). Same `refs/pull/*` residue caveat applies.

## 2026-07-13 — Open-PR sweep: 4 PRs triaged, fixed, merged; release 0.4.3

### Context
Four PRs were open and none could merge as-is, and `main` itself was red: the Security job
(`cargo deny check advisories`) had been failing since ~2026-07-10, unnoticed because `cargo audit` is
`|| echo`-guarded and only `cargo deny` fails the job. CI had never run on the two fork PRs (#79, #81),
which were stuck awaiting workflow approval, so neither had ever been compiled or tested.

### Red main first (#82, fix/security-advisories)
Lockfile-only bumps: quinn-proto 0.11.13 → 0.11.16 (RUSTSEC-2026-0037, -0185; via reqwest),
crossbeam-epoch 0.9.18 → 0.9.20 (RUSTSEC-2026-0204; via criterion/rayon), anyhow 1.0.102 → 1.0.103
(RUSTSEC-2026-0190 unsoundness in `Error::downcast_mut`). Found the anyhow one only by running
`cargo deny check advisories` locally: it still failed after the first two.

### PR #81, Jira GFM tables → ADF (@fabianderschatta), merged
The conversion itself was correct. One defect found and fixed on the branch: ADF forbids `table` inside
`listItem`/`blockquote` (content restricted to paragraph/bulletList/orderedList/codeBlock/media*), but an
indented or quoted markdown table emitted `listItem > table` / `blockquote > table`, which Jira rejects with
a 400 for the whole document, and via `jira bulk import` that fails the entire batch, not one row.
Now degraded to one paragraph per cell, reusing the existing `__transparent__` splice (same precedent as
`blockquote_in_list_item_is_flattened`). Verified `attrs` is optional on table/tableRow/tableCell/tableHeader
against Atlassian's ADF spec, so omitting them is valid.

### PR #79 → #85, Confluence page comments + attachment downloads (@alexevansigg), merged
Both reported bugs real; cherry-picked with authorship preserved, three corrections:
- Comments: per the v2 spec, `FooterCommentModel` has NO top-level `createdAt`; it is `version.createdAt`.
  The PR's `Option<String>` stopped the crash but left the column permanently blank (hard error traded for
  silent data loss). Now reads `version.createdAt`. The command was broken for ANY page with >=1 comment.
- Attachment download: now fetches via `ApiClient::get_bytes` (auth + retries + rate limit + same-origin
  SSRF check, query string preserved) instead of a hand-rolled absolute URL + raw reqwest. Only the `/wiki`
  prefixing is new logic (`attachment_download_path`). Matches `bamboo::download_artifact`.
- Comment body: bounded single-line preview by default, full body behind `--full`, mirroring
  `jira issue comments`.
- The fixture at `crates/cli/tests/confluence_integration.rs:398` had invented a `/wiki`-prefixed
  `downloadLink` the real API never returns, encoding the bug as expected behaviour, which is why no test
  caught it. Corrected. Neither command had ANY test; both now have wiremock coverage.

### PR #80 → #84, aes-gcm 0.11 + indicatif, merged
Dependabot's group could never go green: aes-gcm 0.10 → 0.11 is breaking (aead 0.6,
generic-array → hybrid-array). Migration in `crates/auth/src/encryption.rs`: `aead::OsRng` is gone (nonces
now via the `Generate` trait, using `try_generate` not `generate`, since the latter panics on RNG failure);
`Array::from_slice` deprecated to `TryFrom` (a hard error under `-D warnings`); the orphaned
`argon2::password_hash::rand_core::RngCore` import removed. `derive_key()` and `NONCE_SIZE` frozen.
Dropped `rand` and `zeroize` from crates/auth: direct deps, imported nowhere.

**On-disk compatibility.** Every pre-existing auth test is a same-process round-trip and would pass even if
the format silently changed and orphaned users' `~/.atlassian-cli/credentials.enc`. Generated a
nonce/ciphertext vector with the OLD aes-gcm 0.10 build and pinned it as
`decrypts_ciphertext_written_by_aes_gcm_0_10`. Also live-verified: the new binary decrypted the real local
`credentials.enc` and made an authenticated API call; the file was byte-identical afterwards.

### PR #76, actions/checkout 4 → 7, merged
Red because `release.yml` is generated by cargo-dist (0.30.3), which pins `actions/checkout@v4`; editing it
makes `dist generate --check` fail in the `plan` job. Bumping `cargo-dist-version` would not help, since even dist
0.32 still emits v4. Kept v7 in the three hand-written workflows, left release.yml as dist generates it.
Decision: no dependabot ignore for actions/checkout (it appears in both dist-generated and hand-written
workflows, and ignores are per-dependency, not per-file); future re-proposals get closed manually.

### fix(output), CSV/markdown renderers (found during review, affects every command)
`render_csv` did `row.join(",")` with no quoting: any field with a comma (issue summaries, comment bodies)
shifted every later column, and a newline destroyed the row. Now RFC 4180 quoting. `render_markdown_table`
escaped `|` but not newlines, which terminate the row in markdown. Now `<br>`. This, not the `--full`/preview
split, is the actual fix for the multi-line body hazard; it also repairs `jira issue comments -o csv`.

### Not live-tested
The configured instance has no Confluence (404 on `/wiki/api/v2/spaces`) and no Jira projects, so the
Confluence comment/attachment fixes and the ADF table rendering are covered by unit + wiremock tests and
spec verification, but were not exercised against a live instance. The aes-gcm migration WAS live-verified
against the real credentials file.

### Release 0.4.3
Version bumped in Cargo.toml, the 5 path-dep versions in crates/cli/Cargo.toml, and Cargo.lock.

## 2026-07-11 — SEO content expansion: 50 blog posts + 5 priority fixes (data-driven)

### Context
Live GA4 (520368061) + GSC (sc-domain:atlassiancli.com): ~322 clicks / ~18k impr / 28d (up ~20x from the
early-2026 baseline of 51 clicks/3mo), avg pos ~8.5, but 0 top-10 rankings and a near-zero backlink profile
(1 referring domain per DataForSEO). DataForSEO + Reddit research drove a content + fix plan, codex-reviewed twice.

### Research & planning (moved to the private repo 18 Aug 2026; was NOT unpublished before then)
- DataForSEO: ranked keywords, Jira/Confluence/Bitbucket/JSM keyword universes, SERP competitors, backlinks, AI/LLM demand.
- Reddit: 59 threads mined (top pain: "no gh-style Bitbucket CLI"; cross-product CLI; Confluence->markdown export).
- CONTENT_AND_FIX_PLAN.md + BING_RECOVERY_AND_BACKLINKS.md. Codex reviews in /Users/macmini/projects/codex/.

### 50 new blog posts (docs/blog/*.html)
- One DISTINCT primary keyword each (dev/CLI/API/automation intent), clustered jira/confluence/bitbucket/jsm/cross,
  interlinked to hubs/runbooks/siblings. Each: BreadcrumbList+Article+FAQPage JSON-LD, canonical, gtag,
  verbatim non-affiliation footer + above-the-fold affiliation note, acli-differentiation, ~1400-2000 body words.
- Adversarial QA gate: 49/50 passed; fixed jsm-integrations-cli (removed non-functional ATLASSIAN_CLI_PROFILE env var).
- All CLI commands verified vs docs/docs/commands.html. 0 em dashes, 0 broken internal links, all JSON-LD parses.

### 5 priority fixes (codex-reordered; backlinks flagged as #1 lever)
1. /jira/ made the definitive "Jira CLI" page (query-first title/meta, commands table, FAQ, exact-anchor internal
   links) to fix the homepage-vs-/jira/ URL-selection issue for "jira cli" (1,900/mo).
2. jira-cli-tools-compared: CTR title rewrite + removed wrong-intent schema keywords.
3. Hub depth on /jira/ /confluence/ /bitbucket/ /jsm/ (command tables, FAQ, internal links).
4. confluence-markdown-sync runbook: markdown-cluster keywords + FAQ + interlinks.
5. Bing recovery + backlinks documented; README deep links; custom docs/404.html.

### Consolidation & compliance
- sitemap.xml 32->82 urls (valid), blog/index.html +50 cards, llms.txt updated.
- Codex implementation review caught + fixed: fabricated Jira hub capabilities, unsupported stats
  ("300% since 2023", "single most upvoted"), wrong acli product coverage, a bad --output flag / bbpr arg order,
  and added above-the-fold non-affiliation notices. Removed false "Atlassian Never Shipped" claim.

### Deploy (DONE 2026-07-11)
- Merged seo/content-expansion-50-blogs -> main, pushed (a039319..34ec806). GitHub Pages build for 34ec806
  completed cleanly (status=built, error=None). Published main source verified via API: sitemap 82 <loc>,
  all sampled new posts + 404.html present. Browser-render check not performed from this env (outbound network
  blocked here); recommend a manual eyeball of 2-3 live URLs.

### Risk noted
- 50 same-day posts on a ~0-backlink domain carries Google scaled-content risk; mitigated via distinct keywords,
  genuine content, strict QA. Recommend noindex on later waves as an opt-in follow-up.

## 2026-07-13 — Jira markdown-to-ADF: GFM table support

### Context
`--description`/comment `--body` markdown tables rendered as raw pipe text in
Jira (`| a | b |` dumped literally) instead of a table. Root cause:
`markdown_to_adf` never enabled pulldown-cmark's `ENABLE_TABLES` option, so
table syntax was never parsed as a table.

### Change (crates/cli/src/commands/jira/adf.rs)
- Enabled `Options::ENABLE_TABLES`.
- Mapped `Table`/`TableHead`/`TableRow`/`TableCell` events to ADF
  `table`/`tableRow`/`tableHeader`/`tableCell` nodes; headings inside table
  cells downgrade to paragraphs (same restriction as list items/blockquotes).
- New unit test `gfm_table_converts_to_adf_table`; manually verified against a
  live Jira issue (table renders correctly via both `--description` and
  comment `--body`).

## 2026-06-18 — Jira sprint UX: --sprint flag + sprint in issue get (#72)

### Context
Issue #72 reported `--field customfield_10020=<id>` failing to set a sprint. Not a
current bug: on 0.4.1 it already sends the working payload, the "204 error" was the
pre-0.4.0 false alarm, and `issue get` didn't show the sprint (so "None" was a
verification artifact). Added ergonomic sprint support.

### Change (crates/cli/src/commands/jira/{issues.rs,mod.rs})
- `--sprint <id>` flag on `jira issue create` and `update`, implemented via the Agile
  API `POST /rest/agile/1.0/sprint/{id}/issue` `{"issues":[KEY]}` (no customfield id,
  avoids the "Number value expected" quirk). New `add_to_sprint` + pure
  `build_sprint_add_payload`; numeric-id validation.
- `update` now PUTs only when there are field changes, so a `--sprint`-only update
  doesn't send an empty `{"fields":{}}`; bails if nothing to update.
- `issue get` now displays the sprint: `IssueFields` reads `customfield_10020`
  (optional/default), `extract_active_sprint` summarizes it ("Sprint 12 (active)"),
  shown in markdown + JSON/table output.
- 5 unit tests; commands.html updated. Also merged dependabot PR #71 (zeroize 1.9.0).

## 2026-06-14 — Site-wide CLI command-syntax cleanup (docs accuracy)

### Context
The docs (commands.html, blog posts, runbooks, example .sh) were written against an
older CLI and used many invalid command forms. Verified every form against the built
binary's --help and corrected them.

### Fixed (all verified against the binary)
- `jira <verb>` -> `jira issue <verb>` (search/get/create/update/delete/transition/
  assign/unassign), including bare `<code>` prose refs.
- `bitbucket pullrequest` -> `bitbucket pr`; `bitbucket permissions` -> `permission`.
- `jsm servicedesk` -> `jsm service-desk` (commands + prose; left `--servicedesk-id`).
- Confluence positional IDs: `page/blog/attachment <verb> --id N` -> positional `N`;
  `attachment list --page-id N` -> `N`; add-label/remove-label/add-comment now positional;
  add/remove-restriction -> `--operation --subject-type --subject-id <PAGE_ID>`;
  space add-permission -> `--permission --subject-type --subject-id <KEY>`.
- `confluence search cql/text/in-space` use positional QUERY/SPACE, not --cql/--query/--space.
- `confluence bulk export/delete --space X` -> `--cql "space = X"` (incl. multi-line/span).
- `confluence analytics page-views/space-stats` use positional ID/KEY.
- jira roles/components/versions use positional `<PROJECT>`/`<ROLE_ID>` not --project/--role-id;
  `jira workflows export` uses positional `<NAME>`.
- `--output json` -> `--format json` on repo list / space get; fixed an invalid
  `pr merge --auto-complete` hero snippet.

### Verification
Built a flag-audit script (/tmp/flagcheck.py) that resolves each documented command's
path against the binary and checks every --flag exists. All 42 command paths valid;
remaining audit hits are false positives (global --profile, one prose `pr create:` line).
Structural-pattern greps all return 0.

## 2026-06-14 — Site: 0.4 release announcement + changelog page

### Context
v0.4.0 and v0.4.1 shipped but were never announced on atlassiancli.com, the site had
no changelog page, and every page still showed v0.3.3.

### Change (docs/ only — the published site)
- New `docs/blog/atlassian-cli-0-4-release.html` — combined "What's New in 0.4" post
  (markdown->ADF, Confluence folders, custom pipelines, 0.4.1 attachments + comments
  --full, the HTTP 204 fix). Mirrors the existing blog template; correct `jira issue`
  command syntax.
- New `docs/changelog/index.html` (`/changelog/`) — version blocks (0.4.1, 0.4.0, 0.3.3,
  link-out for older), CollectionPage/ItemList JSON-LD.
- Wired in: new blog card on `/blog/`, sitemap entries + refreshed lastmods, Changelog
  link in the nav + footer of all primary pages and a docs landing card, `/changelog/`
  added to llms.txt.
- Version sweep 0.3.3 -> 0.4.1 across all 31 footer badges + 5 softwareVersion schemas +
  llms.txt + install prose (exact strings only; historical versions in the changelog
  preserved).
- `docs/docs/commands.html`: added Confluence Folders section + `--custom-pipeline`.
- Verified: all pages serve 200 locally, sitemap valid (xmllint), all JSON-LD parses,
  canonical == og:url.

### Known follow-up (not done here)
`docs/docs/commands.html` and the older `docs/blog/*.html` use a stale `jira <verb>`
syntax (e.g. `jira create`) that should be `jira issue <verb>`. Out of scope for the
announcement; worth a separate cleanup pass.

## 2026-06-12 — Confluence v2 Folder API command group (#49)

### Context
Confluence v2 treats folders as a distinct content type with their own endpoints,
but the CLI had no `confluence folder` command, so folder IDs 404'd against
`page get`. Issue #49 asked for get/create/delete.

### Change
- New `crates/cli/src/commands/confluence/folders.rs`:
  - `folder get <ID>` -> GET /wiki/api/v2/folders/{id} (renders full response Value).
  - `folder create --space <KEY> --title <T> [--parent <ID>]` -> POST
    /wiki/api/v2/folders. Resolves the space key to a numeric spaceId first.
  - `folder delete <ID> [--force]` -> DELETE /wiki/api/v2/folders/{id} via
    `delete_no_content` (moves to trash; copy reflects that).
  - Pure `build_folder_payload`; 4 unit tests.
- New `resolve_space_id(ctx, key)` helper in `confluence/utils.rs`
  (GET /wiki/api/v2/spaces?keys=<key> -> results[0].id).
- Wired `Folder(FolderCommands)` group + dispatch in `confluence/mod.rs`.
- Codex-reviewed (no blockers); applied both should-fixes: `get` preserves the full
  response, delete copy says "move to trash" not "permanently delete".
- The `page list` parse bug also referenced in #49 shipped earlier in #56.

## 2026-06-12 — Markdown to ADF for Jira descriptions/comments (#39)

### Context
`--description` (and comment bodies) wrapped the whole string in a single ADF
paragraph, flattening headings/lists/bold. Issue #39 asked the CLI to parse
structure into multi-node ADF.

### Change
- New `crates/cli/src/commands/jira/adf.rs`: `markdown_to_adf(text) -> Value` parses
  CommonMark via `pulldown-cmark` (added, default-features off) into ADF: paragraph,
  heading (attrs.level), bulletList/orderedList (attrs.order) + listItem, codeBlock
  (attrs.language), blockquote, rule, marks strong/em/strike/code/link, hardBreak.
- Removed `plain_text_adf`; repointed description (`build_create_payload`,
  `build_update_payload`, `build_bulk_payload`) and comments (`add_comment`,
  `update_comment`) to `markdown_to_adf`. Plain text still yields a single paragraph.
- Updated `--description` help text (create + update).
- Codex-reviewed; fixed 4 ADF-schema findings: `code` mark only combines with `link`;
  heading downgraded to paragraph inside listItem/blockquote; blockquote flattened
  inside listItem (and nested blockquote); empty listItem/blockquote get an empty
  paragraph; empty-href links stay plain text.
- 16 adf unit tests; full `cargo test` + `cargo clippy` clean.

### Behavior change
`--description` now interprets markdown. Plain text is unaffected (single paragraph),
but text containing markdown syntax (`#`, `-`, `**`, backticks, links) now renders
structured. `--field description=<json>` remains the raw-ADF escape hatch.

## 2026-06-12 — Drop unmaintained proc-macro-error2; fixes Security Audit (#53)

### Context
CI `cargo deny check advisories` failed repo-wide on RUSTSEC-2026-0173
(`proc-macro-error2` unmaintained), pulled transitively via `tabled` ->
`tabled_derive`. This blocked the dependabot dep-bump PR #53 and every other PR.

### Change
- `Cargo.toml`: `tabled = { version = "0.21", default-features = false, features = ["std"] }`.
  The repo only uses `tabled::builder::Builder` and `tabled::settings::Style`, never
  `#[derive(Tabled)]`, so disabling the default `derive` feature drops `tabled_derive`
  and `proc-macro-error2` entirely. Proper removal, not a deny.toml suppression. Also
  takes the tabled 0.20 -> 0.21 bump from #53.
- Verified: `cargo tree -i proc-macro-error2` and `tabled_derive` both empty;
  `cargo deny check advisories` = ok; full `cargo test` + `cargo clippy` clean;
  table render output unchanged.

## 2026-06-11 — Bug fixes: HTTP 204 false errors (#45) + Confluence list parse (#49)

### Context
Issue triage flagged two real defects. #45: mutating Jira/Confluence commands (update,
transition, assign, deletes) exited non-zero with "error decoding response body" even
though the write succeeded, because the API returns HTTP 204 No Content and the client
parsed the empty body as JSON. #49 (trailing note): `confluence page list` failed with
the same parse error because list structs had required `String` fields the v2 API can
omit or null. Codex reviewed the fix design (read-only).

### #45 — central fix in `crates/api/src/lib.rs`
- `request()` success arm now reads body bytes and treats an empty or whitespace-only
  body as JSON `null` before deserializing (was `response.json::<T>()` unconditionally).
- Callers discard the body as `let _: Value`, so they receive `Value::Null` and succeed.
  No call-site changes; fixes all 204-returning endpoints at once.
- Added 3 wiremock tests: 204 PUT → Ok(Null), empty 200 → Ok(Null), JSON 200 still parses.

### #49 — defensive deserialization on Confluence list structs
- `Page` (pages.rs), `Space` (spaces.rs), `SearchResult` (search.rs): non-`id` string
  fields are now `Option<String>` with `#[serde(default)]` (handles both absent AND null;
  plain serde default does not cover explicit null). Rendered via `.as_deref().unwrap_or("")`.

Verification: `cargo test` (all pass, +3 new), `cargo clippy --all-targets` clean.

## 2026-04-21 - Week-4 SEO: breadcrumbs on blog/runbook pages

### Context
Blog and runbook pages had no BreadcrumbList schema, so Google couldn't render breadcrumbs in SERP and didn't have structural signals for `Home > Blog/Runbooks > Post` hierarchy. Codex previously flagged this as worth adding — especially on hierarchical collection pages where it reflects real structure.

### Changes
- [DONE] Added BreadcrumbList JSON-LD to 18 pages (8 blog + 10 runbook), placed before existing Article/HowTo/FAQPage schema blocks
- [DONE] Breadcrumb names extracted from each page's H1 (or title fallback) for accurate labels
- [DONE] All three levels populated: home → section → page

### Verification
- All 18 pages return 200 and parse multiple JSON-LD blocks cleanly
- Blog pages: BreadcrumbList + Article (some also have FAQPage pre-existing)
- Runbook pages: BreadcrumbList + HowTo

---

## 2026-04-21 - Week-4 SEO: deepened hub pages for head-term ranking

### Context
After 3 rounds of SEO work (homepage, install hub, first-party docs), hub pages still ranked pos 30-70 for their canonical terms (`/jira/` pos 64 for "jira cli", `/confluence/` pos 33 for "confluence cli", `/bitbucket/` pos 60 for "bitbucket cli"). Codex's next-priority recommendation was to deepen hub content so these can outrank Appfire, marketplace.atlassian.com, and the github.com/ankit1/jira-cli README.

### Changes
For each of /jira/, /confluence/, /bitbucket/ hub pages:
- [DONE] Added "What is [X] CLI?" intro section (2 substantive paragraphs, links to sibling hubs + /install/ + /docs/auth.html + /docs/commands.html)
- [DONE] Added "When to use a [X] CLI" section with 5 real use-case bullets, each linking to a runbook
- [DONE] Added a semantic `<table>` "command groups" cluster — each row deep-links to /docs/commands.html anchor
- [DONE] Added visible FAQ section (6-7 Qs per page) covering install/auth/custom fields/CQL/bearer vs basic/CI patterns/open-source/DC compatibility — no schema markup per prior codex guidance
- [DONE] Hero now includes "Commands" link pointing to the relevant /docs/commands.html section
- [DONE] All internal links use descriptive anchor text ("Confluence backup runbook", "auth guide", etc.)

### Word count growth
- /jira/: ~700 → ~1344 words
- /confluence/: ~700 → ~1207 words
- /bitbucket/: ~700 → ~1172 words

### Verification
- All 9 canonical URLs return 200
- Word count measured on rendered output
- All command-table anchors match real /docs/commands.html section IDs (#jira-issues, #jira-bulk, #conf-search, #conf-bulk, #bb-repos, #bb-pr, etc.)

---

## 2026-04-18 - Week-3 SEO: first-party docs at /docs/ (codex-reviewed twice)

### Context
Prior codex review flagged "biggest skipped item is first-party docs on atlassiancli.com" — the "Docs" nav link was going to the GitHub README, limiting SEO depth, sitelink potential, and ability to rank for longer-tail docs queries. Built a 3-page docs section (strategy: depth over breadth, avoid thin-content indexing issues).

### Changes
- [DONE] NEW `docs/docs/index.html` — landing page with TOC (guides, product pages, runbooks)
- [DONE] NEW `docs/docs/auth.html` — authentication guide targeting "atlassian cli authentication" (23 GSC impr), "jira cli login" (4), "atlassian cli token" (3). Sections: overview, API tokens, Bitbucket bearer + API token, profiles, env vars & CI/CD, credential storage, troubleshooting
- [DONE] NEW `docs/docs/commands.html` — consolidated command reference with sticky left sidebar TOC. All 4 products covered with subsection groups. HowTo-schema-free (replaced with TechArticle).
- [DONE] Added "Docs" to nav on homepage + 4 hub pages + /install/ (6 pages total)
- [DONE] Homepage footer expanded with /docs/, /docs/auth.html, /docs/commands.html links
- [DONE] `docs/sitemap.xml` — added 3 new URLs
- [DONE] All 4 hub pages: footer "Documentation" link repointed from GitHub README to /docs/
- [DONE] Hub pages (jira, confluence, bitbucket, jsm): `--output json|csv|yaml|table|quiet|markdown` → `--format ...` (20 swaps total; `--output file.json` form preserved where correctly writing to a file)
- [DONE] Hub pages: "Full Documentation" resource cards on bitbucket/jsm now point to specific `/docs/commands.html#` anchors
- [DONE] Added `<main>` landmark to `/docs/` and `/docs/auth.html` (commands.html already had `<main class="doc-content">`)

### Codex-flagged accuracy fixes (second and third passes)
- ENV VARS: rewrote against actual source — `ATLASSIAN_CLI_TOKEN_<PROFILE>`, `ATLASSIAN_API_TOKEN`, `ATLASSIAN_CLI_BITBUCKET_TOKEN_<PROFILE>`, `ATLASSIAN_BITBUCKET_TOKEN`, `BITBUCKET_TOKEN`. Removed false "skip auth login entirely" claim — profile metadata (base_url, email, workspace) still required
- BITBUCKET AUTH: I had Basic/Bearer flipped. Fixed to match actual semantics:
  * Basic auth = Atlassian API tokens (current, requires --email)
  * Bearer auth = repository/workspace/project access tokens (requires --bearer, no --email needed, workspace optional)
  * App passwords: deprecated (Sep 9, 2025 creation ended; Jun 9, 2026 existing tokens disabled)
- GLOBAL FLAGS: `--verbose` → `--debug` (plus added `markdown` format, `--envelope`, `--config`)
- JSM COMMAND NAMES: `jsm servicedesk` → `jsm service-desk` (kebab-case in clap derive), `requesttype` → `request-type`. 13 swaps across commands.html + /jsm/ hub
- JSM COVERAGE: added missing `request-type {list,get,fields,groups}` group + `organization add-user` / `remove-user`. Full JSM section now: service-desk, request-type, request, queue, approval, sla, customer, organization, kb, feedback (10 subcommand groups)
- AUTH SUBCOMMANDS: removed `auth set-default` (hallucinated — doesn't exist). Real surface: login/logout/list/status/whoami/test. Added correct "change default" instructions (re-run `auth login --default` or edit config.yaml)
- BASE URL: all 10 pages now show `--base-url https://your-domain.atlassian.net` (CLI rejects non-HTTPS URLs)
- JIRA HUB: `--base-url your-domain.atlassian.net` → `--base-url https://your-domain.atlassian.net` in quickstart

### Verification
- All 11 URLs return 200: /, /jira/, /confluence/, /bitbucket/, /jsm/, /install/, /docs/, /docs/auth.html, /docs/commands.html, /blog/, /sitemap.xml
- All JSON-LD blocks valid: /docs/ has BreadcrumbList, /docs/auth.html and /docs/commands.html each have BreadcrumbList + TechArticle
- Zero `auth set-default`, zero `jsm servicedesk` (no-hyphen), zero `--verbose`, zero base-url missing-scheme
- Exactly one `<main>` landmark per docs page
- Codex reviews archived:
  * `/Users/macmini/projects/codex/atlassiancli_seo_week3_review_18apr2026.txt`
  * `/Users/macmini/projects/codex/atlassiancli_seo_week3_verify_18apr2026.txt`

### Next priority (per codex)
Hub page content depth — `/jira/`, `/confluence/`, `/bitbucket/` currently rank pos 30-70 for canonical terms. Need more content weight to outrank Appfire, marketplace.atlassian.com, github.com. After that: backlinks (awesome-* lists, Show HN, crates.io polish). Defer blog modifier content and breadcrumbs on old content.

---

## 2026-04-18 - Week-2 SEO: hub pages + install hub (codex-reviewed)

### Context
After week-1 homepage rewrite, focus shifts to product hubs + install funnel. GSC shows `/jira/` ranks pos 64 for "jira cli" (128 impr), `/confluence/` at pos 33 for "confluence cli" (309 impr). Hub nav was missing sibling links (no internal linking between /jira/, /confluence/, /bitbucket/, /jsm/). Install-intent queries (atlassian cli install 21 impr, brew install atlassian cli 15 impr, atlassian cli download 17 impr) had no dedicated landing.

### Changes
- [DONE] `docs/jira/index.html` - nav now has Jira/Confluence/Bitbucket/Blog sibling links, `aria-current="page"` on self
- [DONE] `docs/confluence/index.html` - same nav pattern + fixed 3 broken runbook links (pointed to nonexistent files)
- [DONE] `docs/bitbucket/index.html` - same nav pattern
- [DONE] `docs/jsm/index.html` - same nav pattern + title changed from acronym-first "JSM CLI" to "Jira Service Management CLI (JSM)" (better search behavior match)
- [DONE] `docs/install/index.html` - NEW install hub page (~180 lines): Homebrew, Cargo, prebuilt binary, verify, first-time setup, troubleshooting sections
- [DONE] BreadcrumbList JSON-LD added to 5 pages (4 hubs + install)
- [DONE] Bulk repoint of `/#install` → `/install/` across 23 files (all hub, blog, runbook pages)
- [DONE] `docs/sitemap.xml` - added /install/ entry, bumped 5 pages lastmod to 2026-04-18 (only changed ones)
- [DONE] Homepage footer gained `/install/` link

### Codex feedback incorporated
- Fixed broken Confluence runbook links (`/runbooks/confluence-backup-space.html` etc. → real files `confluence-backup.html`, `confluence-markdown-sync.html`, `confluence-bulk-cleanup.html`)
- Removed "curl" and "Docker" from install page title (page body doesn't have those sections)
- Replaced semantically-wrong HowTo schema (alternative methods modeled as sequential steps) with TechArticle schema
- Repointed all hub Install CTAs to /install/ (were going to homepage #install, weakening new page's internal link signal)

### Verification
- Local server: `/`, `/jira/`, `/confluence/`, `/bitbucket/`, `/jsm/`, `/install/`, `/blog/`, `/sitemap.xml` all return 200
- All 6 main pages have valid JSON-LD blocks
- All 4 hub pages' nav-btn Install button resolves to `/install/`
- Codex review: `/Users/macmini/projects/codex/atlassiancli_seo_week2_review_18apr2026.txt`

### Explicitly deferred (per codex)
- BreadcrumbList on blog/runbook pages (would be Home > Blog > Post; real hierarchy)
- First-party docs on atlassiancli.com (replacing GitHub-only Docs link) — highest-remaining-leverage item
- JSM redirect to /jira/ (codex: keep separate page, small-volume but distinct intent)

---

## 2026-04-18 - Homepage SEO rewrite (week-1, codex-reviewed)

### Context
GSC data: 51 clicks / 5,960 impressions / 0.9% CTR / pos 14.5 over 3 months. Homepage ranks pos 7.3 for "atlassian cli" (2,415 impr) but CTR is 0.79% — title/H1 were slogan-first ("Atlassian Cloud as Code") not query-first. Product cards on homepage were not crawlable (no `<a>` wrapping), so `/jira/`, `/confluence/`, `/bitbucket/`, `/jsm/` got zero internal link juice from the top page.

### Changes
- [DONE] `docs/index.html` — new `<title>`, meta description, og:*, twitter:*, og:site_name
- [DONE] `docs/index.html` — H1 changed from "Atlassian Cloud as Code" → "Atlassian CLI" (query-first)
- [DONE] `docs/index.html` — nav now has Jira/Confluence/Bitbucket/Blog links (removed redundant Docs→GitHub)
- [DONE] `docs/index.html` — product cards wrapped in `<a href="/jira/">` etc. (4 new crawlable links)
- [DONE] `docs/index.html` — product card headings renamed "Jira" → "Jira CLI" etc. for better anchor text
- [DONE] `docs/index.html` — added WebSite JSON-LD with alternateName; updated SoftwareApplication
- [DONE] `docs/index.html` — footer expanded with hub + runbook links
- [DONE] `docs/styles.css` — `.product-card` now `display:block; color:inherit; text-decoration:none` (anchor compatibility)
- [DONE] `docs/llms.txt` — version bumped to 0.3.3

### Verification
- HTML parses cleanly (only false-positive `</link>` which is HTML5 self-closing)
- Both JSON-LD blocks valid JSON; @type=WebSite and @type=SoftwareApplication
- Local server: `/`, `/jira/`, `/confluence/`, `/bitbucket/`, `/jsm/` all return 200
- Raw research data: `/Users/macmini/projects/codex/atlassiancli_seo_18apr2026/`
- Codex review: `/Users/macmini/projects/codex/atlassiancli_seo_week1_review_18apr2026.txt`

### Explicitly not done (per codex review)
- Misspelling hint copy (spammy)
- FAQ schema as SEO hack (Google limits FAQ rich results to gov/health)
- OG image upgrade (not a week-1 lever)
- Sitemap lastmod bump without real content changes

---

## 2026-04-02 - Jira Search API Migration & 410 Error Handling

### Context
Atlassian removed `/rest/api/3/search` (returns 410 Gone per CHANGE-2046). Bulk operations still used old endpoint. API client had no 410 handling — errors fell to generic ServerError with misleading "auth expired" context messages.

### Changes
- [DONE] `crates/cli/src/commands/jira/bulk.rs` — migrated `bulk_export` (line 225) and `search_issue_keys` (line 402) from `POST /rest/api/3/search` to `POST /rest/api/3/search/jql`; converted `fields` from comma-string to array
- [DONE] `crates/api/src/error.rs` — added `EndpointGone` variant with suggestion text, `is_retryable() → false`, unit tests
- [DONE] `crates/api/src/lib.rs` — added `StatusCode::GONE` arm in all 4 HTTP method match blocks
- [DONE] `crates/cli/tests/jira_integration.rs` — updated search mock to new endpoint, added `test_jira_search_410_returns_endpoint_gone` test
- [DONE] `docs/02042026_jira_search_migration.md` — migration documentation

### Notes
- Bug 1 (`auth test` false failure) and Bug 2 (`jira issue search`) were already fixed in v0.3.1 — `auth test` uses `/rest/api/3/myself`, `issue search` uses `/rest/api/3/search/jql`
- Only bulk operations and error handling remained broken

## 2026-03-14 - SEO & Traffic Growth Implementation

### Context
Site was ranking for only 5 keywords (all positions 41-90+) despite target keywords exploding in volume (jira cli: +303% YoY, atlassian cli: +6400% YoY). Root causes: single-page site, zero backlinks, no GSC, no structured data. ChatGPT already drives 8% of traffic without optimization.

### Phase 1: Technical SEO Foundation
- `docs/index.html` — Added JSON-LD SoftwareApplication schema, canonical URL, Twitter card meta tags, blog nav links, version bump to v0.3.0
- `docs/sitemap.xml` — Expanded with lastmod dates, blog index URL, llms.txt URL
- `docs/robots.txt` — Added AI bot directives (GPTBot, ChatGPT-User, Claude-Web, PerplexityBot)
- `docs/llms.txt` — Created AI assistant context file with product info, features, install commands
- `docs/blog/index.html` — Created blog listing page with planned article cards matching site design
- `docs/seo_traffic_growth_plan.md` — Full SEO analysis with DataForSEO data, keyword research, Codex review

### Phase 2: Product Landing Pages — DONE
- [DONE] `docs/jira/index.html` — Jira CLI product page (target: jira cli 2,900/mo)
- [DONE] `docs/confluence/index.html` — Confluence CLI product page (target: confluence cli 480/mo)
- [DONE] `docs/bitbucket/index.html` — Bitbucket CLI product page (target: bitbucket cli 320/mo)
- [DONE] `docs/jsm/index.html` — JSM CLI product page (target: jira service management cli)
- [DONE] `docs/product.css` — shared product page styles

### Phase 3: Runbook Pages — DONE (10 pages from existing scripts)
- [DONE] `docs/runbooks/jira-bulk-transition.html` — from jira/bulk-transition.sh
- [DONE] `docs/runbooks/jira-project-cleanup.html` — from jira/project-cleanup.sh
- [DONE] `docs/runbooks/jira-sprint-report.html` — from jira/sprint-report.sh
- [DONE] `docs/runbooks/confluence-backup.html` — from confluence/backup-space.sh
- [DONE] `docs/runbooks/confluence-bulk-cleanup.html` — from confluence/bulk-cleanup.sh
- [DONE] `docs/runbooks/confluence-markdown-sync.html` — from confluence/doc-pipeline.sh
- [DONE] `docs/runbooks/confluence-space-report.html` — from confluence/space-report.sh
- [DONE] `docs/runbooks/bitbucket-branch-cleanup.html` — from bitbucket/branch-cleanup.sh
- [DONE] `docs/runbooks/bitbucket-pr-automation.html` — from bitbucket/pr-automation.sh
- [DONE] `docs/runbooks/bitbucket-repo-audit.html` — from bitbucket/repo-audit.sh

### Phase 4 (Blog Articles)
- [DONE] Article 2: `docs/blog/atlassian-cli-guide.html` — atlassian-cli deep-dive (target: atlassian cli, 1300/mo, KD: 5). ~1,500 words, covers install, config, product walkthroughs (Jira/Confluence/Bitbucket/JSM), real-world workflows, internal links to product pages and runbooks.
- [DONE] Article 7: `docs/blog/jira-cli-tools-compared.html` — Jira CLI tools comparison (target: jira cli tool, 170/mo, KD: 7). ~1,500 words, comparison table (atlassian-cli vs jira-cli vs go-jira vs ACLI), decision matrix, FAQ with JSON-LD FAQPage schema.
- [DONE] Updated `docs/blog/index.html` — Linked Card 2 and Card 7 to published articles, removed "Coming Soon" hero badge.
- [DONE] Article 1: `docs/blog/jira-cli-complete-guide.html` — Jira CLI Complete Guide (target: jira cli, 2900/mo, KD: 11). ~2,000 words, comparison table, FAQ with JSON-LD FAQPage schema, CI/CD integration section.
- Article 3: Confluence CLI guide (target: confluence cli, 480/mo)
- Article 4: Jira CLI commands cheat sheet (target: jira cli commands, 90/mo, KD: 5)
- [DONE] Article 5: `docs/blog/jira-bulk-operations.html` — Bulk Jira Operations (target: jira bulk operations, KD: 1). ~1,200 words, dry-run pattern, bulk transition/assign/export, concurrency control, sprint cleanup workflow, FAQ with JSON-LD FAQPage schema.
- [DONE] Article 6: `docs/blog/bitbucket-cli-guide.html` — Bitbucket CLI Guide (target: bitbucket cli, 320/mo). ~1,500 words, Basic vs Bearer auth, repo/PR/pipeline/branch operations, real-world CI/CD and audit scripts, FAQ with JSON-LD FAQPage schema.
- [DONE] Updated `docs/blog/index.html` — Linked Card 5 and Card 6 to published articles, changed badges from "Coming Soon" to "Published".
- Article 8: JSM CLI guide (target: jira service management cli)

### Pending: Infrastructure
- Set up Google Search Console for atlassiancli.com
- Set up Bing Webmaster Tools (Bing is #1 search source at 24%)
- Update Cargo.toml homepage to atlassiancli.com (stop authority leakage to GitHub)

---

## 2026-02-20 - Bitbucket Bearer Auth Support & App Password Deprecation

### Context
Bitbucket app passwords deprecated (creation disabled Sep 2025, all disabled Jun 2026).
atlassian-cli only supported Basic auth in CLI paths despite API layer having Bearer support.

### Changes

**Priority 1: Help text & error message updates** (`crates/cli/src/commands/auth.rs`)
- Updated `read_token_from_stdin()` — replaced app password URL with Bitbucket API token instructions
- Updated `test_bitbucket_auth()` failure message — added deprecation hint and bearer suggestion
- Updated `LoginArgs` after_help — added bearer example and deprecation notice
- Updated `list_profiles()` — added `bitbucket_auth` column showing basic/bearer

**Priority 2: Bearer auth support**
- `crates/config/src/lib.rs` — Added `bitbucket_token_type: Option<String>` to `Profile` struct
- `crates/cli/src/commands/auth.rs`:
  - Added `--bearer` flag to `LoginArgs` (requires `--bitbucket`)
  - Made `--email` optional (not required for `--bearer`)
  - Added `is_bitbucket_bearer()` helper
  - Updated `login_bitbucket()` to store token_type in profile
  - Updated `test_bitbucket_auth()` to use `/2.0/workspaces` for bearer tokens
  - Updated `auth_status()` to build bearer client and use correct endpoint
- `crates/cli/src/main.rs`:
  - Added `is_bearer` field to `BitbucketProfile`
  - Updated `resolve_profile_for_bitbucket()` — email optional for bearer
  - Updated `build_bitbucket_client()` — uses `with_bearer_token()` for bearer
  - Passes `is_bearer` through to `bitbucket::execute()`
- `crates/cli/src/commands/bitbucket/utils.rs`:
  - Added `is_bearer` field to `BitbucketContext`
  - Updated `verify_auth()` — uses `/2.0/workspaces` for bearer tokens
- `crates/cli/src/commands/bitbucket/mod.rs` — passes `is_bearer` to context
- `crates/cli/src/commands/bitbucket/workspaces.rs` — `whoami()` shows workspaces for bearer

**Doc fixes**
- `crates/config/src/lib.rs` — fixed "keyring" reference in Profile doc comment to "encrypted credential files"

### Tests Added
- Config: 6 tests for `bitbucket_token_type` (default none, bearer, skip serialization, backwards compat, roundtrip)
- Auth: 4 tests for `is_bitbucket_bearer()` (default, bearer, basic, nonexistent)
- CLI integration: 2 tests (bearer flag in help, bearer profile no email required)

### Key Design Decisions
- Bearer tokens use `/2.0/workspaces` instead of `/2.0/user` (access tokens are not user-scoped)
- `--email` is optional when `--bearer` is set
- Token type stored in profile config (not credential store) for simplicity
- Env vars (`BITBUCKET_TOKEN`) default to whatever `bitbucket_token_type` the profile specifies

---

## 2026-01-31 - Add Pipeline Variable/Secret Management

### Feature
Full CRUD for Bitbucket pipeline variables at 3 scopes: repository, workspace, and deployment environment.

### CLI Commands
```
bb pipeline var list [--workspace-level | --deployment <env>]
bb pipeline var get --key <KEY> [--workspace-level | --deployment <env>]
bb pipeline var create --key <KEY> --value <VAL> [--secured] [--workspace-level | --deployment <env>]
bb pipeline var update --key <KEY> --value <VAL> [--secured | --unsecured] [--workspace-level | --deployment <env>]
bb pipeline var delete --key <KEY> --force [--workspace-level | --deployment <env>]
bb pipeline env list
```

### Key Design Decisions
- Tri-state `--secured`/`--unsecured` on update (neither = preserve current state)
- Key-based operations with transparent UUID resolution
- Environment name-to-UUID resolution (case-insensitive)
- `delete_no_content()` in ApiClient for 204 responses
- `type: "pipeline_variable"` in create/update payloads

### Files Created
- `crates/cli/src/commands/bitbucket/variables.rs` — data structures, CRUD functions, resolution helpers, 13 unit tests

### Files Modified
- `crates/api/src/lib.rs` — added `delete_no_content()` method
- `crates/cli/src/commands/bitbucket/mod.rs` — added VarCommands, EnvCommands, dispatch logic

### Tests
- 13 new unit tests (URL building, deserialization, payload format, scope labels, UUID cleaning, secured display)
- All 310 tests pass, zero clippy warnings

## 2026-01-31 - Fix `--repo` flag for pipeline commands

### Problem
`bb pipeline list --repo genai_images` failed — `repo` was a positional arg on each pipeline subcommand, shadowing the global `--repo` flag from `BitbucketArgs`. Also, 5 of 10 pipeline subcommands panicked on `--help` due to invalid clap positional ordering (optional `repo` before required `pipeline_id`).

### Changes
- Removed positional `repo: Option<String>` from all 10 `PipelineCommands` variants
- Updated all 10 handler match arms to use global `--repo` via `require_repo(None, global_repo.as_deref(), ...)`
- Updated `require_repo()` error message to reference `--repo` flag only

### Result
- `bb pipeline list --repo genai_images` now works
- `bb --repo genai_images pipeline list` works
- `bb pipeline list` auto-detects from git remote
- `bb pipeline get 42` works (pipeline_id as sole positional)
- No more `--help` panics on get/stop/logs/watch/steps

### File Modified
- `crates/cli/src/commands/bitbucket/mod.rs`

## 2026-01-31 - Fix Silent Auth Failure on Search + Related Issues

### Problem
`jira issue search --assignee @me` returns "No issues found" when auth is expired. Jira's API returns HTTP 200 with empty results when permissions-based filtering removes everything. The code printed "No issues found" without validating credentials.

### Changes

1. **Fix ANSI in tracing output** (`crates/cli/src/main.rs`)
   - Added `.with_ansi(std::io::stderr().is_terminal())` to tracing fmt builder
   - Non-TTY output no longer contains ANSI escape codes

2. **Add `Forbidden` variant to ApiError** (`crates/api/src/error.rs`)
   - New `Forbidden { message: String }` variant for 403 responses
   - Shares suggestion with `AuthenticationFailed` (points to `auth test`)
   - Not retryable
   - Added 3 unit tests

3. **Handle 403 in API client** (`crates/api/src/lib.rs`)
   - Added `StatusCode::FORBIDDEN` arm in `request()`, `get_text()`, `get_bytes()`
   - Returns `ApiError::Forbidden` with response body as message
   - Added 4 wiremock tests (403 JSON, 403 text, 403 bytes, 401 auth)

4. **Display error suggestions in main** (`crates/cli/src/main.rs`)
   - Restructured: `main()` → `run()` pattern
   - `main()` catches errors, downcasts to `ApiError`, shows `.suggestion()` on stderr
   - Exits with code 1 on error

5. **Add `verify_auth()` to context types**
   - `JiraContext` → `/rest/api/3/myself` (`crates/cli/src/commands/jira/utils.rs`)
   - `JsmContext` → `/rest/api/3/myself` (`crates/cli/src/commands/jsm/utils.rs`)
   - `ConfluenceContext` → `/wiki/rest/api/user/current` (`crates/cli/src/commands/confluence/utils.rs`)
   - `BitbucketContext` → `/2.0/user` (`crates/cli/src/commands/bitbucket/utils.rs`)
   - `BambooContext` → `/rest/api/latest/info` (`crates/cli/src/commands/bamboo/utils.rs`)

6. **Auth check on empty results** (high-impact commands)
   - `jira issue search` (`crates/cli/src/commands/jira/issues.rs:95`)
   - `confluence search` (`crates/cli/src/commands/confluence/search.rs`)
   - `bitbucket repo list` (`crates/cli/src/commands/bitbucket/repos.rs:73`)
   - `bitbucket pr list` (`crates/cli/src/commands/bitbucket/pullrequests.rs:147`)
   - `bamboo build list` (`crates/cli/src/commands/bamboo/builds.rs:48`)
   - `jsm request list` (`crates/cli/src/commands/jsm/requests.rs:87`)
   - `jsm servicedesk list` (`crates/cli/src/commands/jsm/servicedesk.rs:66`)

### Files Modified
| File | Change |
|------|--------|
| `crates/api/src/error.rs` | Forbidden variant + tests |
| `crates/api/src/lib.rs` | 403 handling in 3 methods + wiremock tests |
| `crates/api/Cargo.toml` | Added wiremock dev-dependency |
| `crates/cli/src/main.rs` | ANSI fix, run() pattern, suggestion display |
| `crates/cli/src/commands/jira/utils.rs` | verify_auth() |
| `crates/cli/src/commands/jira/issues.rs` | Auth check on empty search |
| `crates/cli/src/commands/confluence/utils.rs` | verify_auth() |
| `crates/cli/src/commands/confluence/search.rs` | Auth check + empty handling |
| `crates/cli/src/commands/bitbucket/utils.rs` | verify_auth() |
| `crates/cli/src/commands/bitbucket/repos.rs` | Auth check on empty list |
| `crates/cli/src/commands/bitbucket/pullrequests.rs` | Auth check on empty list |
| `crates/cli/src/commands/bamboo/utils.rs` | verify_auth() |
| `crates/cli/src/commands/bamboo/builds.rs` | Auth check on empty list |
| `crates/cli/src/commands/jsm/utils.rs` | verify_auth() |
| `crates/cli/src/commands/jsm/requests.rs` | Auth check on empty list |
| `crates/cli/src/commands/jsm/servicedesk.rs` | Auth check on empty list |

### Tests
- 7 new tests (3 error.rs unit tests + 4 wiremock integration tests)
- All 226 tests pass

## 2026-01-31 - Merge Dependabot Cargo Deps Bump (PR #29)

### Problem
PR #29 bumped 10 cargo dependencies (clap, tokio, reqwest, serde_json, colored, thiserror, chrono, url, whoami, indexmap). Security Audit failed because `reqwest 0.12→0.13` switched to rustls by default, pulling in `aws-lc-rs` with ISC/OpenSSL licenses and `webpki-root-certs` with CDLA-Permissive-2.0 — none in deny.toml allowlist.

### Changes
- Added `ISC`, `OpenSSL`, `CDLA-Permissive-2.0` to `deny.toml` license allowlist (all permissive, OSI-approved)
- Removed stale `RUSTSEC-2025-0119` ignore (number_prefix no longer in dep tree after updates)

### File Modified
- `deny.toml`

## 2026-01-27 - Website Footer Credit
- Added "Built by Omar Shabab" credit with link to omarshabab.com
- Added `.footer-credit` styling to match existing footer aesthetic
- Files modified: `docs/index.html`, `docs/styles.css`

## 2026-01-25 - CLI UX Improvements from User Feedback

### Issue 1: Rename `--output` to `--format` (global flag)
- Renamed `--output` to `--format` to avoid conflict with subcommand `--output` file path flags
- Made flag global with `global = true` so it works on all subcommands
- Added short flag `-f` for convenience
- File: `crates/cli/src/main.rs:27-29, 81`

### Issue 2: Build number support for `pipeline logs`
- Changed `pipeline_uuid` to `pipeline_id` in Logs command enum
- Added `resolve_pipeline_id()` call before `get_pipeline_logs()`
- Now accepts both build numbers (e.g., `335`) and UUIDs
- File: `crates/cli/src/commands/bitbucket/mod.rs:481-485, 1131-1158`

### Issue 3: Improve 406 error handling for log fetching
- Changed Accept header from `text/plain; charset=utf-8` to `text/plain, */*;q=0.1`
- Added explicit `StatusCode::NOT_ACCEPTABLE` (406) handling
- File: `crates/api/src/lib.rs:202, 224-232`

### Issue 4: Add browser URL to log fetch errors
- Error messages now include browser URL for manual viewing
- URL format: `https://bitbucket.org/{workspace}/{repo}/pipelines/results/{uuid}/steps/{step_uuid}`
- File: `crates/cli/src/commands/bitbucket/pipelines.rs:890-913`

### Files Modified
| File | Change |
|------|--------|
| `crates/cli/src/main.rs` | Rename output→format, add global=true |
| `crates/cli/src/commands/bitbucket/mod.rs` | Logs command uses pipeline_id + resolve |
| `crates/cli/src/commands/bitbucket/pipelines.rs` | Browser URL in error messages |
| `crates/api/src/lib.rs` | Broader Accept header, 406 handling |
| `crates/cli/tests/cli_integration.rs` | Updated test to use --format |

## 2026-01-25 - Fix Homebrew Formula Release Workflow

- Release workflow failing since v0.2.1 due to cargo-dist version mismatch
- Dependabot bumped `actions/checkout@v4` to `@v6`, but cargo-dist expects v4
- Regenerated `.github/workflows/release.yml` via `dist init`
- Re-tagged v0.2.4 to trigger new release build with homebrew formula update
- File: `.github/workflows/release.yml`

## 2026-01-25 - Fix Homebrew Installation Command

- Fixed incorrect Homebrew install command on website
- Changed `omar16100/tap/atlassian-cli` → `omar16100/atlassian-cli/atlassian-cli`
- File: `docs/index.html` (lines 86-87, 350-351)

## 2026-01-19 - UX Improvements

### Issue 1: Auth UX - Hint for `--bitbucket-token`
- Added clap error hook in `main.rs` to detect `--bitbucket-token` typo and suggest correct syntax
- Added `after_help` to `LoginArgs` in `auth.rs` showing examples for Jira and Bitbucket auth
- Files: `crates/cli/src/main.rs`, `crates/cli/src/commands/auth.rs`

### Issue 2: Add `auth status` Command
- New command: `atlassian-cli auth status [--profile <name>] [--configured-only]`
- Shows authentication status for all services: Jira/Confluence, Bitbucket, OpsGenie, Bamboo
- Tests actual API connectivity for Jira and Bitbucket (shows OK/FAILED/N/A)
- OpsGenie and Bamboo show CONFIGURED status (API key present)
- `--configured-only` flag hides N/A entries
- File: `crates/cli/src/commands/auth.rs`

### Issue 3: Pipeline Repo Auto-Detection Error Messages
- Created `require_repo()` helper function for better error messages
- Used across all pipeline commands (list, get, trigger, stop, logs, watch, steps, status, rerun)
- Error now shows: "Repository required for '<cmd>'. Not in a git directory with Bitbucket remote."
- File: `crates/cli/src/commands/bitbucket/mod.rs`

### Issue 4: Build Number vs UUID - Renamed `uuid` to `pipeline_id`
- Renamed `uuid` field to `pipeline_id` in Get, Stop, Watch, Steps commands
- Clarifies that both build numbers and UUIDs are accepted
- Made `resolve_pipeline_id()` public for use in Stop command
- Files: `crates/cli/src/commands/bitbucket/mod.rs`, `crates/cli/src/commands/bitbucket/pipelines.rs`

### Issue 5: Add Commit Hash to Pipeline Output
- Added `commit` field to `PipelineRow`, `PipelineView`, `PipelineStatusOutput` structs
- Shows short hash (7 chars) of the commit that triggered the pipeline
- Added `get_commit_hash()` helper function
- Visible in: `pipeline list`, `pipeline get`, `pipeline status`, `pipeline watch`
- File: `crates/cli/src/commands/bitbucket/pipelines.rs`

### Issue 6: Improve Auth Failure Error Messages
- Improved Bitbucket auth error message to mention `auth list` command
- Shows: "Check token status: atlassian-cli auth list"
- Updated `AuthenticationFailed` suggestion in error.rs to mention both `auth list` and `auth test`
- Files: `crates/cli/src/main.rs`, `crates/api/src/error.rs`

### Files Modified
| File | Changes |
|------|---------|
| `crates/cli/src/main.rs` | Clap error hook, improve Bitbucket auth errors |
| `crates/cli/src/commands/auth.rs` | New Status command, LoginArgs after_help |
| `crates/cli/src/commands/bitbucket/mod.rs` | Rename uuid→pipeline_id, require_repo helper |
| `crates/cli/src/commands/bitbucket/pipelines.rs` | Add commit hash, resolve_pipeline_id public |
| `crates/api/src/error.rs` | Update auth failure suggestion |

## 2026-01-15 - Bug Fixes for JSM, OpsGenie & Bamboo Modules

### Double Rendering Bug Fix
- Fixed 6 locations where `println!()` + early return broke JSON/YAML output
- Files fixed:
  - `crates/cli/src/commands/bamboo/projects.rs:40-45` - list_projects
  - `crates/cli/src/commands/jsm/requests.rs:86-91` - list_requests
  - `crates/cli/src/commands/opsgenie/alerts.rs:57-62` - list_alerts
  - `crates/cli/src/commands/opsgenie/alerts.rs:407-412` - list_recipients
  - `crates/cli/src/commands/opsgenie/alerts.rs:441-446` - list_logs
  - `crates/cli/src/commands/opsgenie/alerts.rs:473-478` - list_notes
- Now always renders via ctx.renderer.render() for consistent output format support

### OpsGenie Query Parameter Bug Fix
- Fixed `crates/cli/src/commands/opsgenie/alerts.rs:14-27`
- When both `--query` and `--status` provided, they were overwriting each other
- Now combines them with " AND " for proper OpsGenie query syntax

## 2026-01-14 - JSM, OpsGenie & Bamboo CLI Implementation

### Phase 1: JSM Enhancement (Refactored to Module)
- Refactored `jsm.rs` → `jsm/` module directory structure
- Created files:
  - `mod.rs` - Main routing with full command enum definitions
  - `utils.rs` - JsmContext, ServiceDesk, RequestType, Customer, Organization types
  - `requesttypes.rs` - list_request_types, get_request_type
  - `customers.rs` - list_customers, get_customer, create_customer, delete_customer
  - `feedback.rs` - list_feedback, get_feedback
  - `knowledgebase.rs` - list_articles, get_article
- Full CRUD for service desk requests with transitions, comments, attachments, participants
- Added `delete_with_body()` method to ApiClient for JSM endpoints requiring body in DELETE

### Phase 2: OpsGenie Implementation (NEW)
- Created `opsgenie/` module with EU/US region support
- Added `GenieKey` authentication to ApiClient
  - New `AuthMethod::GenieKey { api_key }` variant
  - `with_genie_key()` builder method
  - Sets `Authorization: GenieKey {key}` header
- Created files:
  - `utils.rs` - OpsgenieContext, Alert, Incident, Schedule, Team, Escalation, Service, Heartbeat types
  - `alerts.rs` - list, get, create, close, acknowledge, snooze, escalate, assign, add_note, delete, list_recipients, list_logs, list_notes
  - `incidents.rs` - list, get, create, close, resolve, reopen, add_responder, add_note, delete, list_timeline
  - `schedules.rs` - list, get, create, delete, enable, disable, get_on_call, get_timeline, export_ical
  - `teams.rs` - list, get, create, delete, list_members, add_member, remove_member, get_on_call
  - `escalations.rs` - list, get, create, delete
  - `services.rs` - list, get, create, delete
  - `heartbeats.rs` - list, get, create, delete, enable, disable, ping
  - `mod.rs` - Full command enum definitions and routing
- Added config profile fields:
  - `opsgenie_api_key: Option<String>`
  - `opsgenie_region: Option<String>` ("us" or "eu")
- Added `OPSGENIE_API_KEY` and `OPSGENIE_REGION` env var support with fallback to profile

### Phase 3: Bamboo Implementation (NEW)
- Created `bamboo/` module with full CI/CD operations
- Created files:
  - `utils.rs` - BambooContext, Project, Plan, Branch, BuildResult, Agent, DeploymentProject, Environment, Artifact types
  - `projects.rs` - list_projects, get_project
  - `plans.rs` - list, get, enable, disable, favorite, unfavorite
  - `branches.rs` - list, get, create, delete, enable, disable
  - `builds.rs` - list, get, latest, run, stop, logs, comment, add_label, remove_label
  - `deployments.rs` - list_projects, get_project, list_environments, get_environment, list_results, trigger, list_versions
  - `agents.rs` - list, get, enable, disable, capabilities, server_info, queue
  - `artifacts.rs` - list, download
  - `mod.rs` - Full command enum definitions and routing
- Added config profile field:
  - `bamboo_base_url: Option<String>` (falls back to base_url if not set)
- Added `get_bytes()` method to ApiClient for binary artifact downloads

### Phase 4: Cross-Cutting Concerns
- Updated `main.rs`:
  - `resolve_profile_for_opsgenie()` - env var first, then profile config
  - `build_opsgenie_client()` - uses GenieKey auth
  - `resolve_profile_for_bamboo()` - uses bamboo_base_url or base_url
  - `build_bamboo_client()` - uses Basic auth
- Updated `crates/config/src/lib.rs` with new profile fields
- Updated `crates/api/src/lib.rs` with delete_with_body and GenieKey auth

### Files Created
- `crates/cli/src/commands/jsm/*.rs` (12 files)
- `crates/cli/src/commands/opsgenie/*.rs` (9 files)
- `crates/cli/src/commands/bamboo/*.rs` (9 files including artifacts.rs)

### Files Modified
- `crates/api/src/lib.rs` - delete_with_body, GenieKey auth
- `crates/config/src/lib.rs` - opsgenie_api_key, opsgenie_region, bamboo_base_url
- `crates/cli/src/main.rs` - profile resolution and client building for OpsGenie/Bamboo

### Phase 5: Integration Tests
- Created `crates/cli/tests/jsm_integration.rs` (11 tests)
  - Service desk list/get, requests CRUD, request types, customers, comments, transitions, queues
- Created `crates/cli/tests/opsgenie_integration.rs` (14 tests)
  - Alerts CRUD + acknowledge/close, incidents, schedules, teams, services, heartbeats, escalations
  - Tests GenieKey authentication header
- Created `crates/cli/tests/bamboo_integration.rs` (22 tests)
  - Projects, plans, branches, builds, deployments, agents, queue, server info, artifacts
  - Full CRUD coverage for all major endpoints including artifact list/download

### Test Results
- Total: 47 new integration tests
- JSM: 11 tests passed
- OpsGenie: 14 tests passed
- Bamboo: 22 tests passed

## 2026-01-14 - C4 Model Architecture Documentation
- Created `docs/c4model.md` with comprehensive C4 model documentation
- Level 1: System Context diagram showing user, atlassian-cli, and external Atlassian APIs
- Level 2: Container diagram showing 6 Rust crates (cli, api, auth, config, output, bulk)
- Level 3: Component diagrams for each crate detailing internal structure
- Level 4: Code-level class diagram showing key structs
- Added data flow diagrams: authentication flow, bulk operation flow, request lifecycle
- Converted all diagrams from Mermaid to ASCII art for terminal compatibility
- Files created: `docs/c4model.md`

## 2025-12-26 - Confluence Draft Publishing Fix

### Bug Fixed
- **Critical**: Fixed version handling for draft page/blog post publishing
  - Root cause: `update_page()` and `update_blogpost()` always incremented version by 1
  - When publishing drafts (status: "draft" → "current"), Confluence API requires version 1
  - CLI was sending version 2, causing 400 error: "Version number must be 1 when publishing a page for the first time"

### New Commands
- `confluence page publish <PAGE_ID> --body <FILE> [--title <TITLE>] [--message <MSG>]`
  - Publishes a draft page for the first time
  - Requires `--body` flag with content file
  - Validates page is actually a draft before publishing
  - Sends version 1 as required by Confluence API

- `confluence blog publish <BLOGPOST_ID> --body <FILE> [--title <TITLE>] [--message <MSG>]`
  - Same functionality for blog posts

### Enhanced Commands
- `confluence page update` - Added new flags:
  - `--status <current|draft>` - Target status for the page
  - `--message <MSG>` - Version message for audit trail
  - Smart version handling based on status transition

- `confluence blog update` - Same enhancements as page update

### Version Logic
```
draft → current: version stays at 1 (first publish)
draft → draft: version unchanged (draft update)
current → current: version increments by 1 (normal update)
current → draft: error (cannot unpublish)
```

### Error Handling
- Improved `BadRequest` suggestions in `crates/api/src/error.rs`:
  - Detects "Version number must be 1" errors and suggests using `page publish`
  - Detects generic version conflicts and suggests fetching latest

### Files Modified
- `crates/cli/src/commands/confluence/pages.rs` - Version logic fix, added `publish_page()`, `publish_blogpost()`
- `crates/cli/src/commands/confluence/mod.rs` - Added `Publish` subcommands, `--status`, `--message` flags
- `crates/api/src/error.rs` - Improved error suggestions
- `crates/cli/tests/confluence_integration.rs` - Added 3 new tests for draft publishing

### Documentation
- `docs/26122025.md` - Full implementation plan and API reference

### Tests Added
- `test_publish_draft_page` - Verifies draft page publishes with version 1
- `test_update_published_page_increments_version` - Verifies normal updates increment version
- `test_publish_draft_blogpost` - Verifies draft blog post publishes with version 1

## 2025-12-22 - Remove Codecov Integration
- Removed Codecov upload from CI workflow
  - Coverage job was generating reports successfully but upload failing (no token)
  - Removed `codecov/codecov-action@v4` step from `.github/workflows/ci.yml`
  - Removed entire coverage job (lines 43-57)
  - CI time improved by ~6m31s per run
- Updated branch protection documentation
  - Removed `coverage` from required status checks in `.github/BRANCH_PROTECTION.md`
  - Added comment explaining removal rationale
- Note: Coverage can still be generated locally
  - Developers can install: `cargo install cargo-llvm-cov`
  - Generate HTML report: `cargo llvm-cov --workspace --html`
  - View report: `open target/llvm-cov/html/index.html`
- Files modified: `.github/workflows/ci.yml`, `.github/BRANCH_PROTECTION.md`, `todo.md`

## 2025-12-22 - Week 5-6: Quality Gates & Property Testing
- Week 5 - Branch Protection & Quality Gates:
  - Created `.github/CODEOWNERS` for automatic review requests
    - Default owner: @omar16100 for all files
    - Security-sensitive: auth crate, deny.toml, CI workflows require approval
  - Created `.github/BRANCH_PROTECTION.md` documenting required settings
    - PR reviews required (1 approval, Code Owner approval)
    - Status checks: fmt, clippy, test (ubuntu + macos) # coverage removed 2025-12-22
    - Linear history enforced (no merge commits)
    - Force push disabled, includes administrators
  - Created GitHub templates:
    - `.github/ISSUE_TEMPLATE/bug_report.md` - Bug report template
    - `.github/ISSUE_TEMPLATE/feature_request.md` - Feature request template
    - `.github/pull_request_template.md` - PR checklist and guidelines

- Week 6 - Property Testing:
  - Added `proptest` 1.4 to workspace dependencies
  - Added proptest dev-dependency to CLI crate
  - Created property tests for JQL query builder (`crates/cli/src/query/jql.rs`):
    - `escape_never_panics`: Any string can be safely escaped
    - `escaped_strings_are_quoted`: Escaped output always quoted
    - `escaping_increases_or_maintains_length`: Length preservation
    - `no_unescaped_quotes_in_output`: No injection vulnerabilities
    - `builder_with_condition_produces_output`: Non-empty with conditions
    - `multiple_conditions_use_and`: Proper AND joining
    - `in_list_never_panics`: IN lists handle arbitrary values
  - Created property tests for URL params builder (`crates/cli/src/query/url_params.rs`):
    - `encoding_never_panics`: Any key/value can be encoded
    - `no_unencoded_special_chars`: Special chars properly encoded
    - `special_chars_encoded`: &, =, #, + are percent-encoded
    - `multiple_params_separated`: Multiple params use & separator
    - `optional_none_excluded`: None values don't appear in output
    - `empty_builder_empty_output`: Empty builder produces empty string
  - All 13 property tests passing (100 cases each by default)
  - Property tests verify security against injection attacks

- Files created:
  - `.github/CODEOWNERS`
  - `.github/BRANCH_PROTECTION.md`
  - `.github/ISSUE_TEMPLATE/bug_report.md`
  - `.github/ISSUE_TEMPLATE/feature_request.md`
  - `.github/pull_request_template.md`

- Files modified:
  - `Cargo.toml` - added proptest workspace dependency
  - `crates/cli/Cargo.toml` - added proptest dev-dependency
  - `crates/cli/src/query/jql.rs` - added property_tests module
  - `crates/cli/src/query/url_params.rs` - added property_tests module

## 2025-12-22 - Week 3: Performance Benchmarking
- Added criterion benchmark framework:
  - Added `criterion` to workspace dependencies with features: html_reports, async_tokio
  - Configured for async benchmarks and HTML report generation

- Bulk operations benchmarks (`crates/bulk/benches/bulk_benchmarks.rs`):
  - Concurrency levels: Tests performance with 1, 4, 8, 16 concurrent tasks
  - Task counts: Benchmarks different batch sizes (10, 50, 100, 200 items)
  - Progress bar overhead: Measures impact of progress display on performance
  - All benchmarks use realistic async task simulation (100μs per task)

- API benchmarks (`crates/api/benches/api_benchmarks.rs`):
  - Rate limiter concurrent access: Tests mutex contention with 1-16 parallel threads
  - Pagination logic: Benchmarks has_next() and next_start() calculations
  - Page processing: Tests different page sizes (10, 50, 100, 500 items)
  - Measures pagination state management overhead

- Auth/encryption benchmarks (`crates/auth/benches/auth_benchmarks.rs`):
  - Key derivation: Benchmarks Argon2 key derivation from machine ID
  - Encryption/decryption: Tests AES-256-GCM with different payload sizes
  - Roundtrip performance: Measures full encrypt-decrypt cycles
  - Token sizes: Realistic benchmarks for short (32B), medium (64B), long (128B), JWT (512B) tokens
  - Establishes baseline for security-critical operations

- Benchmark configuration:
  - Added `criterion` dev-dependency to bulk, api, and auth crates
  - Configured `[[bench]]` targets with `harness = false` in all Cargo.toml files
  - Benchmarks run with: `cargo bench --bench <name>`
  - HTML reports generated in `target/criterion/`

- Usage:
  ```bash
  cargo bench --bench bulk_benchmarks
  cargo bench --bench api_benchmarks
  cargo bench --bench auth_benchmarks
  cargo bench  # Run all benchmarks
  ```

- Files created:
  - `crates/bulk/benches/bulk_benchmarks.rs`
  - `crates/api/benches/api_benchmarks.rs`
  - `crates/auth/benches/auth_benchmarks.rs`

- Files modified:
  - `Cargo.toml` - added criterion workspace dependency
  - `crates/bulk/Cargo.toml` - benchmark config
  - `crates/api/Cargo.toml` - benchmark config
  - `crates/auth/Cargo.toml` - benchmark config

## 2025-12-22 - Week 2: Security & Coverage
- Coverage tracking (REMOVED 2025-12-22 - see removal entry above):
  - Added `coverage` job to CI workflow in `.github/workflows/ci.yml`
  - Uses `cargo-llvm-cov` to generate LCOV coverage reports
  - Uploads to Codecov for tracking and visualization
  - Runs on every PR and main branch push
  - Status: Removed - upload was failing, no token configured

- Security scanning:
  - Created `.github/workflows/security.yml` for automated security audits
  - Runs weekly on Monday + on every PR and main push
  - Uses `cargo-audit` for vulnerability scanning (warnings only, non-blocking)
  - Uses `cargo-deny` for license compliance, dependency bans, and advisory checks

- Cargo-deny configuration:
  - Created `deny.toml` with license allowlist
  - Allowed licenses: MIT, Apache-2.0, BSD-3-Clause, MPL-2.0, Unicode-3.0
  - Configured advisory ignores for 4 unmaintained transitive dependencies:
    - backoff 0.4.0 (RUSTSEC-2025-0012) - monitoring for replacement
    - instant 0.1.13 (RUSTSEC-2024-0384) - dependency of backoff
    - number_prefix 0.4.0 (RUSTSEC-2025-0119) - dependency of indicatif
    - proc-macro-error 1.0.4 (RUSTSEC-2024-0370) - dependency of tabled
  - Multiple versions warning level (not error)

- Automated dependency updates:
  - Created `.github/dependabot.yml` for weekly dependency updates
  - Monitors both Rust crates and GitHub Actions
  - Groups all production dependencies together
  - Limits to 5 open PRs at a time

- Files modified: `.github/workflows/ci.yml`, `.github/workflows/security.yml`
- Files created: `deny.toml`, `.github/dependabot.yml`

## 2025-12-22 - Week 1 Prevention: Pre-commit Hooks & CI Optimization
- Immediate fixes (Day 1):
  - Fixed version test in `crates/cli/tests/cli_integration.rs:15` to use `env!("CARGO_PKG_VERSION")`
  - Synced `.release-please-manifest.json` from "0.1.9" to "0.2.0"
  - Ran `cargo clippy --fix` and `cargo fmt` - all 187 tests passing

- Pre-commit hooks with cargo-husky:
  - Added `cargo-husky` to workspace dependencies in `Cargo.toml`
  - Added `cargo-husky` to cli crate dev-dependencies in `crates/cli/Cargo.toml`
  - Created `.cargo-husky/hooks/pre-commit` script with fmt, clippy, and unit test checks
  - Hooks auto-install on `cargo build` (zero friction for contributors)

- CI optimization:
  - Replaced sequential job with parallel jobs (fmt, clippy, test) in `.github/workflows/ci.yml`
  - Upgraded from deprecated `actions-rs/toolchain` to `dtolnay/rust-toolchain@stable`
  - Added `Swatinem/rust-cache@v2` for dependency caching
  - Added matrix testing (ubuntu-latest + macos-latest)
  - Added `fail-fast: false` to show all failures
  - Expected CI time reduction: 2-3min → 60-90s

- Developer tooling:
  - Added `pre-commit`, `quick-check`, and `ci` targets to `Makefile`
  - Added `pre-commit`, `quick-check`, and `ci` targets to `justfile`
  - Created `CONTRIBUTING.md` with pre-commit workflow documentation

- Files modified: `crates/cli/tests/cli_integration.rs`, `.release-please-manifest.json`, `Cargo.toml`, `crates/cli/Cargo.toml`, `.github/workflows/ci.yml`, `Makefile`, `justfile`
- Files created: `.cargo-husky/hooks/pre-commit`, `CONTRIBUTING.md`

## 2025-12-15 (v7) - Add cargo-release for automated version bumping
- Added `cargo-release` configuration to workspace Cargo.toml
  - `shared-version = true` - all crates share the same version
  - `tag-name = "v{{version}}"` - creates tags like `v0.1.8`
  - `pre-release-commit-message = "chore: bump version to {{version}}"`
- Added `[package.metadata.release] release = false` to internal crates (api, auth, config, output, bulk)
- Added `[package.metadata.release] release = true` to CLI crate
- Usage: `cargo release patch --execute` (or `minor`, `major`)
- Files modified: `Cargo.toml`, `crates/*/Cargo.toml`

## 2025-12-15 (v6) - Code Review Fixes
- Fixed progress bar panic risk in `crates/bulk/src/lib.rs:267`
  - Changed `.unwrap()` to `.unwrap_or_else()` with fallback to `ProgressStyle::default_bar()`
  - Logs warning when template is invalid
- Added credential corruption warnings in `crates/auth/src/lib.rs`
  - `set_secret()` and `delete_secret()` now log warnings when JSON parsing fails
  - Previously silently returned empty HashMap on parse errors
- Added rate limiter timeout protection in `crates/api/src/ratelimit.rs`
  - All mutex lock calls now use 5-second timeout via `tokio::time::timeout()`
  - Prevents indefinite blocking if lock is held
  - Methods gracefully degrade: `update_from_response()` skips update, `check_limit()` returns None, `get_info()` returns empty info
- Fixed HTTP client connection pool loss in `crates/cli/src/commands/confluence/attachments.rs`
  - Added `http_client()` method to `ApiClient` to expose underlying reqwest client
  - `upload_attachment()` and `download_attachment()` now reuse connection pool instead of creating new clients
- Added user-visible pagination warning in `crates/cli/src/commands/confluence/bulk.rs`
  - `search_page_ids()` now shows `eprintln` warning when results hit 1000 limit
  - Previously only logged to tracing (not visible to users)
- Files modified: `crates/bulk/src/lib.rs`, `crates/auth/src/lib.rs`, `crates/api/src/lib.rs`, `crates/api/src/ratelimit.rs`, `crates/cli/src/commands/confluence/attachments.rs`, `crates/cli/src/commands/confluence/bulk.rs`

## 2025-12-15 (v5) - Fix Bitbucket-only profile auth flow
- Fixed: Bitbucket-only profiles were rejected because `resolve_active_profile()` required base_url + Jira token
- Root cause: `main.rs:164-220` called same resolution function for all commands
- Solution: Split profile resolution into two functions:
  - `resolve_profile_for_product()` - Jira/Confluence/JSM (requires base_url + token)
  - `resolve_profile_for_bitbucket()` - Bitbucket (requires only email + bitbucket token)
- Shared validation via `BaseProfile` struct and `resolve_base_profile()` helper to avoid duplication
- Changed structs: `ActiveProfile` replaced with `BaseProfile`, `ProductProfile`, `BitbucketProfile`
- Improved error messages: distinguishes "no Bitbucket token (only Jira token found)" vs "no token at all"
- Updated command dispatch to call appropriate resolution function per command type
- Bitbucket profile resolution falls back to general token if no bitbucket-specific token exists
- Added regression tests:
  - `test_bitbucket_only_profile_no_base_url_error` - verifies Bitbucket commands work without base_url
  - `test_jira_still_requires_base_url` - verifies Jira commands still require base_url
- Fixed missing `tracing` dependency in `crates/auth/Cargo.toml` (pre-existing issue)
- Files modified: `crates/cli/src/main.rs`, `crates/cli/tests/cli_integration.rs`, `crates/cli/Cargo.toml`, `crates/auth/Cargo.toml`

## 2025-12-15 (v4) - CLI UX Consistency Refactoring
- Phase 1: Quick fixes
  - Standardized `--limit` defaults to 25 across all products (was 50 in Jira/Confluence)
  - Fixed emoji inconsistency: all `✓` changed to `✅` in Bitbucket files (15 instances)
  - Verified snake_case flag was NOT an issue (clap auto-converts to kebab-case)
  - Files: `jira/mod.rs`, `confluence/mod.rs`, all `bitbucket/*.rs` files

- Phase 2: Output format compliance
  - Created `crates/cli/src/commands/common.rs` with `MutationResult` struct and `render_success()` helper
  - Updated 83+ mutation commands to respect `--output json/yaml/csv/quiet` flags
  - Success messages now render as JSON/YAML/etc when appropriate output format specified
  - Table format still shows emoji messages for human readability
  - Quiet format outputs just the ID when available
  - Files modified: all mutation commands across `jira/*.rs`, `bitbucket/*.rs`, `confluence/*.rs`

- Phase 3: Consistency fixes
  - Standardized confirmation messages to single-line format: `"⚠️  This will permanently delete {resource} {id}. Use --force to confirm."`
  - Standardized empty result messages to `"No {resources} found"` pattern
  - Added user-facing println for all empty results (previously only tracing)
  - Files modified: `jira/issues.rs`, `jira/projects.rs`, `jira/bulk.rs`, `bitbucket/repos.rs`, `bitbucket/pullrequests.rs`, `bitbucket/branches.rs`, `bitbucket/webhooks.rs`, `bitbucket/workspaces.rs`, `bitbucket/permissions.rs`, `bitbucket/pipelines.rs`, `bitbucket/commits.rs`, `confluence/bulk.rs`, `jsm.rs`

- Phase 4: Remove short flags from Jira (BREAKING)
  - Removed short flags `-a`, `-s`, `-y`, `-l`, `-t`, `-p` from Jira search command
  - Now consistent with Bitbucket/Confluence which use long flags only
  - File: `jira/mod.rs`

- Phase 5: Jira command restructure (BREAKING)
  - Moved flat issue commands under `jira issue` subcommand
  - Old: `jira search`, `jira get`, `jira create`, `jira update`, `jira delete`, `jira transition`, `jira assign`, `jira unassign`
  - New: `jira issue search`, `jira issue get`, `jira issue create`, etc.
  - Nested commands also moved: `jira watchers` → `jira issue watchers`, `jira links` → `jira issue links`, `jira comments` → `jira issue comments`
  - Now consistent with Bitbucket (`bb repo`, `bb pr`) and Confluence (`confluence page`, `confluence space`) patterns
  - File: `jira/mod.rs`

- Phase 6: Help text improvements
  - Added `long_about` with usage examples to key commands
  - Jira: `issue search`, `issue create` with JQL and filter flag examples
  - Bitbucket: `repo list`, `repo get`, `repo create` with workspace and flag examples
  - Confluence: `space list`, `space get`, `space create`, `search cql`, `search text`, `search in-space`, `search params` with CQL and filter examples
  - Improved argument descriptions with format examples (e.g., "Space key (e.g., TEAM)")
  - Removed short flags from Confluence search params for consistency with other products
  - Files: `jira/mod.rs`, `bitbucket/mod.rs`, `confluence/mod.rs`

## 2025-12-15 (v3)
- Code review fixes for Bitbucket auth flow
  - Added `BITBUCKET_API_URL` constant in `crates/auth/src/lib.rs:10`
  - Exported `get_token()` and `get_bitbucket_token()` as public functions
  - Fixed docstring: clarified no fallback to general token
  - Removed duplicate token lookup in `main.rs`, now uses `auth::get_bitbucket_token()`
  - Made logging levels consistent (all use `debug!` for non-existent token deletion)
  - Added `--all` flag to `auth list` to show all profiles including inactive
  - Improved error messages with env var hints
  - Added unit tests for `token_key()`, `bitbucket_token_key()`, `BITBUCKET_API_URL`
  - Files modified: `crates/auth/src/lib.rs`, `crates/cli/src/commands/auth.rs`, `crates/cli/src/main.rs`

## 2025-12-15 (v2)
- Enhanced Bitbucket pipeline step info in CLI
  - Added step UUID, started, completed, duration, logs_url to `StepInfo` struct
  - Added `started_on`, `completed_on`, `duration_in_seconds` fields to `PipelineStep` API struct
  - Added `format_duration_secs()` helper for human-readable duration formatting
  - Updated `fetch_steps()` to populate new fields with `include_details` parameter
  - Added `--steps` flag to `bb pipeline list` command to show step summary per pipeline
  - Added `bb pipeline steps <repo> <uuid>` command for dedicated step listing
  - Files modified: `crates/cli/src/commands/bitbucket/pipelines.rs`, `crates/cli/src/commands/bitbucket/mod.rs`

## 2025-12-15
- Improved Bitbucket auth flow with separate token storage
  - Added `bitbucket_token_key()` function in `crates/auth/src/lib.rs:15`
  - Bitbucket tokens stored with `{profile}_bitbucket` key in credentials file

- Added `--bitbucket` flag to `auth login`
  - File: `crates/cli/src/commands/auth.rs:110`
  - Stores Bitbucket token separately from Jira token
  - Added `--workspace` flag for Bitbucket workspace config
  - Shows Bitbucket app password URL when in Bitbucket mode

- Added `--bitbucket` flag to `auth test`
  - File: `crates/cli/src/commands/auth.rs:88`
  - Tests against Bitbucket API `/2.0/user` endpoint

- Added `--bitbucket` flag to `auth logout`
  - File: `crates/cli/src/commands/auth.rs:126`
  - Removes only Bitbucket token when flag is set

- Updated `auth list` to show Bitbucket status
  - File: `crates/cli/src/commands/auth.rs:286`
  - Now shows `has_jira_token`, `has_bitbucket_token`, `workspace` columns
  - Only shows profiles with at least one active token

- Updated Bitbucket token lookup to include credentials file
  - File: `crates/cli/src/main.rs:224`
  - Priority: env vars → credentials file (`{profile}_bitbucket` key)

## 2025-11-26
- Fixed `auth whoami` runtime panic (same nested runtime issue)
  - Made `whoami` async in `crates/cli/src/commands/auth.rs:222`
  - Removed nested tokio runtime, now uses existing runtime via `.await`

- Added API token URL hint to `auth login` flow
  - File: `crates/cli/src/commands/auth.rs:228`
  - Now shows "You can get the API token from: https://id.atlassian.com/manage-profile/security/api-tokens" before prompting for token

- Fixed `auth test` runtime panic ("Cannot start a runtime from within a runtime")
  - Made `auth::handle` async in `crates/cli/src/commands/auth.rs:88`
  - Made `test_auth` async in `crates/cli/src/commands/auth.rs:286`
  - Removed nested tokio runtime, now uses existing runtime via `.await`
  - Updated `main.rs:122` to await auth::handle

- Added `bitbucket whoami` command to verify Bitbucket authentication
  - Added `Whoami` variant to `BitbucketCommands` enum in `crates/cli/src/commands/bitbucket/mod.rs:77`
  - Added `whoami()` function calling `/2.0/user` endpoint in `crates/cli/src/commands/bitbucket/workspaces.rs:312`
  - Displays username, display name, account ID, UUID

- Added hidden password input + file-based credential storage (removed keychain)
  - Token input now hidden via `rpassword` crate
  - Removed `keyring` dependency entirely
  - Tokens stored only in `~/.atlassian-cli/credentials` with 600 permissions
  - Token lookup: env var → credentials file
  - Removed `CredentialStore` struct, simplified auth code

## 2026-04-14 — Jira custom field support (`--field`) landed (PR #40)

- Added `--field key=JSON_VALUE` to `jira issue create`/`update` and `custom_fields` map to `bulk import` rows.
- Duplicate `--field` keys hard-error.
- Collision check rejects raw keys colliding with reserved (`project`, `issuetype`, `summary`) or already-set typed flags (`--description`, `--assignee`, `--priority`, `--labels` in bulk).
- Payload assembly extracted to `build_create_payload` / `build_update_payload` / `build_bulk_payload` for unit-testability.
- New doc: `docs/14042026_jira_custom_fields.md`. README example updated. CLI help integration test added.
- Co-authored with @thereisnotime (original PR).

## 2026-05-16 — SEO overhaul + OSS-unaffiliated positioning (branch `seo/oss-positioning-overhaul`)

Driven by GA4 (`520368061`) + GSC (`sc-domain:atlassiancli.com`) review. Plan: `/Users/macmini/.claude/plans/check-google-analytics-and-imperative-comet.md`. Codex review: `/Users/macmini/projects/codex/atlassiancli_oss_unaffiliated_positioning_review.txt`.

- Phase 1: ship untracked `docs/install/` + `docs/docs/` (index/auth/commands); refresh all `docs/sitemap.xml` `lastmod` → 2026-05-16; add `/about/`.
- Phase 2: `docs/index.html` — codex-recommended `<title>`/meta/OG/Twitter (no "Unofficial" in title), de-brand hero copy, JSON-LD `name`=`atlassian-cli` + `disambiguatingDescription` + `isAccessibleForFree` + `SoftwareSourceCode`, on-page FAQ, hero unaffiliated line.
- Phase 3: nav anchors → "Jira/Confluence/Bitbucket CLI"; reciprocal exact-match links from blog guides → section pages; shared footer legal+trademark block site-wide.
- Phase 4: deepen `runbooks/confluence-markdown-sync.html` (pos 40); above-the-fold TL;DR on `blog/bitbucket-cli-guide.html` (70% bounce).
- Phase 5: site-wide `Atlassian CLI` → `atlassian-cli` rename (title/OG/JSON-LD/breadcrumb/H1); replace Atlassian-imitating product SVGs with generic glyphs; remove `<meta name="keywords">` (23 files); add `docs/about/index.html`, `docs/SECURITY.md`, `docs/.well-known/security.txt`, repo `SECURITY.md`; README + `docs/llms.txt` parity.
- Decisions: full overhaul; keep `atlassiancli.com` domain, de-risk via naming/disclaimers (no migration).

## 2026-08-10 — Jira attachments (issue #93, branch `feat/jira-attachments`)

Plan: `/Users/macmini/.claude/plans/https-github-com-omar16100-atlassian-cli-binary-rossum.md`. Reviewed by kimi before implementation.

- New `jira attachment` group: `list`, `get`, `download` (single, `--output -` to stdout, bulk via `--issue`/`--dir`), `upload` (multi-file), `delete`.
- Fixed pre-existing bug: `init_tracing` had no `.with_writer`, so tracing-subscriber logged to stdout and could corrupt piped binary output. Now stderr.
- Moved `AttachmentField` -> `JiraAttachment` + `de_id_to_string` + `attachments_markdown` from `issues.rs` (2046 lines) into new `jira/attachments.rs`; `issues.rs` now 1993.
- `safe_filename` reduces server-supplied filenames to one path segment (traversal, control chars, Windows illegal/reserved, 255-byte truncation), with proptest.
- Download refuses to clobber without `--force`, diverging from the Confluence command's silent truncate.
- `clap`'s `requires = "issue"` alone silently ignored `--dir` in single mode, because the required `ArgGroup` already counted as satisfied. Needed an explicit `conflicts_with = "attachment_id"`.
- `issue get --format json` gains `author` and `created` per attachment (additive).
- New doc: `docs/10082026_jira_attachments.md`. README Jira section added; stale Confluence attachment flags in README fixed (they documented `--page-id`/`--id` for positional args).
- Codex review found a real hole: bulk download used the server-supplied attachment `id` unvalidated, both in the content URL and as the dedup filename prefix, so an id of `../../owned` on a colliding filename escaped `--dir`. Now validated per row (bad id fails only its own row), and `unique_download_name` filters the id and stays within the 255-byte limit via `fit_filename`.
- Also from review: `write_bytes` uses `create_new` instead of `exists()` + write, closing a TOCTOU and refusing to follow a planted symlink; new `crates/cli/tests/jira_attachment_e2e.rs` drives the real binary against wiremock (the other tests only exercised `ApiClient`); the credential-leak test now requires auth on the Jira mock so it cannot pass vacuously.

## 2026-08-10 — Raw API passthrough `jira api` (issue #93 alternative, branch `feat/jira-api-passthrough`)

- New `crates/cli/src/commands/api.rs` (product-agnostic, wired into `jira` only for now) and `ApiClient::request_raw` / `RawRequest` / `RawResponse` / `resolve_url` in `crates/api`.
- `request_raw` returns non-2xx as `Ok` so the API's own error body survives, and retries 429/5xx only for idempotent methods. The existing `request` retries POSTs, which can double-create. It cannot reuse `retry_with_backoff`: that closure must signal retryable outcomes as `Err`, which would discard the `RawResponse` needed on the final attempt.
- Output bypasses `OutputRenderer::render`. `--format table/csv/markdown/quiet` are ignored because a top-level JSON array would render as a table and silently drop nested fields.
- `--force` gates DELETE only (several Jira read endpoints are POST). Exits 1 rather than returning Ok like `jira issue delete`, so it cannot silently no-op in a script.
- Deferred: `--field`/`--raw-field` (name collides with `jira issue create --field`, which requires strict JSON) and `--paginate` (Jira Cloud has several pagination shapes).
- New doc: `docs/10082026_raw_api_passthrough.md`. README raw-API block added.
- Codex review of the passthrough found two blockers, both fixed: (1) `safe_join` compared scheme and host but **not port**, so any other port on the same host (e.g. another local service on a localhost profile) received the profile's credentials. This was pre-existing and affected every command; now a shared `same_origin` helper compares scheme + host + `port_or_known_default`. (2) `request_raw` used the default redirect policy, so a same-origin endpoint could bounce a credentialed, body-carrying request anywhere: 307/308 replay the body and custom `-H` headers are not stripped. `request_raw` now uses a separate client whose policy follows same-origin redirects only and returns the 3xx otherwise. The ordinary client still follows cross-host redirects, which attachment downloads depend on (guarded by a test).
- Also: `--output` refuses to clobber without `--force` and uses `create_new` (no symlink follow); 429 retries respect `Retry-After`; binary detection now rejects control bytes, not just invalid UTF-8, so a response cannot drive the terminal.
- Consequence: `jira api /rest/api/3/attachment/content/<id>` no longer resolves the media-host hop. Documented, with users pointed at `jira attachment download`.

## 2026-08-10 — Release 0.5.0

Minor bump (not patch): two new command groups plus a new public API on the shared `atlassian-cli-api` crate.

- `jira attachment` group: list/get/download (single, `--output -`, bulk `--issue`/`--dir`)/upload/delete (#93, PR #94).
- `jira api` raw authenticated passthrough (#93 alternative, PR #96).
- `ApiClient::request_raw` / `RawRequest` / `RawResponse` / `resolve_url` added.
- Security fixes shipped along the way, both pre-existing and affecting all commands: `init_tracing` logged to stdout, which could corrupt piped binary output; `safe_join` compared scheme and host but not port, so another port on the same host could receive the profile's credentials.

## 2026-08-10 — Documentation freshness pass (branch `docs/index-and-0.5.0-freshness`)

Audit after the 0.5.0 release, prompted by "is everything documented?".

- Created `docs/index.md`, which never existed. It lists every doc, the `DDMMYYYY_topic.md` vs `topic.md` naming convention, required sections per category, and the rule that a dated feature doc carries a status line kept current.
- Un-staled both 0.5.0 docs: they still said "implemented on <branch>" for branches deleted at merge.
- Corrected the test counts in the passthrough doc (26/5/11 claimed vs 28/10/14 actual) and rewrote its Safety section, which predated the port and redirect fixes.
- `docs/status.md` Jira row still claimed 11 wiremock tests and no attachments; now 21 wiremock plus 23 end-to-end.
- Fixed 19 broken `--output json` invocations across all 9 example scripts. The flag is `--format`; `--output` is not an argument of any of those read commands. Verified each affected subcommand's help. In `backup-space.sh` the error was swallowed by `2>/dev/null || echo "[]"`, so that backup silently saved zero attachments.
- Verified every `atlassian-cli <product> <group> <sub>` path used in the examples resolves against the built binary.
- Extended the audit to the README after finding the example-script rot: **45 of its 128 command examples did not parse**. Causes: Jira issue commands never updated after the `issue` group was introduced (`jira get` -> `jira issue get`), `--id`/`--project`/`--space`/`--cql`/`--query` for arguments that are positional, `confluence bulk` documented as space-driven when it is CQL-driven, `space add-permission`/`page add-restriction` documented with flags that do not exist, pipelines taking `--repo` rather than a positional, and `jsm servicedesk` vs `jsm service-desk`. All fixed and verified.
- Found and fixed a defect in the just-released 0.5.0: `jira api -X PUT` was rejected, because `ValueEnum` derives lowercase variant names and only `-X put` parsed. The README and the command's own help both showed uppercase. Now `ignore_case`.
- New `crates/cli/tests/docs_examples.rs` parses every README command against the built binary (unroutable host, so nothing leaves the process; clap exit code 2 means malformed). Spawned concurrently: 7s instead of 100s. Verified it fails when a command line is broken.

## 2026-08-10 — Release 0.5.1

Patch for a defect shipped in 0.5.0 plus the documentation repair.

- `jira api -X PUT` was rejected; only lowercase `-X put` parsed, while the README and the command's own help showed uppercase.
- `docs/index.md` added; 0.5.0 docs un-staled; `docs/status.md` refreshed.
- 19 broken `--output json` invocations across all 9 example scripts and 45 broken README command examples fixed, with `tests/docs_examples.rs` added so they cannot rot silently again.
