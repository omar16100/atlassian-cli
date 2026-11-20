# Jira assignee email lookup and `jira user search`

Status: in progress on `fix/jira-assignee-email` (PR #188).

## Problem

`jira issue create --assignee dev@example.com` failed with
"assignee: Please select a valid user."

The help text of `--assignee` said "account ID or email". The code sent the
value as the account ID with no lookup (`{"id": "<value>"}` on create,
`{"accountId": "<value>"}` on assign). Jira Cloud identifies a user by account
ID only, so an email address was never valid.

No command listed account IDs, and `jira issue update` had no `--assignee`
option.

## Design notes

### Sources

The facts about the Jira API come from the public Jira Cloud platform REST API
v3 document and its OpenAPI file, read on 2026-11-20:

- "Find users", `GET /rest/api/3/user/search`:
  <https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-user-search/#api-rest-api-3-user-search-get>
- "Get current user", `GET /rest/api/3/myself`:
  <https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-myself/#api-rest-api-3-myself-get>
- "Create issue" and "Edit issue", `POST /rest/api/3/issue` and
  `PUT /rest/api/3/issue/{issueIdOrKey}`:
  <https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issues/>
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
- The `User` schema has `accountType` (`atlassian`, `app` or `customer`) and
  `active`.

What the document says about the assignee in an issue body:

- The request example of "Create issue" has
  `"assignee": {"id": "<account ID>"}` in `fields`.
- The request example of "Edit issue" has `fields`, but no assignee.
- "Assign issue" (`PUT /rest/api/3/issue/{issueIdOrKey}/assignee`) takes
  `{"accountId": "<account ID>"}` as the full body.

### The resolver

`crates/cli/src/commands/jira/users.rs` has `resolve_user(client, value,
option)`. It removes spaces at the start and at the end of the value first.

| Value | Result |
| --- | --- |
| empty | error that names the option, with no request |
| `me` or `@me`, upper or lower case | the user from `GET /rest/api/3/myself` |
| contains `@` | lookup with `GET /rest/api/3/user/search?query=<value>&startAt=0&maxResults=50` |
| all other values | the value as the account ID, with no request |

`encode_query_component` encodes the query value. `resolve_account_id` gives
the account ID only. `resolve_user` also gives the display name, which the
`jira bulk assign` confirmation shows.

The rules for an email lookup are in `pick_user_for_email`:

1. Exactly one result has the same `emailAddress` (case ignored): it wins. This
   is also the rule when the search returns many results.
2. More than one result has the same `emailAddress`: the single account that is
   active and has the type `atlassian` wins. If that does not give exactly one
   account, it is an error that names the accounts.
3. No result has the same `emailAddress`, and there is one result with no
   `emailAddress`: it wins. Jira matched the query, and the email address is
   hidden.
4. No result has the same `emailAddress`, and there is one result that shows a
   different email address: error. The search matches a prefix, so
   `dev@example.com` can find `dev@example.com.test`. That is a different user.
5. Zero results: error. The text names the two documented causes (privacy
   settings, the "Browse users and groups" permission) and
   `atlassian-cli jira user search <query>`.
6. More than one result and no exact match: error.

An error names each match as `Display Name (account ID)`, 10 maximum, then a
count. An inactive account and an account that is not of type `atlassian` get a
tag: `(acc-1, inactive)`, `(acc-2, app)`.

The lookup comes before the write request. An error changes nothing in Jira.

### Decisions

- `@me` is accepted with `me`. `jira issue search --assignee @me` uses that
  form. Without this rule `@me` has an `@`, so it becomes an email lookup with
  no result.
- Rule 4 is stricter than "one match gives its account ID". A wrong assignee is
  worse than an error, and the error names the nearest match.
- Rule 2 needs `active: true` and `accountType: atlassian` in the response. An
  account without these fields does not win.
- `jira issue update --assignee` is new. It sends the assignee in the same
  `PUT /rest/api/3/issue/{key}` as the other fields, as
  `fields.assignee = {"id": "<account ID>"}`. This is the shape that the
  document shows for the `fields` of "Create issue", and it is the shape that
  `jira issue create` sends. The document does not show an assignee in the
  "Edit issue" example.
- `jira issue create` and `jira issue update` run the `--field` collision
  checks before the lookup. `--assignee` with `--field assignee=...` is an
  error with no request. The checks are in `check_create_collisions` and
  `check_update_collisions` in `issues.rs`, and the payload builders call the
  same functions.
- `jira bulk assign` makes one lookup for all issues. It makes the lookup in
  list mode too, so a bad email address fails before `--execute`. The
  confirmation shows the display name with the account ID
  (`Dev One (acc-1)`) when the lookup found a name.
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
| `jira issue update --assignee` | yes, new option | The option did not exist |
| `jira bulk assign --assignee` | yes | Same request for each issue. The README example uses an email address |
| `jira bulk import`, row `assignee` | yes | Same payload as create |
| `jira issue watchers add` and `remove` | yes | The help said "account ID or email" and sent the value unchanged |
| `jira issue search --assignee` | no | The value goes into JQL, not into an account ID field. `@me` becomes `currentUser()` |
| `jira project create` and `update --lead`, `jira components create` and `update --lead` | no | Open. The help says "account ID" only |
| `jira roles add-actor` and `remove-actor --user` | no | Open. Same reason |

### `jira user search <query>`

Output keys: `account_id`, `display_name`, `email`, `account_type`, `active`.
`email` is `null` in JSON and empty in a table when Jira does not return it.

`--limit` has a default of 50, and 0 gets all pages. With a limit, the command
asks for one user more than the limit (`maxResults` is the remaining number, 50
maximum). The extra user shows that the list is cut, and the command does not
print it. Thus a result of exactly `--limit` users gives no warning. A response
with fewer users than `maxResults` is the last page.

The command stops after 20 requests. Two different warnings go to stderr: one
for a list that `--limit` cut, and one for a search that the request maximum
stopped. An empty result gives "No users found" for table and markdown, and
`[]` for JSON (`render_list_or_empty`).

### What a script can see

- For an email address or `me`, the result text of `jira issue assign`,
  `jira issue watchers add` and `remove` shows the account ID, not the input.
- For an email address or `me`, the `assignee` column of the `jira bulk assign`
  list shows the account ID. The command makes the lookup before the list.
- An empty user value, or one that is only spaces, is an error before a
  request. Spaces at the start and at the end of a value are removed.
- All other values with no `@` that are not `me` have the same requests and
  the same output as before.

## Limitations

- No test ran against a real Jira site. The behaviour of the search comes from
  the document only.
- The document does not say if a search by email address finds a user with a
  hidden email address. The error text does not make that statement.
- The document does not show an assignee in the "Edit issue" body. The shape
  `{"id": ...}` comes from the "Create issue" example.
- An account ID that is the literal `me`, or that contains `@`, cannot pass
  through. The account ID examples in the document do not have these forms.
- The email lookup reads one page of 50 results. If the exact match is not in
  that page, the result is the "more than one match" error.
- If a Jira site returns fewer users than `maxResults` before the last page,
  `jira user search` stops early. The document says the range comes after the
  filter, so this is not expected.
- Open: `--lead` of `jira project` and `jira components`, and `--user` of
  `jira roles add-actor` and `remove-actor`, accept an account ID only.

## Tests

Unit tests with `wiremock` in `crates/cli/src/commands/jira/users.rs`:

- Email lookup: `one_match_gives_its_account_id`,
  `one_match_with_a_hidden_email_gives_its_account_id`,
  `a_null_email_address_is_a_hidden_email`,
  `one_match_with_a_different_email_is_an_error`,
  `zero_matches_is_an_error_that_names_the_search_command`,
  `many_matches_is_an_error_that_names_each_match`,
  `an_exact_email_among_many_matches_wins`,
  `two_users_with_the_same_email_is_an_error`,
  `the_one_active_person_wins_among_users_with_the_same_email`,
  `two_active_people_with_the_same_email_is_an_error_with_tags`.
- Value forms: `me_gives_the_current_user`,
  `me_ignores_case_and_outer_spaces`,
  `outer_spaces_do_not_go_into_the_query_or_the_account_id`,
  `an_empty_value_is_an_error_that_names_the_option_with_no_request`,
  `a_plain_account_id_makes_no_request`, `the_query_is_percent_encoded`,
  `resolve_user_gives_the_display_name_for_a_prompt`.
- Paging: `fetch_users_follows_full_pages_and_stops_on_a_short_page`,
  `fetch_users_cuts_at_the_limit_and_reports_it`,
  `fetch_users_asks_for_one_more_than_the_limit`,
  `fetch_users_with_exactly_one_full_page_is_complete`,
  `fetch_users_reports_the_request_budget`.

Unit test in `crates/cli/src/commands/jira/bulk.rs`:
`import_assignees_resolve_with_one_lookup_for_each_value`.

Built-binary tests against a mock Jira in `crates/cli/tests/jira_user_e2e.rs`:

- Create: `create_resolves_an_email_assignee_to_the_account_id` (the POST body
  has `fields.assignee.id` with the resolved account ID),
  `create_with_an_unknown_email_fails_before_the_issue_is_created`,
  `create_with_a_plain_account_id_makes_no_lookup`,
  `create_with_me_uses_the_current_user`,
  `create_with_upper_case_me_uses_the_current_user`,
  `create_trims_the_assignee_before_the_lookup`,
  `an_empty_assignee_is_an_error_before_a_request`,
  `create_checks_the_field_collision_before_the_lookup`.
- Update: `update_resolves_an_email_assignee_in_the_same_request`,
  `update_with_only_an_assignee_is_a_valid_update`,
  `update_checks_the_field_collision_before_the_lookup`.
- Assign: `assign_resolves_an_email`, `assign_to_me_uses_the_current_user`,
  `assign_with_two_matches_names_them_and_changes_nothing`.
- Watchers: `watchers_add_resolves_an_email`,
  `watchers_remove_resolves_an_email`.
- Bulk: `bulk_assign_resolves_an_email_once_for_all_issues`,
  `bulk_assign_list_mode_shows_the_account_id_and_writes_nothing`,
  `bulk_assign_with_an_unknown_email_fails_before_the_list`,
  `bulk_assign_without_yes_names_the_user_in_the_refusal`,
  `bulk_import_dry_run_makes_no_lookup`,
  `bulk_import_creates_nothing_when_one_row_does_not_resolve`.
- User search: `user_search_lists_the_matches_as_json`,
  `user_search_with_no_match_gives_an_empty_json_list`,
  `user_search_respects_the_limit`, `user_search_shows_the_account_type`.
