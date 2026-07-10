# Reddit Pain-Point Mining: Atlassian CLIs & Automation

Generated: 2026-07-10 · Source: Reddit search API (app-only OAuth) · Location basis: United States
Raw JSON: `/Users/macmini/projects/atlassian-cli/research/seo/reddit_pain_points.json`

## Method
5 site-wide Reddit searches (time=all): `jira cli command line`, `jira automation script api bulk`,
`atlassian cli confluence bitbucket`, `confluence api automation export`, `bitbucket pipelines cli automation`.
Filtered to Atlassian-ecosystem + dev/ops subreddits, removed marketplace/coupon spam, deduped by post id.
Result: **59 distinct relevant threads**. Below are the highest-signal for blog/FAQ/AI-answer angles.

## Top signal: the recurring themes

1. **"Atlassian never shipped a real Bitbucket CLI like GitHub's `gh`"** — strongest, most-repeated pain.
   Multiple devs built their own (`r/git` 14, `r/atlassian` 11+4). Prime SEO angle: "Bitbucket CLI",
   "gh for Bitbucket", "Bitbucket command line PR create".
2. **Cross-product CLI wanted** (Jira + Confluence + Bitbucket in one tool) — `r/atlassian`, `r/commandline`,
   `r/jira` all show the same "CLI for Atlassian products" post; original poster built one for AI agents to
   talk to Bitbucket/Jira/Confluence with personal access tokens.
3. **Confluence export / docs-as-code** — export spaces to Markdown, MkDocs → Confluence, RAG pipelines.
   "The API is slow, the export format is messy." (`confluence2md`, `mkdocs2confluence`).
4. **Rovo Dev CLI (Atlassian's own AI CLI)** — high interest but also confusion/errors
   ("Rovo Dev Agents is not installed on your site"). Token-limit questions ($8 tier, 5M/20M tokens/day).
5. **Migration automation** — Bitbucket Cloud → GitHub Enterprise, Bitbucket Pipelines → GitHub Actions,
   people want to script it instead of clicking the web importer per-repo.
6. **Scripting the boring stuff** — batch-create a Jira project + matching Confluence space + Stash repo;
   auto-update Confluence tables/pages from Jira; branch naming from Jira issue keys.
7. **Auth friction** — PAT/token setup, MCP setup with Copilot/Claude CLI, locked-down/gov machines (no installs, CAC auth).

## Ranked questions / pain points (blog angles)
Format: subreddit | score | comments | topic tag | question

| # | Subreddit | Score | Cmts | Tag | Real question / pain point |
|---|-----------|-------|------|-----|----------------------------|
| 1 | r/git | 14 | 2 | bitbucket | "Made a Bitbucket CLI because gh spoiled me and Atlassian still hasn't shipped one" |
| 2 | r/atlassian | 11 | 3 | bitbucket/automation | "Got tired of waiting for Atlassian to make a Bitbucket CLI like GitHub's gh, so I built my own" |
| 3 | r/ruby | 82 | 20 | jira | Community demand for a Jira command-line app ("Anyone use JIRA? released a CLI") |
| 4 | r/AIcliCoding | 10 | 9 | jira/auth | Atlassian ACLI/Rovo Dev vs Claude Code/Codex; token limits & value comparison |
| 5 | r/commandline | 1 | 4 | jira/bitbucket/confluence | "I wanted a CLI for my AI agents to talk to Bitbucket & Jira... built one; cloud + PAT" |
| 6 | r/atlassian | 2 | 0 | jira/confluence/bitbucket | "CLI for Atlassian products - Jira, Confluence, and Bitbucket" (cross-posted x3) |
| 7 | r/embedded | 17 | 9 | jira/confluence/bitbucket | Alternatives to Atlassian suite with traceability for ISO cert + CI/CD pipelines |
| 8 | r/opensource | 16 | 2 | confluence | Export Confluence spaces to local Markdown via CLI (confluence2md) |
| 9 | r/technicalwriting | 6 | 10 | confluence | Compile MkDocs → native Confluence storage format from CLI (mkdocs2confluence) |
| 10 | r/SideProject | 0 | 0 | confluence | "API is slow, export format is messy" — Confluence → Markdown for RAG/LLM pipelines |
| 11 | r/jira | 8 | 4 | jira | Atlassian ACLI on Windows: PowerShell helpers; "not quite as powerful as X yet" |
| 12 | r/github | 2 | 8 | bitbucket/automation | Automate migrating many Bitbucket Cloud repos → GitHub Enterprise (avoid per-repo web importer) |
| 13 | r/Tidra | 8 | 0 | bitbucket/ci | Migrate bitbucket-pipelines.yml → GitHub Actions at scale (200 repos) |
| 14 | r/rust | 11 | 0 | bitbucket | CLI to migrate repositories from Bitbucket to GitHub |
| 15 | r/atlassian | 2 | 2 | jira/confluence/automation | Batch script: create Jira project + matching Confluence space + Stash repo + team calendar |
| 16 | r/atlassian | 1 | 3 | jira/confluence/automation | Auto-create a Confluence page + self-updating work-items table from a Jira release |
| 17 | r/jira | 3 | 1 | jsm/auth | Ingest 5GB OneNote (with attachments) into JSM tickets on locked-down gov PC, no installs, CAC auth |
| 18 | r/GithubCopilot | 3 | 5 | jira/auth | How to set up Atlassian (Jira) MCP with Copilot CLI |
| 19 | r/RovoDev | 4 | 8 | jira | "Rovo Dev Agents is not installed on your site" error even though enabled |
| 20 | r/ChatGPTCoding | 27 | 32 | jira | Reaction/eval of Atlassian's Rovo Dev CLI terminal agent (open beta) |
| 21 | r/jira | 1 | 0 | jira/automation | Automate git branch naming from Jira issue keys (gibr CLI) |
| 22 | r/mcp | 1 | 0 | jira/automation | MCP app that reads Jira issues, generates API tests, opens a PR (Jira automation) |
| 23 | r/atlassian | 9 | 7 | jira/automation | Automation for Jira becoming the new engine for JSD; what changes for scripts |
| 24 | r/SideProject | 0 | 0 | jira | "Tired of Jira?" update Jira via Markdown files + command line (imdone-cli) |
| 25 | r/AZURE | 4 | 2 | confluence/automation | Build an AI agent to auto-publish release notes to Confluence (Atlassian MCP) |
| 26 | r/alphaandbetausers | 1 | 0 | jira/confluence/automation | Auto-update Confluence pages linked to Jira tickets when they hit DONE |
| 27 | r/atlassian | 8 | 4 | confluence/bitbucket | Atlassian admin skill roadmap: Jira/Confluence not enough, need Bitbucket + git/automation |
| 28 | r/technicalwriting | 2 | 3 | confluence/bitbucket | Bitbucket → static site because Confluence is awful (knowledge-base workflow) |
| 29 | r/JavaScriptTips | 2 | 1 | confluence | Tips for programmatic Confluence work (self-taught PM automating docs) |
| 30 | r/SideProject | 0 | 11 | automation/ci | Pattern: wrap each platform's REST API (incl Jira) as CLI/agent skills |
| 31 | r/ArcBrowser | 1 | 0 | auth | Can't log in to Atlassian/Bitbucket/Jira/Confluence (auth/session friction) |
| 32 | r/confluence | 8 | 7 | confluence | Building first Atlassian Marketplace app (Forge dev, secret scanning) |

## Notable exact quotes
- "Bitbucket… does not have a `gh`." (r/git)
- "I wanted a CLI for my AI agents to talk to bitbucket and JIRA, so ended up building one myself, it works for cloud hosted jira/bb/confluence with personal access tokens" (r/commandline)
- "The API is slow, the export format is messy, and nothing quite gave me what I wanted: local Markdown files that could be version[ed]" (r/SideProject, Confluence export)
- "write batch scripts which will create a new jira project, a matching confluence space, a new repo in stash and a jira calendar" (r/atlassian)
- "Ideally I'd like to automate this rather than having to go through the Github Importer web interface for each repo" (r/github, Bitbucket→GitHub migration)

## Recommended blog / FAQ / AI-answer targets
- "Is there an official Bitbucket CLI? (and the `gh`-style alternatives)"
- "One CLI for Jira + Confluence + Bitbucket: how to script across all three"
- "Export Confluence to Markdown from the command line"
- "Bulk-create/update Jira issues from the terminal"
- "Automate branch names from Jira issue keys"
- "Migrate Bitbucket repos & pipelines to GitHub (scripted)"
- "Atlassian ACLI / Rovo Dev CLI: what it does and does not do"
- "Authenticate a CLI to Atlassian Cloud with API tokens / PAT"
- "Auto-sync Jira → Confluence pages and tables"
