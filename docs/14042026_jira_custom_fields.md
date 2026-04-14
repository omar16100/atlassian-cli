# Jira Custom Fields (`--field`)

Date: 2026-04-14

## Overview

`jira issue create`, `jira issue update`, and `jira bulk import` accept arbitrary Jira fields (including `customfield_*`) via a repeatable `--field` flag or a `custom_fields` map in the bulk-import JSON schema.

## Usage

### Single issue

```bash
atlassian-cli jira issue create --project PROJ --issue-type Task \
  --summary "cf test" \
  --field 'customfield_10010={"value":"Internal"}' \
  --field 'customfield_12100={"value":"SRE"}'

atlassian-cli jira issue update DEV-42 \
  --field 'customfield_12100={"value":"Platform"}'
```

The value after `=` is JSON, so objects, arrays, strings, and numbers all work. `=` inside JSON values is preserved (only the first `=` splits key from value):

```bash
--field 'customfield_10020={"formula":"a=b"}'
```

### Bulk import

```json
[
  {
    "summary": "Ticket from import",
    "issue_type": "Task",
    "custom_fields": {
      "customfield_10010": [{"value": "Internal"}],
      "customfield_12100": {"value": "SRE"}
    }
  }
]
```

## Discovering field IDs

```bash
atlassian-cli jira fields list           # all fields in your instance
atlassian-cli jira fields get customfield_10010
```

## Collision rules

`--field` (and `custom_fields` in bulk) **cannot silently overwrite** fields that have a dedicated source:

- **Reserved (always rejected)**: `project`, `issuetype`, `summary` on create; same three plus `project`/`issuetype`/`summary` on bulk rows.
- **Optional (rejected when both sources set)**: `description`, `assignee`, `priority`, and (bulk only) `labels`. Using `--field` is fine as long as the typed flag (`--description`, `--assignee`, `--priority`, or row `labels`) is NOT also provided.

Duplicate `--field` entries targeting the same key are also rejected.

Example rejection:

```text
$ jira issue create ... --summary "A" --field 'summary="B"'
Error: --field cannot set reserved key 'summary'; use --summary instead
```

## Rationale

The silent last-write-wins behavior would have been a real footgun — e.g. a user combining `--summary "typed"` with `--field summary=...` would get whichever path ran last. Hard-erroring forces the user to pick one source and makes the CLI's behavior deterministic.
