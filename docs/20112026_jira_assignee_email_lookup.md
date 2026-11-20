# Jira assignee email lookup and `jira user search`

Status: in progress on fix/jira-assignee-email (PR #PR)

## Problem

`jira issue create --assignee dev@example.com` failed with
"assignee: Please select a valid user." Coding agents that used the CLI got
this failure more than one time.

The help text of `--assignee` said "account ID or email". The code sent the
value as the account ID with no lookup (`{"id": "<value>"}` on create,
`{"accountId": "<value>"}` on assign). Jira Cloud identifies a user by account
ID only, so an email address was never valid.

No command listed account IDs. One agent tried `jira user`, which did not exist.

## Design notes

### Sources

The facts about the Jira API come from the public Jira Cloud platform REST API
v3 document and its OpenAPI file, read on 2026-11-20:

- "Find users", `GET /rest/api/3/user/search`:
  <https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-user-search/#api-rest-api-3-user-search-get>
- "Get current user", `GET /rest/api/3/myself`:
  <https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-myself/#api-rest-api-3-myself-get>
- OpenAPI file: <https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json>
- Profile visibility: <https://developer.atlassian.com/cloud/jira/platform/profile-visibility/>

What the document says about "Find users":

- The parameters are `query`, `accountId`, `startAt` (default 0), `maxResults`
  (default 50) and `property`. `query` "is matched against user attributes
  (`displayName`, and `emailAddress`)" and "can match the prefix of the
  attribute's value".
- The response is a bare JSON array of `User` objects. It has no page envelope.
- It applies the filter first, then takes the range from `startAt` and
  `maxResults`, "up to the thousandth user".
- "Privacy controls are applied to the response based on the users'
  preferences. This could mean, for example, that the user's email address is
  hidden." The `User` schema says that `emailAddress` "may be returned as null".
- "Anonymous calls or calls by users without the required permission return
  empty search results." The permission is "Browse users and groups".

### The resolver

`crates/cli/src/commands/jira/users.rs` has `resolve_account_id(client, value)`:

| Value | Result |
| --- | --- |
| `me` or `@me` | `accountId` from `GET /rest/api/3/myself` |
| contains `@` | lookup with `GET /rest/api/3/user/search?query=<value>&startAt=0&maxResults=50` |
| all other values | the value, unchanged, with no request |

`encode_query_component` encodes the query value.

The rules for an email lookup are in `pick_user_for_email`:

1. Exactly one result has the same `emailAddress` (case ignored): it wins. This
   is also the rule when the search returns many results.
2. No result has the same `emailAddress`, and there is one result with no
   `emailAddress`: it wins. Jira matched the query, and the email address is
   hidden.
3. No result has the same `emailAddress`, and there is one result that shows a
   different email address: error. The search matches a prefix, so
   `dev@example.com` can find `dev@example.com.test`. That is a different user.
4. Zero results: error. The text names the two documented causes (privacy
   settings, the "Browse users and groups" permission) and
   `atlassian-cli jira user search <query>`.
5. More than one result and no single exact match: error. The text names each
   match with display name and account ID (10 maximum, then a count).

The lookup comes before the write request. An error changes nothing in Jira.

### Decisions

- `@me` is accepted with `me`. `jira issue search --assignee @me` uses that
  form. Without this rule `@me` has an `@`, so it becomes an email lookup with
  no result.
- Rule 3 is stricter than "one match gives its account ID". A wrong assignee is
  worse than an error, and the error names the nearest match.
- The resolver runs in `jira/mod.rs`, before the handlers. The handlers in
  `issues.rs` and `build_create_payload` did not change. They get an account ID.
- `jira bulk assign` makes one lookup for all issues. It makes the lookup in
  list mode too, so a bad email address fails before `--execute`.
- `jira bulk import` resolves each different `assignee` value one time, before
  it creates the first issue. `--dry-run` makes no lookup, as it made no
  request before.
- `jira user` has the visible alias `users`. The Jira command tree has singular
  names (`issue`, `project`) and plural names (`components`, `versions`), so a
  caller can guess one or the other.
- `jira user search` uses the general user search, not the "assignable users"
  search. The assignable search needs a project or an issue, and watchers are
  not assignees.

### Each place where a user value becomes an account ID

| Place | Changed | Reason |
| --- | --- | --- |
| `jira issue create --assignee` | yes | The reported failure |
| `jira issue assign --assignee` | yes | Same input, same failure |
| `jira bulk assign --assignee` | yes | Same request for each issue. The README example uses an email address |
| `jira bulk import`, row `assignee` | yes | Same payload as create |
| `jira issue watchers add` and `remove` | yes | The help said "account ID or email" and sent the value unchanged |
| `jira issue update` | no | It has no `--assignee` option. `jira issue assign` does this work |
| `jira issue search --assignee` | no | The value goes into JQL, not into an account ID field. `@me` becomes `currentUser()` |
| `jira project create` and `update --lead`, `jira components create` and `update --lead` | no | The help says "account ID" only. Not part of this fix |
| `jira roles add-actor` and `remove-actor --user` | no | Same reason |

### `jira user search <query>`

Output keys: `account_id`, `display_name`, `email`, `active`. `email` is `null`
in JSON and empty in a table when Jira does not return it.

`--limit` has a default of 50, and 0 gets all pages. The command asks for pages
of 50 with `startAt`. A page with fewer than 50 users is the last page. The
command stops after 20 requests, which is the documented limit of 1000 users.
A cut list writes a warning to stderr. An empty result gives "No users found"
for table and markdown, and `[]` for JSON (`render_list_or_empty`).

### What a script can see

- For an email address or `me`, the result text of `jira issue assign`,
  `jira issue watchers add` and `remove` shows the account ID, not the input.
- For an email address or `me`, the `assignee` column of the `jira bulk assign`
  list shows the account ID. The command makes the lookup before the list.
- A value with no `@` that is not `me` has the same requests and the same
  output as before.

## Limitations

- No test ran against a real Jira site. The behaviour of the search comes from
  the document only.
- The document does not say if a search by email address finds a user with a
  hidden email address. The error text does not make that statement.
- An account ID that is the literal `me`, or that contains `@`, cannot pass
  through. The account ID examples in the document do not have these forms.
- The email lookup reads one page of 50 results. If the exact match is not in
  that page, the result is the "more than one match" error.
- If a Jira site returns a page with fewer users than `maxResults` before the
  last page, `jira user search` stops early. The document says the range comes
  after the filter, so this is not expected.
- `jira issue update` has no `--assignee` option. `--lead` and role actor
  options accept an account ID only.

## Tests

Unit tests with `wiremock` in `crates/cli/src/commands/jira/users.rs`:
`one_match_gives_its_account_id`,
`one_match_with_a_hidden_email_gives_its_account_id`,
`a_null_email_address_is_a_hidden_email`,
`one_match_with_a_different_email_is_an_error`,
`zero_matches_is_an_error_that_names_the_search_command`,
`many_matches_is_an_error_that_names_each_match`,
`an_exact_email_among_many_matches_wins`,
`two_users_with_the_same_email_is_an_error`, `me_gives_the_current_user`,
`a_plain_account_id_makes_no_request`, `the_query_is_percent_encoded`,
`fetch_users_follows_full_pages_and_stops_on_a_short_page`,
`fetch_users_cuts_at_the_limit_and_reports_it`.

Unit test in `crates/cli/src/commands/jira/bulk.rs`:
`import_assignees_resolve_with_one_lookup_for_each_value`.

Built-binary tests against a mock Jira in `crates/cli/tests/jira_user_e2e.rs`:
`create_resolves_an_email_assignee_to_the_account_id` (the POST body has
`fields.assignee.id` with the resolved account ID),
`create_with_an_unknown_email_fails_before_the_issue_is_created`,
`create_with_a_plain_account_id_makes_no_lookup`, `assign_resolves_an_email`,
`assign_to_me_uses_the_current_user`,
`assign_with_two_matches_names_them_and_changes_nothing`,
`watchers_add_resolves_an_email`,
`bulk_assign_resolves_an_email_once_for_all_issues`,
`user_search_lists_the_matches_as_json`,
`user_search_with_no_match_gives_an_empty_json_list`,
`user_search_respects_the_limit`.
