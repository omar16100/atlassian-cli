# Fix `docs/examples/**/*.sh` CLI Invocations (Issue #105) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every `atlassian-cli` invocation in `docs/examples/**/*.sh` actually parse, fix the two functional bugs in `pr-automation.sh` that [issue #105](https://github.com/omar16100/atlassian-cli/issues/105) reported, and add a regression test so these scripts can't silently rot again.

**Architecture:** A new Rust integration test (sibling to the existing `docs_examples.rs` README checker) statically extracts every `atlassian-cli ...` command line from the example shell scripts — following backslash continuations and `name=(...)`/`"${name[@]}"` array indirection, substituting shell variables with a fixed placeholder — and runs each one against the built binary pointed at an unroutable host, failing if clap rejects the command line (exit code 2). Each of the 10 example scripts is then fixed until the test is green.

**Tech Stack:** Rust (`clap` derive CLI, `cargo test`), Bash (the example scripts under audit).

---

## Background: what issue #105 says, and what auditing it actually turned up

[Issue #105](https://github.com/omar16100/atlassian-cli/issues/105) (filed by the repo owner, `omar16100`, while working on [#102](https://github.com/omar16100/atlassian-cli/issues/102)/[#103](https://github.com/omar16100/atlassian-cli/pull/103)) reports that every `atlassian-cli` invocation in `docs/examples/bitbucket/pr-automation.sh` fails to parse, for two reasons:

1. The script passes the workspace as a **positional** argument (`pr get <ws> <repo> <pr_id>`), but `pr get`/`pr approve`/`pr merge`/`pr list` take `<REPO> <PR_ID>` — the workspace is the global `--workspace` **flag**.
2. `get_approval_count()` pipes `pr get --format json` into `jq '[.participants[] | select(.approved==true)] | length'`, but that command's JSON output has no `participants` field (it exposes an `approvals` count instead). `pr reviewers --format json` (added in #103) is the direct replacement.

The owner's suggested fix has three parts: (1) fix `pr-automation.sh`, (2) **audit the other example scripts for the same class of error**, (3) add a test that parses `atlassian-cli` command lines out of `docs/examples/**/*.sh` so this can't rot silently again.

Auditing turned up more than the issue describes. Building the "does this actually parse" test (as a plain Rust binary check, not a real invocation) against a local build of the CLI showed:

- **Every single command line in all 10 example scripts places `--profile "$PROFILE"` after the product subcommand** (e.g. `atlassian-cli bitbucket pr list --profile "$PROFILE" ...`). `--profile` is a top-level `Cli` field in `crates/cli/src/main.rs` **without** `global = true` (unlike `--format`/`--envelope`, which do have it), so clap only accepts it *before* the subcommand. This was verified directly:

  ```
  $ atlassian-cli bitbucket pr list --profile x myrepo --state OPEN
  error: unexpected argument '--profile' found
  ```

  This is a bigger, more pervasive bug than issue #105 describes — it means literally none of the 10 scripts work as shipped, not just the three `bitbucket` ones. The fix is to move `--profile "$PROFILE"` to right after `atlassian-cli`, before the product subcommand word.
- The same "workspace as positional arg" bug from #105 also exists in `docs/examples/bitbucket/repo-audit.sh` and `docs/examples/bitbucket/branch-cleanup.sh` (both call `bitbucket repo list`/`bitbucket branch list`/`bitbucket permission list`/`bitbucket branch delete` with the workspace as an extra positional).
- `docs/examples/jira/project-cleanup.sh`'s `add_label()` calls `jira bulk label` without the **required** `--action` flag (`action: String` has no default in `crates/cli/src/commands/jira/mod.rs`), which is the same "fails to parse" class of bug, just a missing-required-flag instead of an extra positional.
- The other 6 scripts (`jira/sprint-report.sh`, `jira/bulk-transition.sh`, `confluence/space-report.sh`, `confluence/doc-pipeline.sh`, `confluence/bulk-cleanup.sh`, `confluence/backup-space.sh`) only have the `--profile` placement bug — their positional/flag arity is otherwise correct.

All of this was verified against a real build of the CLI (`cargo run -p atlassian-cli -- ...`) and against the regression test in Task 1 below, both before and after the fixes in Tasks 2–11: the test fails with exactly 31 "unexpected argument '--profile'" errors before any fix, and passes cleanly after all 10 scripts are fixed. **No task below is speculative — every diff has already been applied and verified once while writing this plan, then reverted so the tasks can re-derive it from a clean tree.**

Out of scope: [PR #103](https://github.com/omar16100/atlassian-cli/pull/103)'s review left 5 unrelated follow-up items (a `--all`/`--add` `conflicts_with`, `todo.md` vs `docs/todo.md` conventions, a dated feature doc, testable row-filtering, and a broken `--add` endpoint). Those are a different piece of work; this plan only covers issue #105 and the audit it asked for.

## File Structure

- **Create:** `crates/cli/tests/docs_examples_scripts.rs` — the regression test (extraction/tokenizing helpers + one integration test + unit tests for the helpers).
- **Modify:** `docs/examples/bitbucket/pr-automation.sh` — `--profile` placement, `--workspace` flag, rewrite `get_approval_count()`.
- **Modify:** `docs/examples/bitbucket/repo-audit.sh` — `--profile` placement, `--workspace` flag.
- **Modify:** `docs/examples/bitbucket/branch-cleanup.sh` — `--profile` placement, `--workspace` flag.
- **Modify:** `docs/examples/jira/sprint-report.sh` — `--profile` placement.
- **Modify:** `docs/examples/jira/project-cleanup.sh` — `--profile` placement, missing `--action` flag.
- **Modify:** `docs/examples/jira/bulk-transition.sh` — `--profile` placement.
- **Modify:** `docs/examples/confluence/space-report.sh` — `--profile` placement.
- **Modify:** `docs/examples/confluence/doc-pipeline.sh` — `--profile` placement.
- **Modify:** `docs/examples/confluence/bulk-cleanup.sh` — `--profile` placement.
- **Modify:** `docs/examples/confluence/backup-space.sh` — `--profile` placement.
- **Modify:** `todo.md` — changelog entry (repo convention per [PR #103](https://github.com/omar16100/atlassian-cli/pull/103)'s review).

---

### Task 1: Add the regression test harness

**Files:**
- Create: `crates/cli/tests/docs_examples_scripts.rs`

- [ ] **Step 1: Write the test file**

```rust
//! Checks that every `atlassian-cli` invocation in `docs/examples/**/*.sh`
//! actually parses.
//!
//! Mirrors `docs_examples.rs` (which does the same for README.md) but has to
//! cope with real shell scripting: backslash line continuations, `name=(...)`
//! array variables later expanded with `"${name[@]}"`, and shell variables
//! standing in for flag/positional values. Values are irrelevant to argument
//! *parsing*, so every bare `$VAR` / `${VAR}` token is replaced with a fixed
//! placeholder before the line is handed to the same "does clap accept this"
//! check the README test uses.
//!
//! This is intentionally not a full shell parser: it understands exactly the
//! patterns the example scripts use (quoting, backslash escapes inside
//! double quotes, array assignment + `[@]` expansion, and truncating at the
//! first unquoted pipe/redirect/statement-separator). Extend the helpers
//! below if a future example script needs a pattern they don't yet cover.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_atlassian-cli");
/// Stands in for every shell variable. Numeric so it satisfies `i64`/`usize`
/// typed arguments (e.g. a PR id or `--limit`) as well as string ones.
const PLACEHOLDER: &str = "1";

/// Quote- and backslash-aware split of a string into shell-style words.
/// A backslash always escapes the next character (including inside double
/// quotes, e.g. `\"`), matching how these scripts quote things. This is not
/// a full POSIX shell lexer, just enough for the patterns actually used
/// under `docs/examples/`.
fn tokenize(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        if c == '\\' && quote != Some('\'') {
            if let Some(next) = chars.next() {
                current.push(next);
                started = true;
            }
            continue;
        }
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started || !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(c);
                started = true;
            }
        }
    }
    if started || !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Joins backslash-newline continuations so a multi-line shell command
/// becomes one logical line, and extracts `name=( ... )` array assignments
/// (stripping them from the text and recording their tokenized contents),
/// so later `"${name[@]}"` usages can be expanded inline.
fn preprocess(text: &str) -> (String, HashMap<String, Vec<String>>) {
    let joined = text.replace("\\\n", " ");

    let mut arrays = HashMap::new();
    let mut out = String::new();
    let mut rest = joined.as_str();

    while let Some(eq_paren) = rest.find("=(") {
        let before = &rest[..eq_paren];
        let name_start = before
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let name = &before[name_start..];
        let is_array_assignment = name
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_');

        if !is_array_assignment {
            out.push_str(&rest[..eq_paren + 2]);
            rest = &rest[eq_paren + 2..];
            continue;
        }

        let body_start = eq_paren + 2;
        let close = rest[body_start..]
            .find(')')
            .unwrap_or_else(|| panic!("unterminated array assignment for {name}"));
        let body = &rest[body_start..body_start + close];
        arrays.insert(name.to_string(), tokenize(body));

        out.push_str(before);
        rest = &rest[body_start + close + 1..];
    }
    out.push_str(rest);

    (out, arrays)
}

/// True if `token` is nothing but a bare shell variable reference, e.g.
/// `$WORKSPACE` or `${WORKSPACE}` (quotes are already stripped by
/// [`tokenize`]). Variables embedded inside a longer string (e.g. a CQL
/// query) are left untouched — their content doesn't affect parseability.
fn is_bare_variable(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('$') else {
        return false;
    };
    let inner = match rest.strip_prefix('{') {
        Some(braced) => braced.strip_suffix('}').unwrap_or(braced),
        None => rest,
    };
    !inner.is_empty() && inner.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// If `token` is an array expansion like `${args[@]}`, returns the array
/// name (`args`).
fn array_expansion_name(token: &str) -> Option<&str> {
    token.strip_prefix("${")?.strip_suffix("[@]}")
}

/// Shell contexts after which `atlassian-cli` starts a new command (as
/// opposed to appearing as a plain word, e.g. in `for cmd in atlassian-cli
/// pandoc jq` or inside a comment/string).
fn is_command_position(prefix: &str) -> bool {
    let trimmed = prefix.trim();
    trimmed.is_empty()
        || trimmed.ends_with("$(")
        || trimmed.ends_with('(')
        || trimmed.ends_with(';')
        || trimmed.ends_with("&&")
        || trimmed.ends_with("||")
        || trimmed.ends_with('|')
        || trimmed.ends_with('!')
        || matches!(trimmed, "if" | "elif" | "while" | "until" | "do")
}

/// Extracts every `atlassian-cli ...` invocation from a shell script,
/// expanding array-variable usages and substituting bare variable
/// references with [`PLACEHOLDER`]. Each invocation is truncated at the
/// first unquoted `;`, `|`, `)` or (file-descriptor-prefixed) `>`, so
/// statement separators, pipelines, redirects, and the closing paren of a
/// `$(...)` command substitution don't get parsed as CLI arguments.
fn extract_commands(text: &str) -> Vec<Vec<String>> {
    let (joined, arrays) = preprocess(text);
    let mut commands = Vec::new();

    for line in joined.lines() {
        let Some(start) = line.find("atlassian-cli") else {
            continue;
        };
        let word_end = start + "atlassian-cli".len();
        let boundary_before = start == 0
            || !matches!(line.as_bytes()[start - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-');
        let boundary_after = word_end == line.len()
            || !matches!(line.as_bytes()[word_end], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-');
        if !boundary_before || !boundary_after || !is_command_position(&line[..start]) {
            continue;
        }

        let mut end = line.len();
        let mut quote: Option<char> = None;
        let bytes = line.as_bytes();
        let mut chars = line[start..].char_indices();
        while let Some((i, c)) = chars.next() {
            if c == '\\' {
                chars.next();
                continue;
            }
            match quote {
                Some(q) if c == q => quote = None,
                Some(_) => {}
                None if c == '\'' || c == '"' => quote = Some(c),
                None if c == ';' || c == '|' || c == ')' => {
                    end = start + i;
                    break;
                }
                None if c == '>' => {
                    let mut cut = start + i;
                    while cut > start && bytes[cut - 1].is_ascii_digit() {
                        cut -= 1;
                    }
                    end = cut;
                    break;
                }
                None => {}
            }
        }

        let raw_tokens = tokenize(&line[start..end]);
        let mut expanded_tokens = Vec::new();
        for tok in raw_tokens {
            if let Some(name) = array_expansion_name(&tok) {
                let expanded = arrays
                    .get(name)
                    .unwrap_or_else(|| panic!("no array recorded for {name} (line: {line})"));
                expanded_tokens.extend(expanded.iter().cloned());
            } else {
                expanded_tokens.push(tok);
            }
        }

        let tokens = expanded_tokens
            .into_iter()
            .map(|tok| {
                if is_bare_variable(&tok) {
                    PLACEHOLDER.to_string()
                } else {
                    tok
                }
            })
            .collect();
        commands.push(tokens);
    }

    commands
}

fn example_scripts() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples")
        .canonicalize()
        .expect("docs/examples not found");

    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("failed to read docs/examples") {
            let entry = entry.expect("failed to read dir entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "sh") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_example_script_command_parses() {
    let dir = TempDir::new().unwrap();
    let config = dir.path().join("config.yaml");
    // Port 1 refuses instantly, so a command that parses fails at connect
    // rather than doing anything.
    std::fs::write(
        &config,
        "default_profile: t\nprofiles:\n  t:\n    email: a@b.c\n    base_url: http://127.0.0.1:1\n    workspace: w\n",
    )
    .unwrap();

    let scripts = example_scripts();
    assert!(
        scripts.len() >= 10,
        "expected to find the docs/examples scripts, found {}",
        scripts.len()
    );

    let mut failures = Vec::new();
    for script in &scripts {
        let text = std::fs::read_to_string(script).unwrap();
        for tokens in extract_commands(&text) {
            // tokens[0] is always the literal "atlassian-cli" word.
            let output = Command::new(BIN)
                .arg("--config")
                .arg(&config)
                .args(&tokens[1..])
                .env("ATLASSIAN_CLI_TOKEN_T", "x")
                .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_T", "x")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output()
                .expect("failed to run the CLI");

            if output.status.code() == Some(2) {
                let reason = String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                failures.push(format!(
                    "  {}: {}\n      -> {reason}",
                    script.display(),
                    tokens[1..].join(" ")
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} docs/examples commands do not parse:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod extraction_tests {
    use super::*;

    #[test]
    fn tokenize_splits_simple_words() {
        assert_eq!(tokenize("a b  c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn tokenize_strips_surrounding_quotes() {
        assert_eq!(
            tokenize(r#""$WORKSPACE" 'literal'"#),
            vec!["$WORKSPACE", "literal"]
        );
    }

    #[test]
    fn tokenize_handles_escaped_quotes_inside_double_quotes() {
        assert_eq!(
            tokenize(r#""space = $key AND title = \"$title\"""#),
            vec![r#"space = $key AND title = "$title""#]
        );
    }

    #[test]
    fn detects_bare_variables() {
        assert!(is_bare_variable("$WORKSPACE"));
        assert!(is_bare_variable("${WORKSPACE}"));
        assert!(!is_bare_variable("space=$key"));
        assert!(!is_bare_variable("--workspace"));
        assert!(!is_bare_variable("OPEN"));
    }

    #[test]
    fn detects_array_expansion() {
        assert_eq!(array_expansion_name("${args[@]}"), Some("args"));
        assert_eq!(array_expansion_name("$args"), None);
    }

    #[test]
    fn extracts_simple_invocation_and_substitutes_variables() {
        let script = "atlassian-cli bitbucket pr approve \\\n    --workspace \"$WORKSPACE\" \\\n    \"$REPO\" \\\n    \"$pr_id\"\n";
        let commands = extract_commands(script);
        assert_eq!(
            commands,
            vec![vec![
                "atlassian-cli",
                "bitbucket",
                "pr",
                "approve",
                "--workspace",
                "1",
                "1",
                "1",
            ]]
        );
    }

    #[test]
    fn expands_array_variable_usage() {
        let script = concat!(
            "local args=(\n",
            "    \"--profile\" \"$PROFILE\"\n",
            "    \"jira\" \"bulk\" \"transition\"\n",
            "    \"--jql\" \"$JQL\"\n",
            ")\n",
            "atlassian-cli \"${args[@]}\"\n",
        );
        let commands = extract_commands(script);
        assert_eq!(
            commands,
            vec![vec![
                "atlassian-cli",
                "--profile",
                "1",
                "jira",
                "bulk",
                "transition",
                "--jql",
                "1",
            ]]
        );
    }

    #[test]
    fn ignores_non_invocation_uses_of_the_word() {
        let script = "for cmd in atlassian-cli pandoc jq; do\n    true\ndone\n# atlassian-cli installed and configured\n";
        assert_eq!(extract_commands(script), Vec::<Vec<String>>::new());
    }

    #[test]
    fn truncates_at_pipe_redirect_and_semicolon() {
        let script = concat!(
            "results=$(atlassian-cli confluence search cql \\\n",
            "    --format json \\\n",
            "    \"$cql\" 2>/dev/null || echo \"[]\")\n",
        );
        let commands = extract_commands(script);
        assert_eq!(
            commands,
            vec![vec![
                "atlassian-cli",
                "confluence",
                "search",
                "cql",
                "--format",
                "json",
                "1"
            ]]
        );

        let script2 = "if atlassian-cli confluence attachment download --output \"$OUT\"; then\n";
        assert_eq!(
            extract_commands(script2),
            vec![vec![
                "atlassian-cli",
                "confluence",
                "attachment",
                "download",
                "--output",
                "1"
            ]]
        );
    }
}
```

- [ ] **Step 2: Run the unit tests for the extraction helpers**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts extraction_tests`
Expected: `test result: ok. 9 passed; 0 failed; ...` — these test `tokenize`, `is_bare_variable`, `array_expansion_name`, and `extract_commands` in isolation and don't depend on the (still broken) example scripts.

- [ ] **Step 3: Run the integration test to see the current failures**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture`
Expected: **FAILS** with `31 docs/examples commands do not parse`, every one reporting `error: unexpected argument '--profile' found`, spanning all 10 files under `docs/examples/`. This is the expected red state — do not try to fix this by editing the test. Tasks 2–11 fix it by editing the scripts.

Do not commit yet — a script fix will be bundled into the first script-fix commit (Task 2) so no commit in this plan leaves a known-red test sitting on its own.

### Task 2: Fix `docs/examples/jira/sprint-report.sh`

**Files:**
- Modify: `docs/examples/jira/sprint-report.sh:120-124`

- [ ] **Step 1: Move `--profile` before the subcommand**

In `fetch_issues()`:

```diff
     atlassian-cli jira issue search \
-        --profile "$PROFILE" \
+    atlassian-cli --profile "$PROFILE" jira issue search \
         --jql "$jql" \
         --format json
```

i.e. replace:

```bash
    atlassian-cli jira issue search \
        --profile "$PROFILE" \
        --jql "$jql" \
        --format json
```

with:

```bash
    atlassian-cli --profile "$PROFILE" jira issue search \
        --jql "$jql" \
        --format json
```

- [ ] **Step 2: Verify this file's failures are gone**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep sprint-report.sh`
Expected: no output (no more failures reference `sprint-report.sh`). The overall test still fails (other scripts aren't fixed yet) — that's expected until Task 11.

- [ ] **Step 3: Commit**

```bash
git add crates/cli/tests/docs_examples_scripts.rs docs/examples/jira/sprint-report.sh
git commit -m "test: add docs/examples script parse-check; fix(docs): jira/sprint-report.sh --profile placement"
```

### Task 3: Fix `docs/examples/jira/project-cleanup.sh`

**Files:**
- Modify: `docs/examples/jira/project-cleanup.sh:118-168`

This script has three separate bugs: two `--profile` placements, and a missing required `--action` flag on `jira bulk label`.

- [ ] **Step 1: Fix `preview_issues()`**

Replace:

```bash
    issues=$(atlassian-cli jira issue search \
        --profile "$PROFILE" \
        --jql "$jql" \
        --format json 2>/dev/null || echo "[]")
```

with:

```bash
    issues=$(atlassian-cli --profile "$PROFILE" jira issue search \
        --jql "$jql" \
        --format json 2>/dev/null || echo "[]")
```

- [ ] **Step 2: Fix `add_label()` — profile placement and missing `--action`**

`jira bulk label` requires `--action` (`add`, `remove`, or `set`); the script was omitting it entirely, which is the same "fails to parse" bug as a missing required argument. Since this function only ever adds a label, the action is `add`.

Replace:

```bash
    atlassian-cli jira bulk label \
        --profile "$PROFILE" \
        --jql "$jql" \
        --labels "$label" \
        --concurrency "$CONCURRENCY"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" jira bulk label \
        --jql "$jql" \
        --action add \
        --labels "$label" \
        --concurrency "$CONCURRENCY"
```

- [ ] **Step 3: Fix `generate_report()`**

Replace:

```bash
    issues=$(atlassian-cli jira issue search \
        --profile "$PROFILE" \
        --jql "$jql" \
        --format json)
```

with:

```bash
    issues=$(atlassian-cli --profile "$PROFILE" jira issue search \
        --jql "$jql" \
        --format json)
```

- [ ] **Step 4: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep project-cleanup.sh`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add docs/examples/jira/project-cleanup.sh
git commit -m "fix(docs): jira/project-cleanup.sh --profile placement and missing --action"
```

### Task 4: Fix `docs/examples/jira/bulk-transition.sh`

**Files:**
- Modify: `docs/examples/jira/bulk-transition.sh:97-137`

- [ ] **Step 1: Fix `preview_issues()`**

Replace:

```bash
    issues=$(atlassian-cli jira issue search \
        --profile "$PROFILE" \
        --jql "$JQL" \
        --format json 2>/dev/null || echo "[]")
```

with:

```bash
    issues=$(atlassian-cli --profile "$PROFILE" jira issue search \
        --jql "$JQL" \
        --format json 2>/dev/null || echo "[]")
```

- [ ] **Step 2: Fix `execute_transition()`'s `args` array — `--profile` must come before the subcommand words**

Replace:

```bash
    local args=(
        "jira" "bulk" "transition"
        "--profile" "$PROFILE"
        "--jql" "$JQL"
        "--transition" "$TRANSITION"
        "--concurrency" "$CONCURRENCY"
    )
```

with:

```bash
    local args=(
        "--profile" "$PROFILE"
        "jira" "bulk" "transition"
        "--jql" "$JQL"
        "--transition" "$TRANSITION"
        "--concurrency" "$CONCURRENCY"
    )
```

(`atlassian-cli "${args[@]}"` on the next line is unchanged — the array itself is what needed reordering.)

- [ ] **Step 3: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep bulk-transition.sh`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/jira/bulk-transition.sh
git commit -m "fix(docs): jira/bulk-transition.sh --profile placement"
```

### Task 5: Fix `docs/examples/confluence/space-report.sh`

**Files:**
- Modify: `docs/examples/confluence/space-report.sh:39-90`

Four functions, each just needs `--profile "$PROFILE"` moved before the subcommand.

- [ ] **Step 1: Fix `get_space_stats()`**

Replace:

```bash
    stats=$(atlassian-cli confluence analytics space-stats \
        --profile "$PROFILE" \
        "$SPACE_KEY" \
        --format json)
```

with:

```bash
    stats=$(atlassian-cli --profile "$PROFILE" confluence analytics space-stats \
        "$SPACE_KEY" \
        --format json)
```

- [ ] **Step 2: Fix `get_all_pages()`**

Replace:

```bash
    pages=$(atlassian-cli confluence page list \
        --profile "$PROFILE" \
        --space "$SPACE_KEY" \
        --limit 1000 \
        --format json)
```

with:

```bash
    pages=$(atlassian-cli --profile "$PROFILE" confluence page list \
        --space "$SPACE_KEY" \
        --limit 1000 \
        --format json)
```

- [ ] **Step 3: Fix `get_page_views()`**

Replace:

```bash
    atlassian-cli confluence analytics page-views \
        --profile "$PROFILE" \
        "$page_id" \
        --format json 2>/dev/null | \
        jq -r '.view_count // 0' || echo "0"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence analytics page-views \
        "$page_id" \
        --format json 2>/dev/null | \
        jq -r '.view_count // 0' || echo "0"
```

- [ ] **Step 4: Fix `get_page_details()`**

Replace:

```bash
    atlassian-cli confluence page get \
        --profile "$PROFILE" \
        "$page_id" \
        --format json
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence page get \
        "$page_id" \
        --format json
```

- [ ] **Step 5: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep space-report.sh`
Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add docs/examples/confluence/space-report.sh
git commit -m "fix(docs): confluence/space-report.sh --profile placement"
```

### Task 6: Fix `docs/examples/confluence/doc-pipeline.sh`

**Files:**
- Modify: `docs/examples/confluence/doc-pipeline.sh:96-153`

Three fixes. Note `get_or_create_page()`'s `search cql` call (lines 77–80) has no `--profile` at all — that's fine, `--profile` is optional and falls back to the config's `default_profile`; leave it as-is.

- [ ] **Step 1: Fix the `create_args` array in `get_or_create_page()`**

Replace:

```bash
    local create_args=(
        "confluence" "page" "create"
        "--profile" "$PROFILE"
        "--space" "$space_key"
        "--title" "$title"
    )
```

with:

```bash
    local create_args=(
        "--profile" "$PROFILE"
        "confluence" "page" "create"
        "--space" "$space_key"
        "--title" "$title"
    )
```

- [ ] **Step 2: Fix `update_page()`**

Replace:

```bash
    atlassian-cli confluence page update \
        --profile "$PROFILE" \
        "$page_id" \
        --title "$title" \
        --body "$temp_file"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence page update \
        "$page_id" \
        --title "$title" \
        --body "$temp_file"
```

- [ ] **Step 3: Fix `add_labels()`**

Replace:

```bash
        atlassian-cli confluence page add-label \
            --profile "$PROFILE" \
            "$page_id" \
            "$label" || warn "Failed to add label: $label"
```

with:

```bash
        atlassian-cli --profile "$PROFILE" confluence page add-label \
            "$page_id" \
            "$label" || warn "Failed to add label: $label"
```

- [ ] **Step 4: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep doc-pipeline.sh`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add docs/examples/confluence/doc-pipeline.sh
git commit -m "fix(docs): confluence/doc-pipeline.sh --profile placement"
```

### Task 7: Fix `docs/examples/confluence/bulk-cleanup.sh`

**Files:**
- Modify: `docs/examples/confluence/bulk-cleanup.sh:106-186`

Four invocations across `preview_pages()`, `add_labels_bulk()`, and `delete_pages_bulk()` (which has two).

- [ ] **Step 1: Fix `preview_pages()`**

Replace:

```bash
    results=$(atlassian-cli confluence search cql \
        --profile "$PROFILE" \
        --format json \
        "$cql" 2>/dev/null || echo "[]")
```

with:

```bash
    results=$(atlassian-cli --profile "$PROFILE" confluence search cql \
        --format json \
        "$cql" 2>/dev/null || echo "[]")
```

- [ ] **Step 2: Fix `add_labels_bulk()`**

Replace:

```bash
    atlassian-cli confluence bulk add-labels \
        --profile "$PROFILE" \
        --cql "$cql" \
        --labels "$LABEL_NAME" \
        --concurrency 4
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence bulk add-labels \
        --cql "$cql" \
        --labels "$LABEL_NAME" \
        --concurrency 4
```

- [ ] **Step 3: Fix both invocations in `delete_pages_bulk()`**

Replace:

```bash
    count=$(atlassian-cli confluence search cql \
        --profile "$PROFILE" \
        --format json \
        "$cql" | jq '. | length')
```

with:

```bash
    count=$(atlassian-cli --profile "$PROFILE" confluence search cql \
        --format json \
        "$cql" | jq '. | length')
```

and replace:

```bash
    atlassian-cli confluence bulk delete \
        --profile "$PROFILE" \
        --cql "$cql" \
        --concurrency 2
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence bulk delete \
        --cql "$cql" \
        --concurrency 2
```

- [ ] **Step 4: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep bulk-cleanup.sh`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add docs/examples/confluence/bulk-cleanup.sh
git commit -m "fix(docs): confluence/bulk-cleanup.sh --profile placement"
```

### Task 8: Fix `docs/examples/confluence/backup-space.sh`

**Files:**
- Modify: `docs/examples/confluence/backup-space.sh:50-131`

Five invocations: `export_metadata()`, `export_pages()`, `export_blogs()`, and two in `download_attachments()`.

- [ ] **Step 1: Fix `export_metadata()`**

Replace:

```bash
    atlassian-cli confluence space get \
        --profile "$PROFILE" \
        "$SPACE_KEY" \
        --format json > "$BACKUP_DIR/space_info.json"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence space get \
        "$SPACE_KEY" \
        --format json > "$BACKUP_DIR/space_info.json"
```

- [ ] **Step 2: Fix `export_pages()`**

Replace:

```bash
    atlassian-cli confluence bulk export \
        --profile "$PROFILE" \
        --cql "$cql" \
        --output "$BACKUP_DIR/pages.json" \
        --format json
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence bulk export \
        --cql "$cql" \
        --output "$BACKUP_DIR/pages.json" \
        --format json
```

- [ ] **Step 3: Fix `export_blogs()`**

Replace:

```bash
    atlassian-cli confluence bulk export \
        --profile "$PROFILE" \
        --cql "$cql" \
        --output "$BACKUP_DIR/blogposts.json" \
        --format json
```

with:

```bash
    atlassian-cli --profile "$PROFILE" confluence bulk export \
        --cql "$cql" \
        --output "$BACKUP_DIR/blogposts.json" \
        --format json
```

- [ ] **Step 4: Fix both invocations in `download_attachments()`**

Replace:

```bash
        attachments=$(atlassian-cli confluence attachment list \
            --profile "$PROFILE" \
            "$page_id" \
            --format json 2>/dev/null || echo "[]")
```

with:

```bash
        attachments=$(atlassian-cli --profile "$PROFILE" confluence attachment list \
            "$page_id" \
            --format json 2>/dev/null || echo "[]")
```

and replace:

```bash
            if atlassian-cli confluence attachment download \
                --profile "$PROFILE" \
                "$att_id" \
                --output "$BACKUP_DIR/attachments/$safe_filename"; then
```

with:

```bash
            if atlassian-cli --profile "$PROFILE" confluence attachment download \
                "$att_id" \
                --output "$BACKUP_DIR/attachments/$safe_filename"; then
```

- [ ] **Step 5: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep backup-space.sh`
Expected: no output.

- [ ] **Step 6: Commit**

```bash
git add docs/examples/confluence/backup-space.sh
git commit -m "fix(docs): confluence/backup-space.sh --profile placement"
```

### Task 9: Fix `docs/examples/bitbucket/repo-audit.sh`

**Files:**
- Modify: `docs/examples/bitbucket/repo-audit.sh:40-58`

Same class of bug as issue #105's `pr-automation.sh` report: workspace passed positionally instead of via `--workspace`, plus the `--profile` placement bug.

- [ ] **Step 1: Fix `get_all_repos()`**

`bitbucket repo list` takes **no** positional argument (workspace is resolved via the global `--workspace` flag). Replace:

```bash
    atlassian-cli bitbucket repo list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        --format json
}
```

with:

```bash
    atlassian-cli --profile "$PROFILE" bitbucket repo list \
        --workspace "$WORKSPACE" \
        --format json
}
```

- [ ] **Step 2: Fix `get_repo_permissions()`**

`bitbucket permission list` takes exactly one positional (`repo`); the workspace must be a flag. Replace:

```bash
    atlassian-cli bitbucket permission list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$repo" \
        --format json 2>/dev/null || echo "[]"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" bitbucket permission list \
        --workspace "$WORKSPACE" \
        "$repo" \
        --format json 2>/dev/null || echo "[]"
```

- [ ] **Step 3: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep repo-audit.sh`
Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add docs/examples/bitbucket/repo-audit.sh
git commit -m "fix(docs): bitbucket/repo-audit.sh workspace flag and --profile placement"
```

### Task 10: Fix `docs/examples/bitbucket/branch-cleanup.sh`

**Files:**
- Modify: `docs/examples/bitbucket/branch-cleanup.sh:111-152`

Same class of bug as Task 9, across three functions.

- [ ] **Step 1: Fix `get_repos()`**

Replace:

```bash
        atlassian-cli bitbucket repo list \
            --profile "$PROFILE" \
            "$WORKSPACE" \
            --format json | jq -r '[.[].slug]'
```

with:

```bash
        atlassian-cli --profile "$PROFILE" bitbucket repo list \
            --workspace "$WORKSPACE" \
            --format json | jq -r '[.[].slug]'
```

- [ ] **Step 2: Fix `get_merged_branches()`**

`bitbucket branch list` takes exactly one positional (`repo`). Replace:

```bash
    branches=$(atlassian-cli bitbucket branch list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$repo" \
        --format json)
```

with:

```bash
    branches=$(atlassian-cli --profile "$PROFILE" bitbucket branch list \
        --workspace "$WORKSPACE" \
        "$repo" \
        --format json)
```

- [ ] **Step 3: Fix `delete_branch()`**

`bitbucket branch delete` takes exactly two positionals (`repo`, `branch`). Replace:

```bash
    atlassian-cli bitbucket branch delete \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$repo" \
        "$branch" || warn "Failed to delete: $branch"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" bitbucket branch delete \
        --workspace "$WORKSPACE" \
        "$repo" \
        "$branch" || warn "Failed to delete: $branch"
```

- [ ] **Step 4: Verify**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts every_example_script_command_parses -- --nocapture 2>&1 | grep branch-cleanup.sh`
Expected: no output.

- [ ] **Step 5: Commit**

```bash
git add docs/examples/bitbucket/branch-cleanup.sh
git commit -m "fix(docs): bitbucket/branch-cleanup.sh workspace flag and --profile placement"
```

### Task 11: Fix `docs/examples/bitbucket/pr-automation.sh` (the script issue #105 actually names)

**Files:**
- Modify: `docs/examples/bitbucket/pr-automation.sh:104-177`

This is the script from the issue itself: `--profile`/workspace bugs in all four functions, plus the `get_approval_count()` rewrite the issue explicitly suggests.

- [ ] **Step 1: Fix `get_open_prs()`**

`bitbucket pr list` takes exactly one positional (`repo`). Replace:

```bash
    atlassian-cli bitbucket pr list \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        --state OPEN \
        --format json
```

with:

```bash
    atlassian-cli --profile "$PROFILE" bitbucket pr list \
        --workspace "$WORKSPACE" \
        "$REPO" \
        --state OPEN \
        --format json
```

- [ ] **Step 2: Rewrite `get_approval_count()` to use `pr reviewers`, per the issue's suggested fix**

`pr get --format json` exposes an `approvals` **count** field, not a `participants` array — `[.participants[] | select(.approved==true)] | length` was querying a field that doesn't exist. `bb pr reviewers <repo> <pr_id> --format json` (added in [#103](https://github.com/omar16100/atlassian-cli/pull/103)) returns rows shaped `{name, role, status, participated_on}` where `status` is `"Approved"`, `"Changes Requested"`, or `"No Response"` — that's the direct replacement the issue names.

Replace:

```bash
# Get PR approval count
get_approval_count() {
    local pr_id="$1"

    atlassian-cli bitbucket pr get \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        "$pr_id" \
        --format json | \
        jq '[.participants[] | select(.approved == true)] | length'
}
```

with:

```bash
# Get PR approval count
get_approval_count() {
    local pr_id="$1"

    atlassian-cli --profile "$PROFILE" bitbucket pr reviewers \
        --workspace "$WORKSPACE" \
        "$REPO" \
        "$pr_id" \
        --format json | \
        jq '[.[] | select(.status == "Approved")] | length'
}
```

- [ ] **Step 3: Fix `approve_pr()`**

Replace:

```bash
    atlassian-cli bitbucket pr approve \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        "$pr_id"
```

with:

```bash
    atlassian-cli --profile "$PROFILE" bitbucket pr approve \
        --workspace "$WORKSPACE" \
        "$REPO" \
        "$pr_id"
```

- [ ] **Step 4: Fix `merge_pr()`**

Replace:

```bash
    atlassian-cli bitbucket pr merge \
        --profile "$PROFILE" \
        "$WORKSPACE" \
        "$REPO" \
        "$pr_id" \
        --strategy merge
```

with:

```bash
    atlassian-cli --profile "$PROFILE" bitbucket pr merge \
        --workspace "$WORKSPACE" \
        "$REPO" \
        "$pr_id" \
        --strategy merge
```

- [ ] **Step 5: Verify the entire test suite is green**

Run: `cargo test -p atlassian-cli --test docs_examples_scripts -- --nocapture`
Expected: `test result: ok. 10 passed; 0 failed; ...` (9 `extraction_tests` + `every_example_script_command_parses`), with zero failures listed.

- [ ] **Step 6: Commit**

```bash
git add docs/examples/bitbucket/pr-automation.sh
git commit -m "fix(docs): bitbucket/pr-automation.sh workspace flag, --profile placement, and approval-count query (closes #105)"
```

### Task 12: Full verification, formatting, and changelog

**Files:**
- Modify: `todo.md` (append an entry; check the exact top-of-file format first with `head -20 todo.md`, since it may have changed since this plan was written)

- [ ] **Step 1: Run the full crate test suite**

Run: `cargo test -p atlassian-cli`
Expected: all tests pass, including `docs_examples::every_readme_command_parses` and `docs_examples_scripts::every_example_script_command_parses`.

- [ ] **Step 2: Format and lint the new test file**

Run: `cargo fmt -p atlassian-cli && cargo clippy -p atlassian-cli --tests --all-features -- -D warnings`
Expected: `cargo fmt` reformats `crates/cli/tests/docs_examples_scripts.rs` (a couple of long method-chain/vec-literal lines get wrapped); clippy reports no warnings. If `cargo fmt` changes anything, re-run Step 1 to confirm tests still pass, then amend the Task 11 commit or add a small follow-up `style:` commit — whichever this repo's history convention favors (check `git log --oneline -- crates/cli/tests/docs_examples_scripts.rs`).

- [ ] **Step 3: Add a changelog entry**

Per [PR #103](https://github.com/omar16100/atlassian-cli/pull/103)'s review, changelog entries for this repo go in the root `todo.md` (a running log), not `docs/todo.md` (the forward-looking roadmap). Read the top of `todo.md` to match its exact heading style, then prepend a new `## YYYY-MM-DD — ...` entry (use today's date) summarizing: all 10 `docs/examples/**/*.sh` scripts had `--profile` in the wrong position (clap only accepts it before the product subcommand) and three additionally had positional/required-flag arity bugs (`pr-automation.sh`, `repo-audit.sh`, `branch-cleanup.sh` treated workspace as positional; `project-cleanup.sh` was missing `jira bulk label`'s required `--action`); `pr-automation.sh`'s approval count also queried a nonexistent `participants` field and now uses `pr reviewers --format json` instead. Mention the new `crates/cli/tests/docs_examples_scripts.rs` regression test that catches this class of bug going forward. Closes #105.

- [ ] **Step 4: Commit**

```bash
git add todo.md
git commit -m "docs(todo): record docs/examples script fixes for #105"
```

---

## Self-review notes (for whoever executes this plan)

- **Every diff in this plan has already been applied to a clean checkout and verified**: unit tests for the extraction helpers pass (9/9), the integration test correctly reports exactly 31 failures before any script fix (all `--profile` placement errors) and 0 failures after all 10 scripts are fixed, `cargo clippy --tests --all-features -- -D warnings` is clean, and `cargo test -p atlassian-cli --test docs_examples` (the pre-existing README check) still passes throughout. The working tree was reverted afterward so the tasks above re-derive the same result from scratch.
- If a future `docs/examples/**/*.sh` script or CLI flag changes and this test starts panicking with `no array recorded for ...` or asserting on an unexpected shape, that means a script uses a shell pattern the extractor in Task 1 doesn't understand yet (e.g. a new array, a new redirect form) — extend `preprocess`/`extract_commands`, don't disable the check.
- Do **not** "fix" `docs/examples/confluence/doc-pipeline.sh`'s `search cql` call (lines 77–80) by adding `--profile` — it's intentionally relying on the config's `default_profile`, which is valid, and adding it isn't part of what issue #105 asked for.
