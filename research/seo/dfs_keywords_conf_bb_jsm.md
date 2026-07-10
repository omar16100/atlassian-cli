# Confluence + Bitbucket + JSM keyword universe (DataForSEO)

Target: atlassiancli.com | Location: United States | Language: English
Endpoints: keyword_ideas (multi-seed), keyword_suggestions (confluence cli / bitbucket cli / jira service management), bulk_keyword_difficulty.

## Best ~40 keywords (deduped; prioritized volume>=20 & KD<=30, then volume)

| keyword | volume | KD | intent | product |
|---|---|---|---|---|
| jira service management | 6600 | 29 | navigational | jsm |
| jira service management pricing | 880 | 29 | commercial | jsm |
| confluence-cli | 320 | 3 | navigational | confluence |
| confluence cli | 320 | 3 | navigational | confluence |
| bitbucket-cli | 320 | 1 | navigational | bitbucket |
| bitbucket cli | 320 | 17 | navigational | bitbucket |
| what is jira service management | 260 | 23 | informational | jsm |
| jira service management itsm | 210 | 24 | navigational | jsm |
| jira service management free plan | 170 | 14 | informational | jsm |
| jira service management ticketing system | 140 | 21 | navigational | jsm |
| jira service management licensing | 90 | 19 | navigational | jsm |
| jira service management license | 90 | 19 | informational | jsm |
| jira service management licenses | 90 | 29 | informational | jsm |
| jira service management cost | 70 | 6 | commercial | jsm |
| jira customer service management | 70 | 11 | navigational | jsm |
| jira service management vs servicenow | 50 | 1 | informational | jsm |
| jira service management vs jira software | 50 | 8 | commercial | jsm |
| jira service management data center | 50 | 17 | navigational | jsm |
| jira service management demo | 50 | 27 | navigational | jsm |
| jira service management assets | 50 | 25 | navigational | jsm |
| jira service management asset | 50 | 19 | navigational | jsm |
| jira service management forms | 50 | 9 | navigational | jsm |
| jira service management portal | 40 | 27 | navigational | jsm |
| jira service management integrations | 40 | 3 | navigational | jsm |
| jira service management integration | 40 | 10 | navigational | jsm |
| jira service management standard pricing | 40 | 6 | informational | jsm |
| jira service management icon | 40 | 13 | navigational | jsm |
| jira service management knowledge base | 30 | 9 | navigational | jsm |
| jira service management free | 30 | 29 | informational | jsm |
| jira service management free tier | 30 | 25 | informational | jsm |
| jira service management free trial | 30 | 25 | informational | jsm |
| jira service management certification | 30 | 11 | informational | jsm |
| jira service management ai | 30 | 27 | navigational | jsm |
| jira service management app | 30 | 17 | navigational | jsm |
| jira service management cloud pricing | 30 | 14 | commercial | jsm |
| service management jira | 6600 | 37 | navigational | jsm |
| atlassian jira service management | 480 | 32 | navigational | jsm |
| bitbucket cli commands | 320 | n/a | informational | bitbucket |
| jira service management jsm | 110 | 37 | navigational | jsm |
| jira service management api | 110 | n/a | navigational | jsm |

## Core seed / pillar terms (bulk_keyword_difficulty = KD only, volume not returned by that endpoint)
These are high-value topic pillars. Low KD, strong for how-to / API blog content. Exact volume needs a search-volume call.

| keyword | KD | intent | product |
|---|---|---|---|
| confluence api | 25 | informational | confluence |
| confluence rest api | 9 | informational | confluence |
| confluence automation | 2 | informational | confluence |
| confluence markdown | 10 | informational | confluence |
| bitbucket api | 26 | navigational | bitbucket |
| bitbucket rest api | 6 | informational | bitbucket |
| bitbucket pipelines | 37 | navigational | bitbucket |
| bitbucket pr | 14 | navigational | bitbucket |
| bitbucket pull request | 12 | navigational | bitbucket |
| atlassian rest api | 27 | informational | atlassian |
| atlassian automation | 42 | navigational | atlassian |

## Notes
- KD `n/a` = DataForSEO returned no keyword_difficulty (usually ~0 volume / thin SERP data).
- JSM cluster dominates volume: `jira service management` 6,600/mo (KD 29). Most JSM long-tails are KD<30 = winnable.
- Confluence + Bitbucket CLI head terms all sit at 320/mo with very low KD (1-17): easy wins for a CLI-focused site.
- `keyword_ideas` multi-seed returned mostly generic "*software" noise (broad category match); the suggestions endpoints were the signal. Full raw kept in raw_keyword_ideas_multiseed.json.

## Files
- Consolidated: /Users/macmini/projects/atlassian-cli/research/seo/dfs_keywords_conf_bb_jsm.json
- Summary: /Users/macmini/projects/atlassian-cli/research/seo/dfs_keywords_conf_bb_jsm.md
- Raw keyword_ideas: /Users/macmini/projects/atlassian-cli/research/seo/raw_keyword_ideas_multiseed.json
- Raw JSM suggestions: /Users/macmini/projects/atlassian-cli/research/seo/raw_suggestions_jira_service_management.json
