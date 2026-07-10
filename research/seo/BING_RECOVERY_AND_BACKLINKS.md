# Bing Recovery & Backlinks Checklist

Domain: `atlassiancli.com`
Owner actions doc. Every item below is a manual / external action performed by the site owner (Omar). Nothing here runs automatically. Check items off as completed and add the date.

Rule for every submission below: atlassian-cli is independent and unofficial. Never imply Atlassian affiliation or endorsement. Use product names (Jira, Confluence, Bitbucket, JSM) nominatively, only to describe compatibility.

---

## Part 1 - Bing recovery

Goal: get atlassiancli.com re-crawled and indexed by Bing (and, downstream, other IndexNow consumers).

### 1.1 Verify Bing Webmaster Tools ownership
- [ ] Sign in at https://www.bing.com/webmasters with the Google account tied to Search Console (fastest path is "Import from Google Search Console").
- [ ] Add site `https://atlassiancli.com/` (use the https + www-less canonical that matches the CNAME `atlassiancli.com`).
- [ ] Complete ownership verification. Options, easiest first:
  - Import from Google Search Console (one click if GSC already verified this property).
  - XML file: download `BingSiteAuth.xml`, place it at `/Users/macmini/projects/atlassian-cli/docs/BingSiteAuth.xml`, commit, confirm live at `https://atlassiancli.com/BingSiteAuth.xml`, then click Verify.
  - Meta tag: add `<meta name="msvalidate.01" content="...">` to the `<head>` of `/Users/macmini/projects/atlassian-cli/docs/index.html`.
  - DNS TXT/CNAME record on the domain registrar.
- [ ] Confirm the property shows "Verified" in Bing Webmaster Tools.

### 1.2 Confirm IndexNow key is live and pinging
- [ ] Key file already exists locally: `/Users/macmini/projects/atlassian-cli/docs/6cd7dd4fccf17030f15877ad39aabd25.txt` (contents = the key `6cd7dd4fccf17030f15877ad39aabd25`).
- [ ] Confirm it is live and returns the raw key with `text/plain`:
  ```bash
  curl -s https://atlassiancli.com/6cd7dd4fccf17030f15877ad39aabd25.txt
  # expect exactly: 6cd7dd4fccf17030f15877ad39aabd25
  ```
- [ ] In Bing Webmaster Tools > IndexNow, confirm the key is recognized / associated with the site.
- [ ] Send a single-URL ping (Bing endpoint) and confirm HTTP 200/202:
  ```bash
  curl -s -o /dev/null -w "%{http_code}\n" \
    "https://www.bing.com/indexnow?url=https://atlassiancli.com/&key=6cd7dd4fccf17030f15877ad39aabd25"
  ```
- [ ] Send a bulk ping for the key URLs (shared IndexNow endpoint pings Bing + Yandex + others):
  ```bash
  curl -s -X POST "https://api.indexnow.org/indexnow" \
    -H "Content-Type: application/json" \
    -d '{
      "host": "atlassiancli.com",
      "key": "6cd7dd4fccf17030f15877ad39aabd25",
      "keyLocation": "https://atlassiancli.com/6cd7dd4fccf17030f15877ad39aabd25.txt",
      "urlList": [
        "https://atlassiancli.com/",
        "https://atlassiancli.com/jira/",
        "https://atlassiancli.com/confluence/",
        "https://atlassiancli.com/bitbucket/",
        "https://atlassiancli.com/jsm/",
        "https://atlassiancli.com/install/",
        "https://atlassiancli.com/blog/"
      ]
    }'
  ```
- [ ] Note: IndexNow only accepts URLs that already return 200. Do not submit the 404 page.

### 1.3 Resubmit sitemap to Bing
- [ ] Confirm sitemap is live: `curl -sI https://atlassiancli.com/sitemap.xml` (expect 200, `application/xml`).
- [ ] In Bing Webmaster Tools > Sitemaps, submit `https://atlassiancli.com/sitemap.xml`.
- [ ] Confirm `robots.txt` still advertises it (it does): `Sitemap: https://atlassiancli.com/sitemap.xml`.
- [ ] Before resubmitting, sanity-check sitemap freshness at `/Users/macmini/projects/atlassian-cli/docs/sitemap.xml` (`lastmod` dates, no stale/removed URLs, no 404-only entries). The new `/404.html` intentionally has no sitemap entry.

### 1.4 Check indexed URL count in Bing
- [ ] Bing Webmaster Tools > Site Explorer / URL Inspection: record how many URLs Bing reports as indexed and the crawl status of each key page (`/`, `/jira/`, `/confluence/`, `/bitbucket/`, `/jsm/`, `/install/`, `/blog/`).
- [ ] Quick external check: search `site:atlassiancli.com` on https://www.bing.com and record the count.
- [ ] Use URL Inspection to request indexing on any key page shown as "Discovered / not indexed".
- [ ] Set a recheck reminder for ~7-14 days after submission; log the before/after indexed count in this file.

---

## Part 2 - Backlinks

Goal: earn a handful of high-quality, topically-relevant, editorially-legitimate backlinks. Quality and relevance over volume. No paid links, no link farms, no spam.

### 2.1 crates.io metadata (verify only - no code change needed)
- [ ] `Cargo.toml` already sets `homepage = "https://atlassiancli.com"` (line 18) and `repository = "https://github.com/omar16100/atlassian-cli"` (line 17). No edit required.
- [ ] Verify the published crate page renders the homepage link (this is the backlink): open `https://crates.io/crates/atlassian-cli` and confirm the "Homepage" field points to `atlassiancli.com`.
- [ ] If the live crate still shows an old/missing homepage, it means the last publish predates the `homepage` field. Fix by publishing a new version (only when a release is due): `cargo publish`. Do not bump the version solely for this.

### 2.2 "Awesome" list PRs (editorial, high-relevance)
Add one concise, honest entry per list. Follow each list's CONTRIBUTING format and alphabetical/section rules. Describe as an independent/unofficial CLI.
- [ ] awesome-atlassian: search GitHub for the canonical `awesome-atlassian` repo; open a PR adding atlassian-cli under a CLI/tools section. Suggested line: `atlassian-cli - Independent open-source CLI compatible with Jira, Confluence, Bitbucket, and JSM Cloud.`
- [ ] awesome-rust (https://github.com/rust-unofficial/awesome-rust): add under Applications > a relevant category (e.g. productivity / command-line utilities). Must be a Rust project (it is) and meet their maturity bar.
- [ ] awesome-cli / awesome-cli-apps (https://github.com/agarrharr/awesome-cli-apps and https://github.com/toolleeo/awesome-cli-apps-in-a-csv): add under a Productivity / Development section.
- [ ] For each PR: link to `https://atlassiancli.com/` and the GitHub repo; keep the description factual; do not claim "official" or Atlassian endorsement.

### 2.3 crates.io / lib.rs ecosystem
- [ ] Confirm the crate is discoverable on https://lib.rs/crates/atlassian-cli (mirrors crates.io metadata, includes the homepage link).
- [ ] Ensure crate keywords/categories in `Cargo.toml` are relevant (e.g. `command-line-utilities`, `jira`) so it surfaces in category pages that link out.

### 2.4 Dev.to cross-posts with canonical
- [ ] Repurpose existing blog posts from `/Users/macmini/projects/atlassian-cli/docs/blog/` as Dev.to articles.
- [ ] In each Dev.to post front matter set `canonical_url` to the original, e.g. `canonical_url: https://atlassiancli.com/blog/jira-bulk-operations.html`. This attributes SEO value to the origin and still yields a contextual backlink.
- [ ] Include an in-body link to the relevant product page (`/jira/`, `/confluence/`, etc.) and the GitHub repo.
- [ ] Tag appropriately (`#rust`, `#cli`, `#productivity`, `#devops`). Disclose it is an independent/unofficial tool.

### 2.5 Use-case-led Reddit posts (value first, not link-drops)
Post genuinely useful, problem-solving content; link only where it naturally helps. Read each subreddit's self-promotion rules first; several restrict links.
- [ ] r/atlassian: a "how I bulk-transition Jira issues from the terminal" style post tied to a real workflow. Lead with the workflow, mention the tool as one option, disclose you're the author, note it's unofficial/independent.
- [ ] r/git: angle around Bitbucket PR/branch workflows from the CLI, if applicable.
- [ ] r/devops or r/commandline: automation / scripting angle (dry-run bulk ops, CI usage).
- [ ] For all: follow the 9:1 rule (mostly participate, rarely self-link), obey per-subreddit promo rules, and always disclose authorship + unofficial status. These links are typically `nofollow` but drive referral traffic and discovery.

### 2.6 Show HN (only for a real launch)
- [ ] Save this for a genuine milestone (e.g. a notable release), not a soft launch. Title format: `Show HN: atlassian-cli - an independent open-source CLI for Jira/Confluence/Bitbucket`.
- [ ] Link to `https://atlassiancli.com/` and the GitHub repo. Be present in comments for the first few hours.
- [ ] Be explicit in the post that it is not affiliated with or endorsed by Atlassian, and that Atlassian ships its own separate official ACLI.

### 2.7 Atlassian Community post
- [ ] Post in the Atlassian Community (https://community.atlassian.com) in a relevant space (e.g. Jira / Automation / Marketplace-adjacent developer discussion) sharing the tool as a community-built, independent option.
- [ ] Strictly follow their community + self-promotion guidelines. Clearly label it independent/unofficial; do not use Atlassian logos or imply partnership; product names nominative only.

---

## Tracking log
Record each action with a date and result (indexed count deltas, PR URLs, post URLs). Keep this section updated as the source of truth for outreach status.

| Date | Action | Link / Result |
| ---- | ------ | ------------- |
|      |        |               |
