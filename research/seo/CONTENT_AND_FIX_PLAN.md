# atlassiancli.com — Content & Fix Plan (2026-07-10)

Driven by live GA4 (`520368061`) + GSC (`sc-domain:atlassiancli.com`) + DataForSEO + Reddit data.
Data saved under `research/seo/`. Branch: `seo/content-expansion-50-blogs`. Deploy = merge to `main` (`/docs` → GitHub Pages → atlassiancli.com).

## Baseline (measured, last 28d unless noted)
- GSC: ~322 clicks / ~18k impressions / ~1.8% CTR / avg pos ~8.5 (up ~20x clicks vs early-2026 baseline of 51 clicks/3mo).
- Ranked keywords (DataForSEO, US): 22, all page 2+ (pos 11-64), 0 top-10. ETV 13.5/mo.
- Backlinks: **1 referring domain** (robuta.com, spam 35). Effectively greenfield.
- AI/GEO: ChatGPT cites NO authoritative CLI source for "jira cli"/"atlassian cli" — open citation slot.
- GA channels: direct 231, google organic 185 (up from 106 in Mar), **bing organic 13 (down from 128 in Mar)**, AI ~14.

---

## PART A — 5 PRIORITY FIXES

### Fix 1 — Win "jira cli" (top prize: 1,900-2,900/mo, KD 10)
Problem: GSC pos ~10.8 but only 0.35% CTR (863 impr, 3 clicks). DataForSEO shows the **homepage `/`** ranks for it (pos 40-48), NOT `/jira/` → canonical confusion; no single page owns the term.
Actions:
- Make `/jira/index.html` the definitive "Jira CLI" page: title/H1/meta lead with "Jira CLI"; add depth (what it is, install, top commands table, JQL, automation, FAQ); add FAQPage + BreadcrumbList schema.
- Strengthen `/blog/jira-cli-complete-guide.html` as the informational pillar (title "Jira CLI: The Complete Guide (2026)").
- Add internal links with exact anchor text "Jira CLI" → `/jira/` from homepage product card, complete-guide, tools-compared, cheat-sheet.
- Ensure homepage points to `/jira/` as the canonical Jira CLI destination (reduce homepage self-competition).
Target: "jira cli" into top 10; CTR > 2%.

### Fix 2 — CTR rescue: `/blog/jira-cli-tools-compared.html` (4,524 impr, pos 7.9, 0.77% CTR)
Pure CTR play (already ranks page 1). Rewrite `<title>`, meta description, og/twitter, H1/subtitle for click appeal ("free & open source", year, tool count). No content/ranking risk. Keep schema.

### Fix 3 — Hub content depth (`/confluence/` pos 14.9, `/jira/` 19.2, `/jsm/` 19.2)
All stuck page 2. For each hub add: expanded feature sections, a commands table, a "common tasks" list, FAQ (FAQPage schema), 4-6 internal links to relevant runbooks/blogs, a nominative comparison note, refreshed lastmod. Target: page 2 → page 1.

### Fix 4 — Confluence-markdown cluster boost (runbook pos 26.6 GSC / 52-64 DataForSEO)
Ranks for a 210-110/mo cluster (confluence import markdown, markdown import, paste markdown, markdown converter, convert markdown to confluence) but deep. Actions: expand `/runbooks/confluence-markdown-sync.html` (depth + FAQ + keywords in title/meta), and create dedicated cluster blog posts (in Part B: export-confluence-to-markdown, markdown-to-confluence) and interlink into a topic cluster.

### Fix 5 — Bing recovery + measurement (GA: Bing organic 128 → 13)
Bing was the #1 source in Mar and has collapsed. Actions:
- Verify Bing indexing (Bing Webmaster Tools — needs user access; document steps).
- Confirm IndexNow key live (commit 775a141) and pings on publish; resubmit sitemap to IndexNow/Bing.
- Check Bing-specific crawl issues (robots, canonical, sitemap freshness).
- Add all 50 new posts to sitemap + ping IndexNow.

---

## PART B — 50-BLOG CONTENT PLAN

Rules: one DISTINCT primary keyword per post; dev/CLI/API/automation intent only (skip generic product terms like "jira", "jira software", "jira cloud" — wrong intent for a CLI, owned by Atlassian). Volumes = measured DataForSEO US monthly; "LT" = long-tail (Reddit-sourced, unmeasured — labeled honestly, NOT fabricated). Each post: ~1500-1800 words, FAQ schema, legal footer, acli-differentiation, 3-6 internal links, real CLI commands.

Existing (do NOT duplicate): atlassian-cli-guide, jira-cli-complete-guide, jira-cli-tools-compared, jira-cli-commands-cheat-sheet, jira-bulk-operations, bitbucket-cli-guide, confluence-cli-guide, jsm-cli-guide + 10 runbooks.

### Jira cluster (16)
| # | slug | primary keyword | vol | KD | angle |
|---|---|---|---|---|---|
| 1 | jira-rest-api-guide | jira rest api | 1000 | 7 | practical REST API guide w/ curl + CLI |
| 2 | jira-api-from-command-line | jira api | 2900 | 13 | calling the Jira API via CLI/scripts |
| 3 | jira-automation-guide | jira automation | 1300 | 17 | rules vs scripted CLI automation |
| 4 | jira-automation-rules-examples | jira automation rules | 480 | 15 | rule recipes + CLI equivalents |
| 5 | jira-automation-examples | jira automation examples | 70 | 6 | 15 copy-paste automations |
| 6 | jql-query-cheat-sheet | jql query | 480 | 6 | JQL reference + run from CLI |
| 7 | jira-jql-command-line | jira jql | 390 | 14 | run/save JQL from the terminal |
| 8 | jira-command-line-getting-started | jira command line | 70 | 10 | beginner start |
| 9 | jira-branch-names-from-issue-keys | jira issuekey | 260 | 3 | Reddit: auto branch names |
| 10 | jira-epics-from-terminal | jira epic | 1000 | 10 | manage epics via CLI |
| 11 | jira-issue-hierarchy-guide | jira hierarchy | 590 | 2 | hierarchy + querying it |
| 12 | format-jira-markdown-adf | format jira | 320 | 6 | markdown→ADF formatting |
| 13 | create-jira-issues-from-csv | create jira issues from csv | LT | — | Reddit: bulk create |
| 14 | jira-worklog-time-tracking-cli | jira worklog cli | LT | — | log/report time via CLI |
| 15 | jira-webhooks-automation | jira webhooks | LT | — | webhooks + CLI reactions |
| 16 | open-source-jira-tools | open source jira | 260 | 25 | OSS Jira tooling roundup |

### Confluence cluster (10)
| # | slug | primary keyword | vol | KD | angle |
|---|---|---|---|---|---|
| 17 | confluence-rest-api-guide | confluence rest api | — | 9 | REST API w/ examples |
| 18 | confluence-api-from-cli | confluence api | — | 25 | API via CLI |
| 19 | confluence-automation-guide | confluence automation | — | 2 | automate docs via CLI |
| 20 | export-confluence-to-markdown | export confluence to markdown | LT | — | Reddit top angle |
| 21 | markdown-to-confluence | convert markdown to confluence page | 40 | 6 | md→storage format |
| 22 | confluence-markdown-guide | confluence markdown | — | 10 | markdown support overview |
| 23 | confluence-cql-search-cli | confluence cql | LT | — | CQL search from CLI |
| 24 | confluence-space-export-backup | confluence space export | LT | — | backup/export spaces |
| 25 | sync-jira-to-confluence | auto-update confluence from jira | LT | — | Reddit: Jira→Confluence sync |
| 26 | docs-as-code-confluence | mkdocs to confluence | LT | — | Reddit: docs-as-code |

### Bitbucket cluster (9)
| # | slug | primary keyword | vol | KD | angle |
|---|---|---|---|---|---|
| 27 | bitbucket-rest-api-guide | bitbucket rest api | — | 6 | REST API guide |
| 28 | bitbucket-api-from-cli | bitbucket api | — | 26 | API via CLI |
| 29 | bitbucket-cli-commands | bitbucket cli commands | 320 | — | command reference |
| 30 | bitbucket-cli-like-gh | bitbucket cli like gh | LT | — | Reddit #1 pain: gh-for-bitbucket |
| 31 | bitbucket-pipelines-guide | bitbucket pipelines | — | 37 | pipelines + trigger via CLI |
| 32 | bitbucket-pull-request-cli | bitbucket pull request | — | 12 | create/merge PRs from CLI |
| 33 | create-bitbucket-pr-command-line | bitbucket pr | — | 14 | PR create workflow |
| 34 | migrate-bitbucket-to-github | migrate bitbucket to github | LT | — | Reddit: scripted migration |
| 35 | bitbucket-pipelines-to-github-actions | bitbucket pipelines to github actions | LT | — | Reddit: CI migration |

### JSM cluster (7)
| # | slug | primary keyword | vol | KD | angle |
|---|---|---|---|---|---|
| 36 | jsm-cli-service-desk-automation | service desk automation cli | LT | — | JSM automation via CLI |
| 37 | jira-service-management-api | jira service management api | 110 | — | JSM REST API |
| 38 | automate-jira-service-management | what is jira service management | 260 | 23 | what it is + how to automate (CLI hook) |
| 39 | jsm-vs-servicenow-automation | jira service management vs servicenow | 50 | 1 | comparison (automation lens) |
| 40 | jsm-ticketing-automation | jira service management ticketing system | 140 | 21 | ticket automation |
| 41 | jsm-request-management-cli | jsm request cli | LT | — | manage requests from CLI |
| 42 | jsm-queue-sla-reporting-cli | jsm sla reporting | LT | — | queues + SLA reports |

### Cross-product / Atlassian / auth / AI-GEO (8)
| # | slug | primary keyword | vol | KD | angle |
|---|---|---|---|---|---|
| 43 | atlassian-rest-api-guide | atlassian rest api | — | 27 | unified API overview |
| 44 | one-cli-jira-confluence-bitbucket | cli for jira confluence bitbucket | LT | — | Reddit: cross-product CLI |
| 45 | atlassian-api-token-vs-pat | atlassian api token | LT | — | Reddit: auth/PAT setup |
| 46 | acli-vs-atlassian-cli | acli | 90 | — | official acli vs this (differentiation) |
| 47 | rovo-dev-cli-explained | rovo dev cli | LT | — | Reddit: Atlassian's AI CLI |
| 48 | atlassian-cli-github-actions | atlassian cli github actions | LT | — | CI/CD usage |
| 49 | best-atlassian-cli-tools-2026 | atlassian cli tools | LT | — | GEO listicle (AI-citation play) |
| 50 | atlassian-automation-scripts | atlassian automation | — | 42 | script library (high KD, demote if culled) |

**Cannibalization guardrails:** posts 1 vs 2 (raw REST vs CLI usage), 3/4/5 (concept vs rules vs examples), 6 vs 7 (JQL reference vs CLI usage), 32 vs 33 (PR concept vs create workflow) are the closest pairs — each must take a genuinely distinct angle. Codex to flag any that should merge.

**Topic clusters (internal linking):** each cluster links up to its pillar (jira→/jira/ + jira-cli-complete-guide; confluence→/confluence/; bitbucket→/bitbucket/; jsm→/jsm/; cross→/blog/atlassian-cli-guide) and sideways to 2-3 siblings.

---

## PART C — OTHER FIXES

1. **Backlinks (biggest lever, ~0 now):** point `Cargo.toml` homepage → atlassiancli.com (stop authority leak to GitHub); submit to awesome-atlassian / awesome-rust / awesome-cli lists; crates.io description w/ site link; use-case-led Reddit/Dev.to posts; HN "Show HN" for a real launch. (Mostly outreach — document, some are user actions.)
2. **GEO / AI citation:** publish post 49 (definitive listicle) + keep `llms.txt` updated with all new posts so ChatGPT can cite atlassiancli.com.
3. **Canonicalization:** one clear target page per cluster; fix homepage-vs-`/jira/` competition (Fix 1).
4. **Technical SEO:** add custom 404 (currently none); sitemap lastmod hygiene; add all 50 posts to sitemap + blog index + llms.txt; optional RSS for blog.
5. **Bing:** Part A Fix 5.

---

## PART D — NON-OFFICIAL / TRADEMARK COMPLIANCE (mandatory, every page)
- Verbatim `footer-legal` block on every published HTML page (audit existing; add to all 50 new).
- Each post includes an acli-differentiation sentence (link acli → developer.atlassian.com/cloud/acli/).
- Product names nominative only; never "official"/"endorsed"/"Atlassian product"; no Atlassian logos.
- `llms.txt` keeps "independent, not affiliated, distinct from acli" framing.

---

## PART E — DEPLOY & VERIFY
1. Implement on branch (50 posts + fixes + disclaimer audit + sitemap/index/llms.txt).
2. Local validation: every page serves 200 (python http.server), JSON-LD parses, no broken internal links, disclaimer present on 100% of pages.
3. **Codex review** of plan (now) + implementation (before merge).
4. **Review against `docs/seo_traffic_growth_plan.md`** (alignment check).
5. Merge to `main` → GitHub Pages rebuild → **verify live via real Chrome** (homepage, 3 sample new posts, sitemap, disclaimer) + re-check GSC/GA post-deploy.

## CODEX REVIEW — APPLIED DELTAS (2026-07-10, GPT-5.5)
Full review: `/Users/macmini/projects/codex/atlassiancli_content_fix_plan_review_10jul2026.txt`
- **Priority reorder:** #1 = Backlink/deep-link sprint (domain has 1 referring domain; 50 posts wait on trust). Deep-link `/jira/ /bitbucket/ /confluence/`, not just homepage. Then: jira-cli URL ownership, Bing recovery (NOT "add 50 posts"), CTR/schema cleanup, cluster depth (elevate Bitbucket — "gh for Bitbucket" is the strongest Reddit pain).
- **Fix 1 rediagnosed:** not clean canonical confusion — URL selection driven by homepage authority/prominence. Strengthen `/jira/` + exact-anchor internal links + honest lastmod + request indexing + external deep links; do NOT remove Jira relevance from homepage.
- **Wrong-intent retargets:** #12 format jira→"jira markdown"; #16 open source jira→cull/merge; #31 bitbucket pipelines kept but PR/commands elevated; #38 what-is-JSM→"jira service management integrations"; #39 vs-servicenow→retargeted; #40 ticketing→"jira service management api"; #46 acli→"acli atlassian"; #47 rovo→kept as careful informational; #50 atlassian automation (KD42)→"automation for jira" (320/15).
- **Mass-publish:** Google scaled-content-abuse spam policy (updated 2026-05-15). Mitigation = ~15 now, 3-5/wk; only sitemap/index the first tranche; do NOT fake lastmod. USER CHOSE all-50-live; we comply but (a) enforce strict QA (thin/fabricated/dup excluded), (b) recommend noindex on waves 2-3 as opt-in follow-up.
- **Trademark:** domain-name residual risk (Atlassian trademark policy forbids marks in domains) — no footer fully erases it; keep "independent, not affiliated" above the fold; #46 title = "Atlassian acli vs atlassian-cli: official tool vs independent open-source CLI".
- **Factual fixes:** Cargo.toml homepage ALREADY = atlassiancli.com (action → "verify crates.io metadata"); "jira cli" = 1,900/mo US (not 2,900); bare "acli" not measured (use "acli atlassian"/"acli jira"); Confluence/Bitbucket API rows are KD-only, not volume-measured — never print unmeasured volumes as authoritative.

## RISKS
- **Mass 50-post same-day publish on a ~0-backlink domain = Google "mass-produced content" risk.** Mitigate: distinct keywords, genuine useful content, per-post QA, honest long-tail labeling. Recommend natural lastmod cadence and monitoring for quality flags. (User chose all 50 knowingly.)
- Wrong-intent keywords (several JSM/generic) — codex to cull/replace from measured pool.
- Overlap with existing pages (auth, pr-automation, confluence backup) — differentiate angles.
- Thin content on long-tail posts — QA gate must reject padding.
