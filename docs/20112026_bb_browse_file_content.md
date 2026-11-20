Status: in progress on feat/bb-browse-file-content (PR #PR)

# `bb commit browse` prints the content of a file

## Problem

No command read a file from a Bitbucket repository. `bb commit browse --path
<file>` failed with "Invalid response format", because `browse_source` parsed
each response as a JSON directory list. Callers used raw
`bb api /2.0/repositories/.../src/{rev}/{path}` requests as a replacement.

## What changes for users

- A `--path` that is a file writes the file content to stdout. The bytes are
  not changed and no newline is added.
- A `--path` that is a directory, and no `--path`, list the entries as before.
  The output keys (`file_type`, `path`, `size`) are the same.
- `--output <file>` (`-o`) writes the content to a file. `--output -` is stdout.
  An existing file is not replaced unless `--force` is given.
- A path or revision that does not exist gives an error that names the path,
  the revision and the repository, with the message that Bitbucket sent.
- A branch or tag name that contains `/` (for example `feature/login`) works
  as `--commit`. The command resolves it to a commit hash first.
- `bb commit browse ... | head -1` ends with no error when the reader closes
  the pipe.

```bash
atlassian-cli bb commit browse web_app --commit main --path src            # list
atlassian-cli bb commit browse web_app --commit main --path src/main.rs    # print
atlassian-cli bb commit browse web_app --commit main --path logo.png -o ./logo.png
```

## Source

The public Bitbucket Cloud REST description (`https://api.bitbucket.org/swagger.json`,
read on 2026-11-20) documents
`GET /repositories/{workspace}/{repo_slug}/src/{commit}/{path}`. The same text
is on `https://developer.atlassian.com/cloud/bitbucket/rest/api-group-source/`.
It says:

- "When `path` points to a file, this endpoint returns the raw contents. The
  response's Content-Type is derived from the filename extension (not from the
  contents). The file contents are not processed and no character
  encoding/recoding is performed".
- "When the request for a file path includes the query parameter
  `?format=meta`, instead of returning the file's raw contents, Bitbucket
  instead returns the JSON object describing the file's properties". The
  example object has `"type": "commit_file"`.
- "When `path` points to a directory instead of a file, the response is a
  paginated list of directory and file objects". A directory object has
  `"type": "commit_directory"`, and its `meta` link "returns the actual
  `directory` object itself".
- "When listing the contents of the repo's root directory, the use of a
  trailing slash at the end of the URL is required."
- "If the file is managed by LFS, then a 301 redirect pointing to Atlassian's
  media services platform is returned."
- Status 404: "If the path or commit in the URL does not exist."
- The `commit` path parameter is "The commit's SHA1." The `path` parameter is
  "Path to the file."

The same description documents `GET
/repositories/{workspace}/{repo_slug}/refs/branches/{name}` ("Returns a branch
object within the specified repository", and "the branch name should not
include any prefixes (e.g. refs/heads)") and `GET
/repositories/{workspace}/{repo_slug}/refs/tags/{name}` ("Returns the specified
tag"). The two example answers have a `target` object with a `hash`. Status 404
is "The specified repository or branch does not exist." and "The specified
repository or tag does not exist."

The description does not state:

- how a `/` in a branch or tag name must be written in the `{name}` path
  parameter, or in the `{commit}` parameter of the `src` endpoint;
- what a `?format=meta` request gives for a directory path with no trailing
  slash (the meta link of a directory in the example has a trailing slash).

## Design notes

**How the command tells a file from a directory.** With a `--path`, the command
sends `GET .../src/{commit}/{path}?format=meta` first. The answer is always
JSON, and its `type` is `commit_file` or `commit_directory`. The command then
sends the content request or the list request. Each other `type` is an error
that names the value.

The Content-Type of the response is not used for this decision. Bitbucket
derives it from the file name, so a `.json` file comes back as JSON, and such a
file can have a `values` key. A test covers this case.

**A 404 from the meta request.** The API client keeps no body for a 404, so
the error of the meta request cannot say if the commit or the path is the
cause. After a 404, the command sends the list request again with no status
mapping and reads the body. If the answer is an error, the user error has the
`error.message` text that Bitbucket sent (`Bitbucket says: ...`). If the answer
is a directory list, the command lists it. The list part is a defensive step:
the description does not state what a meta request with no trailing slash gives
for a directory. The additional request occurs on the 404 path only.

**A revision that contains `/`.** The `src` endpoint has one path segment for
the revision, so the name `feature/login` cannot go into that path. When
`--commit` contains `/`, the command sends `GET .../refs/branches/{name}` and
reads `target.hash`. If the answer is a 404, it sends `GET
.../refs/tags/{name}`. The hash goes into the `src` requests. If the two
requests give a 404, the error names the revision and says that it is not a
branch or a tag. A revision with no `/` sends no refs request.

The name goes into the refs path through `encode_ref_path`: the `/` stays and
each other reserved character is percent-encoded. `bb branch get`, `bb branch
delete` and `bb bulk delete-branches` build the same path in the same way. The
description does not state this form, and it was not verified on a live site.

With no `--path` (or `--path /`), the target is the repository root. The root is
always a directory, so the command sends the list request only, with the
trailing slash that the document requires.

**Bytes.** The content request uses the existing `ApiClient::get_bytes`. No
method was added to `crates/api`. The command writes the bytes with
`write_all` and does not convert them to a string.

**`--format` in file mode.** `--format` has no effect on the content of a
file. With no `--output <file>`, stdout gets the file bytes and nothing else
for each format, the same as `jira attachment download --output -`. With
`--output <file>`, the confirmation on stdout obeys `--format`. A JSON wrapper cannot hold bytes that are
not UTF-8 without a change to them, and the callers in the evidence wanted the
content. Logs go to stderr.

**`--output`.** The rules are those of `jira attachment download --output`:
`-` is stdout, another value is used as the file path, and an existing file is
refused unless `--force` is given. The command looks for the file before it
sends a request, so a refused target costs no download. The file is then opened
with `create_new`, which is the real guard: the check and the write are one
operation. `write_bytes` moved from
`commands/jira/attachments.rs` to `commands/common.rs` so that the two commands
share it. After a write, the command prints a confirmation through
`render_success` (a `success`/`message` object in JSON).

**Broken pipe.** A `BrokenPipe` error from the write to stdout is a success. In
`bb commit browse ... | head -1` the reader closes the pipe when it has what it
needs. `jira attachment download` is not changed.

`--output <file>` with a directory path is an error. The command refuses after
the meta request and writes no file. `--force` requires `--output`.

**Request path.** The workspace and the repository slug go through
`encode_path_segment`. The revision goes through `encode_ref_path`, which
refuses `.` and `..` components and a leading or trailing `/`. A commit hash
that Bitbucket gives for a ref goes through `encode_path_segment`. The file path is split at `/`, and
each segment goes through `encode_path_segment`. Leading and trailing `/` are
removed. An empty segment, `.` and `..` are refused before a request is sent.
Before this change, these four values were not encoded in this command.

**Module.** The code moved from `commands/bitbucket/commits.rs` to the new
`commands/bitbucket/source.rs`.

## Limitations

- A path costs two requests (meta, then content or list). The root costs one.
  A revision that contains `/` costs one or two more (branch, then tag).
- No request to a live Bitbucket site was made for this work. The description
  does not state what `?format=meta` gives for a directory path with no
  trailing slash, or how a `/` in a ref name must be written in the refs path.
- A branch and a tag with the same name: the branch is used.
- A failed write can leave a partial `--output` file.
- `--output` with the path of an existing directory gives the message "already
  exists. Use --force to overwrite.", and with `--force` a write error. The
  message does not say that the target is a directory.
- No test covers a Git LFS redirect, a symbolic link or a submodule entry.
- The content is held in memory before it is written. The client timeout of 30
  seconds applies, so a very large file can fail.
- Git LFS files are not verified. The document says that Bitbucket answers
  with a 301 redirect to a different host. The HTTP client follows redirects,
  but no test and no live request covers this case.
- A directory list shows the first page only, as before. This change does not
  add pagination.
- A symbolic link and a submodule are files for Bitbucket. The command prints
  what Bitbucket sends (the link target, or the commit hash of the submodule).
- `bb commit browse` has no option for `format=rendered`.
- `%`, `?` and `#` in `--path` and `--commit` are sent as literal characters.
  A value that is already percent-encoded (`a%20b`) and a query suffix
  (`src/?pagelen=100`) do not work as they did before.
- A `--path` with a `.`, `..` or empty segment (for example `a//b`) is refused.
  A `--commit` with a leading or trailing `/`, or a `.` or `..` component, is
  refused. A workspace or repository value with `/` is refused.

## Tests

Unit tests in `crates/cli/src/commands/bitbucket/source.rs` use a `wiremock`
server and an `ApiClient` that points to it. The Bitbucket base URL is a
constant in the binary (`BITBUCKET_API_URL`) with no override, so a test cannot
point the built binary to a mock server. For this reason these tests call the
command function, and they give it a buffer in the place of stdout.

- `file_path_sends_the_exact_bytes_to_stdout_in_each_format`: bytes that are not
  valid UTF-8 reach stdout with no change, for table, JSON, YAML, CSV and quiet.
- `json_file_with_a_values_key_is_content_not_a_list`.
- `empty_file_writes_nothing`.
- `directory_path_lists_the_entries_and_writes_no_file_bytes` (table and JSON).
  It also compares the JSON keys of the rows that go to the renderer.
- `no_path_lists_the_root_with_one_request`.
- `path_with_a_space_and_nested_directories_is_encoded_by_segment`.
- `revision_with_a_slash_is_resolved_to_the_branch_hash`,
  `revision_with_a_slash_is_resolved_to_the_tag_hash_when_no_branch_exists`,
  `revision_with_a_slash_that_is_not_a_branch_or_a_tag_is_an_error` and
  `revision_with_no_slash_makes_no_refs_request`.
- `not_found_error_carries_the_message_of_the_server`.
- `existing_output_file_without_force_sends_no_request`.
- `broken_pipe_on_stdout_is_not_an_error`.
- `dot_segments_and_bad_identifiers_send_no_request`.
- `missing_path_gives_an_error_that_names_the_path_and_revision` (404).
- `directory_lists_when_the_meta_request_gives_a_404`.
- `output_file_gets_the_bytes_and_stdout_gets_none_of_them`,
  `output_file_is_not_replaced_without_force`, `output_dash_is_stdout`,
  `output_file_with_a_directory_path_is_an_error_and_writes_no_file`.
- `unknown_meta_type_is_an_error`.
- `encode_source_path_rules`.

Built-binary tests in `crates/cli/tests/bb_browse_cli_e2e.rs` cover what does
not need a server: the help text, the `--force` rule, and that a refused path
writes nothing to stdout.
