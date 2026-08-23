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
/// and `name+=( ... )` array appends (stripping them from the text and
/// recording/extending their tokenized contents in `arrays`), so later
/// `"${name[@]}"` usages can be expanded inline.
fn preprocess(text: &str, source: &str) -> (String, HashMap<String, Vec<String>>) {
    let joined = text.replace("\\\n", " ");

    let mut arrays: HashMap<String, Vec<String>> = HashMap::new();
    let mut out = String::new();
    let mut rest = joined.as_str();

    while let Some(eq_paren) = rest.find("=(") {
        let before = &rest[..eq_paren];
        let is_append = before.ends_with('+');
        let name_search = if is_append {
            &before[..before.len() - 1]
        } else {
            before
        };
        let name_start = name_search
            .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let name = &name_search[name_start..];
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
            .unwrap_or_else(|| panic!("unterminated array assignment for {name} in {source}"));
        let body = &rest[body_start..body_start + close];
        let tokens = tokenize(body);
        if is_append {
            arrays.entry(name.to_string()).or_default().extend(tokens);
        } else {
            arrays.insert(name.to_string(), tokens);
        }

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
fn extract_commands(text: &str, source: &str) -> Vec<Vec<String>> {
    let (joined, arrays) = preprocess(text, source);
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
                let expanded = arrays.get(name).unwrap_or_else(|| {
                    panic!("no array recorded for {name} in {source} (line: {line})")
                });
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
        let source = script.display().to_string();
        for tokens in extract_commands(&text, &source) {
            // tokens[0] is always the literal "atlassian-cli" word.
            let output = Command::new(BIN)
                .arg("--config")
                .arg(&config)
                // Never resolve against the developer's real home.
                .env("HOME", config.parent().unwrap_or_else(|| Path::new(".")))
                .env(
                    "ATLASSIAN_CLI_CONFIG_DIR",
                    config.parent().unwrap_or_else(|| Path::new(".")),
                )
                .env_remove("XDG_CONFIG_HOME")
                .env_remove("ATLASSIAN_API_TOKEN")
                .env_remove("ATLASSIAN_BITBUCKET_TOKEN")
                .env_remove("BITBUCKET_TOKEN")
                .args(&tokens[1..])
                .env("ATLASSIAN_CLI_TOKEN_T", "x")
                .env("ATLASSIAN_CLI_BITBUCKET_TOKEN_T", "x")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output()
                .expect("failed to run the CLI");

            // Exit code 2 == clap rejected the argv. This is the only kind of
            // regression we catch here: structure/spelling of flags and
            // positionals. We do NOT validate semantic correctness — e.g.
            // free-form `String` args (`--strategy`, `--state`, `--action`)
            // accept any value, and `jq`/JSON-shape assumptions in the scripts
            // are not exercised because the CLI never reaches the network.
            // If you change a response shape or an accepted enum value, add
            // an integration test that hits a mock server; this one won't
            // notice.
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
        let commands = extract_commands(script, "<test>");
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
        let commands = extract_commands(script, "<test>");
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
    fn expands_array_variable_appended_to_after_declaration() {
        let script = concat!(
            "local args=(\"a\" \"b\")\n",
            "args+=(\"c\" \"$VAR\")\n",
            "atlassian-cli \"${args[@]}\"\n",
        );
        let commands = extract_commands(script, "<test>");
        assert_eq!(commands, vec![vec!["atlassian-cli", "a", "b", "c", "1"]]);
    }

    #[test]
    fn ignores_non_invocation_uses_of_the_word() {
        let script = "for cmd in atlassian-cli pandoc jq; do\n    true\ndone\n# atlassian-cli installed and configured\n";
        assert_eq!(
            extract_commands(script, "<test>"),
            Vec::<Vec<String>>::new()
        );
    }

    #[test]
    fn truncates_at_pipe_redirect_and_semicolon() {
        let script = concat!(
            "results=$(atlassian-cli confluence search cql \\\n",
            "    --format json \\\n",
            "    \"$cql\" 2>/dev/null || echo \"[]\")\n",
        );
        let commands = extract_commands(script, "<test>");
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
            extract_commands(script2, "<test>"),
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
