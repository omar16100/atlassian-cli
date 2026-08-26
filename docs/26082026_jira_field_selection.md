# `--fields` on `jira issue get` and `jira issue search`

Status: in progress on `feat/jira-field-selection` (issue #132).

## Problem

`jira issue get` deserialized eight named fields and discarded everything else,
even though it sent no `fields` parameter and so received every navigable field
Jira had. `jira issue search` asked for a hardcoded
`fields=key,summary,status,assignee,issuetype`.

Custom fields, which is where most teams keep the data they actually care about,
were unreachable. The only workaround was `jira api /rest/api/3/issue/KEY`, which
returns the raw payload and loses every convenience of the command.

Reported in #132, which also asked for display names rather than
`customfield_10016`, since nobody knows their own site's field ids.

## Surface

```bash
jira issue get DEV-123 --fields summary,status
jira issue get DEV-123 --fields "Story Points" --format json
jira issue get DEV-123 --fields all --format json
jira issue search --project DEV --fields status,"Story Points" --format csv
```

One flag, `--fields`, on both commands. `Vec<String>` with
`value_delimiter = ','`, so `--fields a,b`, `--fields a --fields b` and a mix all
work, matching `jira bulk export --fields`.

Not `--field`: on `issue create` and `issue update` that already means *set*
`KEY=JSON_VALUE`. Reusing the singular for *select* would be a real footgun.

**`all` is the documented spelling for everything.** A bare `*all` is a glob, and
zsh refuses the whole command line with "no matches found" before the CLI starts.
Any token already starting with `*` or `-` is passed to Jira untouched, so
`--fields '*navigable'` and the exclusion form `--fields '*all,-comment'` work at
no extra cost.

On `search`, `--fields` **replaces** the default five columns rather than adding
to them, otherwise narrowing output would be impossible. `key` is always the
first column and is never duplicated.

**Without `--fields`, both commands behave exactly as before**, pinned by test.

## Design notes

### A separate raw path, not `#[serde(flatten)]`

`IssueFields` is strongly typed with no catch-all. Adding
`#[serde(flatten)] extra: BTreeMap<String, Value>` was rejected twice over:

- The default `jira issue get` sends no `fields` parameter and so receives the
  entire issue, comments and worklogs included. Flatten would allocate and retain
  all of it on every plain `get`, for output that discards it.
- It collides with the existing `#[serde(rename = "customfield_10020")] sprint`.
  That key would be consumed by `sprint` and therefore missing from `extra`, so
  `--fields customfield_10020` would silently return nothing.

`--fields` instead takes its own path over
`RawIssue { key: String, fields: serde_json::Map<String, Value> }`. The existing
types and render paths are not edited at all, so the default output is unchanged
by construction rather than by review. `jira bulk export` already worked this
way.

Jira does the projection server-side, so nothing unrequested is downloaded.

### Name resolution, and why ambiguity is an error

`GET /rest/api/3/field` is fetched only when a token could be a display name,
which means unless every token matches `customfield_<digits>`. That is the only
shape that is unambiguously an id: `summary` and `status` are ids too, but a site
can perfectly well have a custom field named "Status", and nothing local can tell
the difference.

Matching is, per token: wildcard passthrough, then case-insensitive exact match
on `id`, then case-insensitive exact match on `name`. An id wins over a custom
field sharing its display name, or every existing script would quietly start
reading a different field.

**Several fields sharing a name is the normal state of a mature Jira site**, and
the copies belong to different screens and hold different values. Picking one
would be wrong in a way the output cannot show, so it is an error listing every
candidate:

```
Field name 'Story Points' is ambiguous on this site. Candidates:
  customfield_10016  Story Points
  customfield_10032  Story Points
Pass the id instead, for example --fields customfield_10016.
```

Unknown names and ambiguous names both fail before the issue is fetched, so a
typo does not cost a request.

No prefix or fuzzy matching. Partial matches against a 300-field list produce
confident wrong answers, and this flag feeds scripts.

Output keys are the token as typed, so the table header and the JSON key always
agree. The cost, worth knowing: a display name with a space is awkward under
`jq` (`jq '.["Story Points"]'`), and the fix is to pass the id.

A requested field the response does not carry is an empty cell and a
`tracing::warn!`, not an error. Issue types differ in which fields they have, so
erroring would make any wide selection unusable across a mixed result set.

### Column order

`coerce_rows` derives columns as an alphabetical `BTreeSet` union, and
`serde_json::Map` is a `BTreeMap`, so `--fields status,summary` would have
printed `summary` first.

Enabling `serde_json/preserve_order` was rejected: it would change JSON, YAML,
CSV and table key order for all ~70 existing commands, breaking `--format csv`
consumers as a side effect of a Jira feature, and it would not even fix this,
since `coerce_rows` re-sorts regardless.

Instead `crates/output` gained one public method,
`OutputRenderer::render_rows_ordered(rows, columns)`, backed by
`coerce_rows_with(value, Option<&[String]>)`. `coerce_rows(value)` is unchanged
behaviour, so every existing call site and test is untouched. When columns are
given they are authoritative: unlisted keys are dropped, listed-but-missing
render empty. JSON and YAML are deliberately left alone, because their key order
is alphabetical no matter what we do.

`jira issue get --fields` sidesteps ordering entirely by rendering the tabular
formats as a two-column vertical table of field and value, which is also far more
readable than a twelve-column horizontal scroll:

```
╭──────────────┬─────────────────────╮
│ field        │ value               │
├──────────────┼─────────────────────┤
│ key          │ DEV-1               │
│ summary      │ Fix the login error │
│ Story Points │ 5                   │
│ Department   │ Platform            │
╰──────────────┴─────────────────────╯
```

Its JSON and YAML stay a flat object, since an array of field/value pairs would
be hostile to `jq`.

### Flattening is a rendering decision only

A custom field value is nearly always a wrapper object: a select option is
`{"value":"Internal"}`, a user is `{"displayName":"Ada"}`, a priority or sprint
is `{"name":"High"}`. Printing the JSON into a table cell is lossless and
unreadable.

`friendly_scalar` handles the table, CSV and markdown formats only: scalars pass
through, rich text goes through the existing ADF extractor, other objects take
the first non-empty string among `displayName`, `name`, `value`, `filename`,
`key`, and arrays map one level and join with `, `. Anything unrecognised falls
back to compact JSON, so data is never lost.

`value_to_string` in `crates/output` was deliberately not taught any of this: it
serves Bitbucket and Confluence too, where `{"value": ...}` means nothing in
particular.

**JSON and YAML never call it and are byte for byte what Jira sent.** The format
people pipe into `jq` must not be the lossy one.

### The curated markdown view

`view_issue`'s hand-built markdown is not reached when `--fields` is given:
dispatch routes to `view_issue_fields` first, so the curated path is untouched
code. The `--fields` markdown output is a `# KEY` heading plus the generic
two-column table.

Conditionally re-adding the Description and Attachments sections when the
selection happens to include those fields would make the output shape depend on
the selection in a way nobody can predict. `--fields` means "these and nothing
else"; users who want the curated view omit the flag.

### Found in self-review, after the first pass

Three defects surfaced by running the built binary against a mock Jira and
reading what it actually sent, rather than by reading the code:

- **A blank selection cost a request.** `--fields ""` and `--fields ,` reach the
  resolver as empty tokens, which are not custom field ids, so the field list
  was fetched before the "no field names" error. Now checked first.
- **Two spellings of one field became two columns.** `--fields summary,Summary`
  and `--fields customfield_10016,"Story Points"` sent the id to Jira twice and
  printed the same value twice, because duplicates were detected on the token
  rather than on the resolved id.
- **An empty result printed `[]`.** Under table, CSV, markdown and quiet, a
  search returning nothing printed `[]`, where every other list command prints
  "No issues found" for the formats people read and nothing at all for the
  line-oriented ones. That is exactly the inconsistency #110 fixed across ~70
  commands, reintroduced on a new path. The empty case now goes through the same
  `render_list_or_empty` helper.

A wildcard mixed with named fields (`--fields all,summary`) now warns: Jira reads
it as everything, so the named fields do not narrow anything.

## Limitations

- **`--fields all` returns Jira's raw ids as keys** (`customfield_10016`, not
  "Story Points"). Mapping them back would force the `/field` fetch on every
  call and make JSON keys unstable across sites.
- **No caching of the field list.** One `GET /rest/api/3/field` per invocation
  that uses a display name. There is no caching infrastructure in the workspace,
  and the config directory is deliberately limited to three files. A
  process-lifetime cache is the next step if this is ever a complaint, not a
  disk one.
- **No `--expand`.** `renderedFields`, `changelog` and `names` are a separate
  feature with a separate output shape.
- **`--fields all` on a large search is a large response.** `--limit` (default
  25) is the only control.
- Not added to `issue create`/`update`, JSM or Confluence.

## Tests

- **Unit, `field_selection.rs`** (22): the id-only fast path and the cases that
  must still trigger a lookup; `all` as a wildcard in any case; Jira wildcards
  and exclusions passing through; id winning over a same-named custom field;
  unknown name pointing at `jira fields list`; **ambiguous name listing every
  candidate id**, which a naive `.find()` would fail; duplicate tokens collapsing
  in place; order preserved; the full `friendly_scalar` table including ADF,
  arrays, `displayName` preferred over a sibling `name`, and the compact-JSON
  fallback; projection putting `key` first, not duplicating it, and yielding null
  for an absent field.
- **Unit, `crates/output`** (5): explicit columns preserve order, drop unlisted
  keys and fill absent ones with empty; empty columns render nothing; and the
  default union is still alphabetical, pinning the contract for the other ~70
  commands.
- **Command-level e2e, `crates/cli/tests/jira_fields_e2e.rs`** (11): only the
  resolved ids reach the `fields` parameter; the field list is never fetched for
  an id-only selection; `all` becomes `*all`; no `fields` parameter and the
  curated markdown view without the flag; `--format json` keeps the raw wrapper
  object while `--format table` shows the label; unknown and ambiguous names fail
  with the issue mock hit zero times; the default search columns are unchanged;
  and `--fields` replaces them with the CSV header exactly
  `key,status,Story Points`, an absent value rendering as an empty cell.

Both of the load-bearing behaviours were verified by reverting them: ignoring the
explicit columns turns the CSV header into `Story Points,key,status`, and
replacing the ambiguity check with `.find()` makes the site silently answer with
one of two different fields.
