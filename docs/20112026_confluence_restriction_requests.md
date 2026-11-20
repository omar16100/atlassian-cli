# `confluence page add-restriction`: the documented request body

Status: in progress on fix/confluence-add-restriction-body (PR #PR)

## Problem

`confluence page add-restriction` failed with the server message "Required
request body is missing".

The command sent this body to `POST /wiki/rest/api/content/{id}/restriction`:

```json
{
  "operation": "update",
  "restrictions": { "user": [{ "type": "user", "identifier": "<value>" }] }
}
```

The Confluence Cloud REST API v1 specification
(<https://dac-static.atlassian.com/cloud/confluence/swagger.v3.json>, schema
`ContentRestrictionAddOrUpdateArray`) does not have this shape:

- The top level is an object with a required `results` array, or a plain
  array. Each entry is one operation. The command sent one bare entry.
- A user entry (`ContentRestrictionUserInput`) requires `accountId`. A group
  entry (`ContentRestrictionGroupInput`) requires `id`. The command sent
  `identifier` for the two types.
- `type` is optional. Its permitted values are `known` for a user and `group`
  for a group. The command sent `user` for a user.

The HTTP client did send the body, with the JSON content type
(`ApiClient::post` calls `reqwest`'s `json`). The specification does not say
why the server reports a wrong body as a missing body. That part of the cause
is not verified.

## Design notes

- **Body.** `restriction_payload` in `confluence/pages.rs` builds the body of
  the specification example:

  ```json
  {
    "results": [{
      "operation": "update",
      "restrictions": { "user": { "results": [{ "accountId": "<value>" }] } }
    }]
  }
  ```

  A group uses `"group": { "results": [{ "id": "<value>" }] }`. The optional
  `type` field is not sent.
- **Checks before the request.** `--operation` must be `read` or `update`, and
  `--subject-type` must be `user` or `group`. These are the sets in the
  specification. Other values are refused with an error and no request is
  sent. Before, other values went to the server.
- **Help text.** `--subject-id` is a user account ID or a group ID. The help
  said "user ID or group name". The specification names a group by its ID.
- **Method and path.** `POST /wiki/rest/api/content/{id}/restriction` is
  correct and does not change. The specification says that POST adds to the
  existing restrictions.
- **Output.** The success line on stdout does not change.

**This change is verified against the public REST specification and against a
mock server. It is NOT verified against a live Confluence site.**

## Limitations

- `--subject-id` for a group must be the group ID. The CLI does not resolve a
  group name to an ID.
- One invocation adds one user or one group for one operation.
- `get-restrictions` sends `GET /wiki/rest/api/content/{id}/restriction`, which
  agrees with the specification. It is not changed. It does not send the page
  ID through `encode_path_segment`.

## Tests

`crates/cli/tests/confluence_add_restriction_e2e.rs` runs the built binary
against `wiremock`. The mock matches the method, the path and the exact JSON
body:

- `add_restriction_for_a_user_sends_the_documented_body`
- `add_restriction_for_a_group_sends_the_group_id`
- `an_unknown_operation_or_subject_type_sends_no_request`

Unit tests in `crates/cli/src/commands/confluence/pages.rs`
(`restriction_tests`) cover the two bodies and the two checks.
