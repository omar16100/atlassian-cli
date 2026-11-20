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
  the revision and the repository.

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

## Design notes

**How the command tells a file from a directory.** With a `--path`, the command
sends `GET .../src/{commit}/{path}?format=meta` first. The answer is always
JSON, and its `type` is `commit_file` or `commit_directory`. The command then
sends the content request or the list request. Each other `type` is an error
that names the value.

The Content-Type of the response is not used for this decision. Bitbucket
derives it from the file name, so a `.json` file comes back as JSON, and such a
file can have a `values` key. A test covers this case.

**A 404 from the meta request.** The document shows the meta link of a
directory with a trailing slash only (`.../tests/test_project/?format=meta`).
The command sends the path with no trailing slash. If the meta request gives a
404, the command sends the list request that older releases sent, and lists the
directory if the answer is a list. If not, the error is the 404 error. This
keeps a directory list correct if Bitbucket refuses the meta form with no
slash. The additional request occurs on the 404 path only.

With no `--path` (or `--path /`), the target is the repository root. The root is
always a directory, so the command sends the list request only, with the
trailing slash that the document requires.

**Bytes.** The content request uses the existing `ApiClient::get_bytes`. No
method was added to `crates/api`. The command writes the bytes with
`write_all` and does not convert them to a string.

**`--format` in file mode.** `--format` has no effect when the path is a file.
Stdout gets the file bytes and nothing else for each format, the same as
`jira attachment download --output -`. A JSON wrapper cannot hold bytes that are
not UTF-8 without a change to them, and the callers in the evidence wanted the
content. Logs go to stderr.

**`--output`.** The rules are those of `jira attachment download --output`:
`-` is stdout, another value is used as the file path, and an existing file is
refused unless `--force` is given. The file is opened with `create_new`, so the
check and the write are one operation. `write_bytes` moved from
`commands/jira/attachments.rs` to `commands/common.rs` so that the two commands
share it. After a write, the command prints a confirmation through
`render_success` (a `success`/`message` object in JSON).

`--output <file>` with a directory path is an error. The command refuses after
the meta request and writes no file. `--force` requires `--output`.

**Request path.** The workspace and the repository slug go through
`encode_path_segment`. The revision goes through `encode_ref_path`, which keeps
`/` and refuses `.` and `..` components. The file path is split at `/`, and
each segment goes through `encode_path_segment`. Leading and trailing `/` are
removed. An empty segment, `.` and `..` are refused before a request is sent.
Before this change, these four values were not encoded in this command.

**Module.** The code moved from `commands/bitbucket/commits.rs` to the new
`commands/bitbucket/source.rs`.

## Limitations

- A path costs two requests (meta, then content or list). The root costs one.
- The answer of Bitbucket to `?format=meta` for a directory path with no
  trailing slash is not verified on a live site. The 404 path above covers the
  case in which Bitbucket refuses it.
- The content is held in memory before it is written. The client timeout of 30
  seconds applies, so a very large file can fail.
- Git LFS files are not verified. The document says that Bitbucket answers
  with a 301 redirect to a different host. The HTTP client follows redirects,
  but no test and no live request covers this case.
- A branch name that contains `/` is sent with the `/` kept, as before. The
  document does not say how Bitbucket separates such a revision from the path.
  This was not verified on a live site. A commit hash is not ambiguous.
- A directory list shows the first page only, as before. This change does not
  add pagination.
- A symbolic link and a submodule are files for Bitbucket. The command prints
  what Bitbucket sends (the link target, or the commit hash of the submodule).
- `bb commit browse` has no option for `format=rendered`.
- A path with a `.`, `..` or empty segment (for example `a//b`) is now refused.
  A revision with a `?` or `#` is now encoded.

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
- `directory_path_lists_the_entries_and_writes_no_file_bytes` (table and JSON)
  and `directory_rows_keep_the_existing_json_keys`.
- `no_path_lists_the_root_with_one_request`.
- `path_with_a_space_and_nested_directories_is_encoded_by_segment`.
- `branch_name_with_a_slash_keeps_the_slash`.
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
