# `confluence page remove-restriction`: the endpoint for one subject

Status: in progress on fix/confluence-remove-restriction-endpoint (PR #PR)

This branch is stacked on `fix/confluence-add-restriction-body`, see
[20112026_confluence_restriction_requests.md](20112026_confluence_restriction_requests.md).
It uses the subject type conversion and the `--subject-id` check from that
branch.

## Problem

`confluence page remove-restriction <id> --operation <op> --subject-type <type>
--subject-id <value>` asks to remove one user or one group from one operation.
The command sent this request:

```
DELETE /wiki/rest/api/content/{id}/restriction?operation=<op>&<type>.identifier=<value>
```

The Confluence Cloud REST API v1 specification
(<https://dac-static.atlassian.com/cloud/confluence/swagger.v3.json>) gives
that endpoint only one query parameter, `expand`. Its description is:

> Removes all restrictions (read and update) on a piece of content.

JSON pointer: `/paths/~1wiki~1rest~1api~1content~1{id}~1restriction/delete/description`.

The parameters `operation` and `<type>.identifier` are not in the
specification. By the specification, the request that the command sent removes
all restrictions of the page, not the one that the user named. The effect on a
live site is not verified.

The specification has one endpoint for each subject:

- `DELETE /wiki/rest/api/content/{id}/restriction/byOperation/{operationKey}/user`
  with the query parameter `accountId`. Summary: "Remove user from content
  restriction".
- `DELETE /wiki/rest/api/content/{id}/restriction/byOperation/{operationKey}/byGroupId/{groupId}`.
  Summary: "Remove group from content restriction".

`operationKey` has the values `read` and `update`.

## Design notes

- **Path.** `remove_restriction_path` in `confluence/pages.rs` builds one of
  the two `byOperation` paths. The command does not send the DELETE that
  removes all restrictions.
- **Encoding.** The page ID, the operation and the group ID go through
  `encode_path_segment`. A group ID that is empty or contains `/` is refused.
  The account ID goes through the query encoder of `UrlParamsBuilder`.
- **Checks before the request.** clap accepts only `read` and `update` for
  `--operation`, and only `user` and `group` for `--subject-type`
  (`value_parser` on the two options). The values are case-sensitive. `--help`
  shows them. clap refuses other values with its usage error and exit code 2,
  and no request is sent. Before, other values went to the server.
- **`--subject-id`.** The value is trimmed, and the trimmed value is sent. An
  empty or blank value is refused with `--subject-id must not be empty.` and
  exit code 1, and no request is sent.
- **Group ID.** `--subject-id` for a group is the group ID, not the group
  name. The help said "user ID or group name". The specification has no
  restriction endpoint that names a group by its name. A group name is sent as
  a group ID, and the server refuses it.
- **Output.** The success line on stdout does not change.

What a script can see:

- The request removes one subject only.
- A value of `--operation` or `--subject-type` outside the permitted set gives
  a usage error and exit code 2 before a request.
- An empty `--subject-id` gives an error and exit code 1 before a request.

## Limitations

- `remove-restriction` has no confirmation prompt and no dry run, as before.
  `docs/08092026_remaining_hardening.md` lists that as open.
- The CLI does not resolve a group name to a group ID.
- One invocation removes one user or one group for one operation. No command
  removes all restrictions of a page.
- The response body of the two DELETE endpoints is not read. The specification
  says that the body is empty.
- Nothing here is verified against a live Confluence site. One live check on a
  disposable page is advised before a release.

## Tests

`crates/cli/tests/confluence_remove_restriction_e2e.rs` runs the built binary
against `wiremock`. The tests that expect a request also mount the DELETE that
removes all restrictions with an expected count of 0:

- `remove_restriction_for_a_user_uses_the_by_operation_endpoint`
- `remove_restriction_for_a_group_uses_the_group_id_endpoint`
- `an_unknown_operation_or_subject_type_sends_no_request`
- `help_lists_the_permitted_values`
- `an_empty_subject_id_sends_no_request`
- `the_subject_id_is_sent_trimmed`
- `remove_restriction_refuses_a_group_id_with_a_slash`
- `remove_restriction_encodes_the_group_id`
- `a_404_from_the_server_is_a_clear_error`

Unit tests in `crates/cli/src/commands/confluence/pages.rs`
(`restriction_tests`) cover the two remove paths and the refusal of a group ID
that is not one path segment.
