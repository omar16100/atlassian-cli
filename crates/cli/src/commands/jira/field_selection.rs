//! `--fields` on `jira issue get` and `jira issue search`.
//!
//! Jira returns every navigable field on a single issue and `view_issue` threw
//! all but eight of them away, while `search_issues` asked for a hardcoded five.
//! Custom fields, which is where most teams keep the data they actually want,
//! were unreachable without dropping to `jira api`.
//!
//! Two things make this more than a passthrough:
//!
//! - **Names, not just ids.** Nobody knows `customfield_10016` is Story Points.
//!   The display names from `/rest/api/3/field` are accepted, and resolved to
//!   ids before the request. An ambiguous name is an error rather than a guess:
//!   several fields sharing a name is the normal state of a mature Jira site,
//!   and the copies hold different values.
//! - **Order.** The columns come back in the order they were typed, which means
//!   the tabular formats cannot use the renderer's default alphabetical union.
//!
//! JSON and YAML stay exactly what Jira sent. Only the tabular formats flatten
//! wrapper objects into a label, and that is a rendering decision, not a
//! transformation of the data.

use anyhow::{anyhow, bail, Context, Result};
use atlassian_cli_output::OutputFormat;
use serde::Deserialize;
use serde_json::{Map, Value};

use super::utils::JiraContext;

/// Jira's own name for "every field", and what `--fields all` becomes.
const ALL_FIELDS: &str = "*all";

/// The key column, always present and always first.
const KEY_COLUMN: &str = "key";

/// One field definition from `/rest/api/3/field`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FieldDef {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// A requested field, resolved.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedField {
    /// Exactly what the user typed. Used as the column header and the JSON key,
    /// so the two always agree and a script can predict both.
    pub token: String,
    /// What goes in the `fields=` parameter.
    pub id: String,
}

/// What to ask Jira for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FieldSelection {
    /// A wildcard. The response's own keys become the columns, so the order is
    /// Jira's rather than the user's.
    Raw(String),
    /// An explicit, ordered list.
    Explicit(Vec<ResolvedField>),
}

impl FieldSelection {
    /// The value of the `fields` query parameter.
    pub fn query_value(&self) -> String {
        match self {
            FieldSelection::Raw(raw) => raw.clone(),
            FieldSelection::Explicit(fields) => fields
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

/// Whether a token asks for everything, or uses Jira's own wildcard syntax.
///
/// `all` is the documented spelling because a bare `*all` is a glob: zsh, the
/// default shell on macOS, refuses the whole command line with "no matches
/// found" before the CLI ever starts. Anything already starting with `*` or `-`
/// is passed through untouched, which buys `*navigable` and the exclusion form
/// `'*all,-comment'` at no cost.
fn is_wildcard(token: &str) -> bool {
    let trimmed = token.trim();
    trimmed.eq_ignore_ascii_case("all") || trimmed.starts_with('*') || trimmed.starts_with('-')
}

fn wildcard_value(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        ALL_FIELDS.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Whether any token might be a display name, and so needs the field list.
///
/// Only `customfield_<digits>` is unambiguously an id. `summary` and `status`
/// are ids too, but nothing local can prove that: a site can perfectly well
/// have a custom field named "Status", and guessing would resolve to the wrong
/// one silently.
pub(crate) fn needs_field_lookup(tokens: &[String]) -> bool {
    !tokens.iter().all(|token| {
        let token = token.trim();
        token
            .strip_prefix("customfield_")
            .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
    })
}

/// Resolve what the user typed against the site's field definitions.
///
/// Pure, so the interesting behaviour is testable without a mock server.
pub(crate) fn match_fields(tokens: &[String], defs: &[FieldDef]) -> Result<Vec<ResolvedField>> {
    let mut resolved: Vec<ResolvedField> = Vec::with_capacity(tokens.len());

    for token in tokens {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }

        // A repeated field is a repeated column, which is meaningless. Keep the
        // first position rather than erroring: unlike `--field key=value` on
        // create, a duplicate here is not a conflict, just noise.
        if resolved.iter().any(|r| r.token == trimmed) {
            continue;
        }

        resolved.push(resolve_one(trimmed, defs)?);
    }

    if resolved.is_empty() {
        bail!("--fields was given no field names");
    }

    Ok(resolved)
}

fn resolve_one(token: &str, defs: &[FieldDef]) -> Result<ResolvedField> {
    // An id wins over a custom field that happens to share the name. Ids are
    // what scripts pass, and a site cannot create a custom field whose *id*
    // collides with a system one.
    if let Some(def) = defs.iter().find(|d| d.id.eq_ignore_ascii_case(token)) {
        return Ok(ResolvedField {
            token: token.to_string(),
            id: def.id.clone(),
        });
    }

    let by_name: Vec<&FieldDef> = defs
        .iter()
        .filter(|d| d.name.trim().eq_ignore_ascii_case(token))
        .collect();

    match by_name.as_slice() {
        [single] => Ok(ResolvedField {
            token: token.to_string(),
            id: single.id.clone(),
        }),
        [] => Err(anyhow!(
            "Unknown field '{token}'. Run `atlassian-cli jira fields list` to see \
             the fields available on this site."
        )),
        // Several custom fields sharing a display name is ordinary on a site
        // that has been in use for a while, and they belong to different
        // screens and hold different values. Choosing one would be wrong in a
        // way the user cannot see from the output.
        many => {
            let candidates = many
                .iter()
                .map(|d| format!("  {}  {}", d.id, d.name))
                .collect::<Vec<_>>()
                .join("\n");
            Err(anyhow!(
                "Field name '{token}' is ambiguous on this site. Candidates:\n{candidates}\n\
                 Pass the id instead, for example --fields {}.",
                many[0].id
            ))
        }
    }
}

/// Turn the tokens into a selection, fetching the field list only if needed.
pub(crate) async fn resolve_field_ids(
    ctx: &JiraContext<'_>,
    tokens: &[String],
) -> Result<FieldSelection> {
    if let Some(wildcard) = tokens.iter().find(|t| is_wildcard(t)) {
        // One wildcard makes the whole selection a wildcard: mixing `all` with
        // named fields cannot narrow anything, and Jira reads the parameter as
        // one list anyway, so we hand it over whole.
        let value = if tokens.len() == 1 {
            wildcard_value(wildcard)
        } else {
            tokens
                .iter()
                .map(|t| wildcard_value(t))
                .collect::<Vec<_>>()
                .join(",")
        };
        tracing::debug!(fields = %value, "requesting fields by wildcard");
        return Ok(FieldSelection::Raw(value));
    }

    if !needs_field_lookup(tokens) {
        tracing::debug!("every requested field is a custom field id, skipping the field lookup");
        let resolved = tokens
            .iter()
            .map(|t| ResolvedField {
                token: t.trim().to_string(),
                id: t.trim().to_string(),
            })
            .collect();
        return Ok(FieldSelection::Explicit(resolved));
    }

    let defs: Vec<FieldDef> = ctx
        .client
        .get("/rest/api/3/field")
        .await
        .context("Failed to list this site's fields to resolve --fields")?;
    tracing::debug!(count = defs.len(), "fetched field definitions");

    let resolved = match_fields(tokens, &defs)?;
    tracing::debug!(
        fields = %resolved.iter().map(|r| r.id.as_str()).collect::<Vec<_>>().join(","),
        "resolved --fields"
    );
    Ok(FieldSelection::Explicit(resolved))
}

/// One issue as returned when we asked for specific fields.
#[derive(Debug, Deserialize)]
struct RawIssue {
    key: String,
    #[serde(default)]
    fields: Map<String, Value>,
}

/// Build one output row: `key` first, then the requested fields in order.
///
/// A requested field the response does not carry becomes `Null`, which renders
/// as an empty cell. That is the honest answer for a field the site does not
/// have on that issue type, and erroring would make a wide `--fields` list
/// unusable across mixed issue types.
fn project_issue(issue: &RawIssue, selection: &FieldSelection) -> (Value, Vec<String>) {
    let mut row = Map::new();
    let mut columns = vec![KEY_COLUMN.to_string()];
    row.insert(KEY_COLUMN.to_string(), Value::String(issue.key.clone()));

    match selection {
        FieldSelection::Raw(_) => {
            // Jira chose the keys, so it chooses the order too.
            for (name, value) in &issue.fields {
                if name == KEY_COLUMN {
                    continue;
                }
                columns.push(name.clone());
                row.insert(name.clone(), value.clone());
            }
        }
        FieldSelection::Explicit(fields) => {
            for field in fields {
                // `key` is not inside `fields`, and is already the first column.
                if field.token == KEY_COLUMN || field.id == KEY_COLUMN {
                    continue;
                }
                let value = issue.fields.get(&field.id).cloned().unwrap_or(Value::Null);
                if value.is_null() && !issue.fields.contains_key(&field.id) {
                    tracing::warn!(
                        field = %field.id,
                        issue = %issue.key,
                        "requested field is absent from the response"
                    );
                }
                columns.push(field.token.clone());
                row.insert(field.token.clone(), value);
            }
        }
    }

    (Value::Object(row), columns)
}

/// Flatten one Jira field value for a table cell.
///
/// Custom field values are nearly always a wrapper object: a select option is
/// `{"value":"Internal"}`, a user is `{"displayName":"Ada"}`, a priority or a
/// sprint is `{"name":"High"}`. Printing the JSON is lossless and unreadable,
/// so the tabular formats get the label instead. JSON and YAML never call this.
pub(crate) fn friendly_scalar(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => items
            .iter()
            // One level only. An array of arrays is rare enough that recursing
            // would cost more in surprise than it saves in width.
            .map(|item| match item {
                Value::Array(_) => compact(item),
                other => friendly_scalar(other),
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        Value::Object(obj) => {
            // Rich text, which is every description-like field.
            if obj.get("type").and_then(Value::as_str) == Some("doc") {
                return super::issues::extract_adf_text(value);
            }

            // displayName first, so a user object never falls through to a
            // sibling `name`. `key` last: it is an identifier, useful only when
            // nothing more readable exists.
            for label in ["displayName", "name", "value", "filename", "key"] {
                if let Some(text) = obj.get(label).and_then(Value::as_str) {
                    if !text.is_empty() {
                        return text.to_string();
                    }
                }
            }

            // Nothing recognised: show the data rather than lose it.
            compact(value)
        }
    }
}

fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// Apply `friendly_scalar` to every cell, for the formats people read.
fn humanize(row: &Value, format: OutputFormat) -> Value {
    if !matches!(
        format,
        OutputFormat::Table | OutputFormat::Csv | OutputFormat::Markdown
    ) {
        return row.clone();
    }

    match row {
        Value::Object(obj) => Value::Object(
            obj.iter()
                .map(|(k, v)| (k.clone(), Value::String(friendly_scalar(v))))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `jira issue get --fields`.
pub(crate) async fn view_issue_fields(
    ctx: &JiraContext<'_>,
    key: &str,
    tokens: &[String],
) -> Result<()> {
    let selection = resolve_field_ids(ctx, tokens).await?;

    let issue: RawIssue = ctx
        .client
        .get(&format!(
            "/rest/api/3/issue/{key}?fields={}",
            urlencoding::encode(&selection.query_value())
        ))
        .await
        .with_context(|| format!("Failed to fetch issue {key}"))?;

    let (row, columns) = project_issue(&issue, &selection);

    match ctx.renderer.format() {
        // A single issue as a horizontal table is a scroll bar. Two columns of
        // field and value read like the curated view the flag replaces, and the
        // order is just row order, so it needs no column plumbing.
        OutputFormat::Table | OutputFormat::Csv | OutputFormat::Markdown => {
            if ctx.renderer.format() == OutputFormat::Markdown {
                ctx.renderer.render_raw(&format!("# {}\n", issue.key))?;
            }

            let obj = row.as_object().expect("project_issue returns an object");
            let pairs: Vec<Value> = columns
                .iter()
                .filter_map(|column| {
                    obj.get(column).map(|value| {
                        serde_json::json!({
                            "field": column,
                            "value": friendly_scalar(value),
                        })
                    })
                })
                .collect();

            ctx.renderer
                .render_rows_ordered(&pairs, &["field".to_string(), "value".to_string()])
        }
        // Byte for byte what Jira sent, as a flat object: an array of
        // field/value pairs would be hostile to `jq`.
        _ => ctx.renderer.render(&row),
    }
}

/// `jira issue search --fields`, given the JQL the caller already built.
pub(crate) async fn search_rows(
    ctx: &JiraContext<'_>,
    jql: &str,
    limit: usize,
    tokens: &[String],
) -> Result<()> {
    let selection = resolve_field_ids(ctx, tokens).await?;

    #[derive(Deserialize)]
    struct SearchResponse {
        #[serde(default)]
        issues: Vec<RawIssue>,
    }

    let query = format!(
        "/rest/api/3/search/jql?jql={}&maxResults={}&fields={}",
        urlencoding::encode(jql),
        limit.min(1000),
        urlencoding::encode(&selection.query_value())
    );

    let response: SearchResponse = ctx
        .client
        .get(&query)
        .await
        .context("Failed to execute search")?;

    if response.issues.is_empty() {
        ctx.verify_auth().await?;
        tracing::info!("No issues found");
    }

    let mut columns: Vec<String> = vec![KEY_COLUMN.to_string()];
    let mut rows = Vec::with_capacity(response.issues.len());

    for issue in &response.issues {
        let (row, row_columns) = project_issue(issue, &selection);
        // Under a wildcard the columns come from the payload, and issue types
        // differ in which fields they carry, so the set is the union.
        for column in row_columns {
            if !columns.contains(&column) {
                columns.push(column);
            }
        }
        rows.push(humanize(&row, ctx.renderer.format()));
    }

    match ctx.renderer.format() {
        OutputFormat::Table | OutputFormat::Csv | OutputFormat::Markdown => {
            ctx.renderer.render_rows_ordered(&rows, &columns)
        }
        _ => ctx.renderer.render(&rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn defs() -> Vec<FieldDef> {
        vec![
            FieldDef {
                id: "summary".into(),
                name: "Summary".into(),
            },
            FieldDef {
                id: "status".into(),
                name: "Status".into(),
            },
            FieldDef {
                id: "customfield_10016".into(),
                name: "Story Points".into(),
            },
            FieldDef {
                id: "customfield_10020".into(),
                name: "Sprint".into(),
            },
        ]
    }

    fn tokens(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn custom_field_ids_alone_need_no_lookup() {
        assert!(!needs_field_lookup(&tokens(&[
            "customfield_10016",
            "customfield_10020"
        ])));
    }

    #[test]
    fn any_possible_name_triggers_a_lookup() {
        assert!(needs_field_lookup(&tokens(&[
            "customfield_10016",
            "status"
        ])));
        assert!(needs_field_lookup(&tokens(&["summary"])));
        // Not a real id: a site could have a field named exactly this.
        assert!(needs_field_lookup(&tokens(&["customfield_abc"])));
        assert!(needs_field_lookup(&tokens(&["customfield_"])));
    }

    #[test]
    fn all_is_a_wildcard_whatever_its_case() {
        for token in ["all", "ALL", "All", " all "] {
            assert!(is_wildcard(token), "{token} should be a wildcard");
            assert_eq!(wildcard_value(token), ALL_FIELDS);
        }
    }

    #[test]
    fn jira_wildcards_pass_through_untouched() {
        for token in ["*all", "*navigable", "-comment"] {
            assert!(is_wildcard(token));
            assert_eq!(wildcard_value(token), token);
        }
    }

    #[test]
    fn a_field_id_resolves_to_itself() {
        let resolved = match_fields(&tokens(&["summary"]), &defs()).unwrap();
        assert_eq!(resolved[0].id, "summary");
        assert_eq!(resolved[0].token, "summary");
    }

    #[test]
    fn a_display_name_resolves_case_insensitively() {
        let resolved = match_fields(&tokens(&["story points"]), &defs()).unwrap();
        assert_eq!(resolved[0].id, "customfield_10016");
        // The header stays what the user typed, so it matches the JSON key.
        assert_eq!(resolved[0].token, "story points");
    }

    /// A site can name a custom field "Summary". The id must still win, or
    /// every existing script quietly starts reading a different field.
    #[test]
    fn an_id_wins_over_a_custom_field_with_the_same_name() {
        let mut defs = defs();
        defs.push(FieldDef {
            id: "customfield_10099".into(),
            name: "Summary".into(),
        });

        let resolved = match_fields(&tokens(&["summary"]), &defs).unwrap();
        assert_eq!(resolved[0].id, "summary");
    }

    #[test]
    fn an_unknown_name_points_at_the_fields_command() {
        let error = match_fields(&tokens(&["stroy points"]), &defs()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("stroy points"), "{message}");
        assert!(message.contains("jira fields list"), "{message}");
    }

    /// The case a naive `.find()` gets wrong: it would silently pick one of
    /// two fields holding different numbers.
    #[test]
    fn an_ambiguous_name_errors_and_lists_every_candidate() {
        let mut defs = defs();
        defs.push(FieldDef {
            id: "customfield_10032".into(),
            name: "Story Points".into(),
        });

        let error = match_fields(&tokens(&["Story Points"]), &defs).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("ambiguous"), "{message}");
        assert!(message.contains("customfield_10016"), "{message}");
        assert!(message.contains("customfield_10032"), "{message}");
    }

    #[test]
    fn duplicate_tokens_collapse_and_keep_their_first_position() {
        let resolved = match_fields(&tokens(&["status", "summary", "status"]), &defs()).unwrap();
        let ids: Vec<&str> = resolved.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["status", "summary"]);
    }

    #[test]
    fn resolution_preserves_the_order_typed() {
        let resolved =
            match_fields(&tokens(&["Story Points", "status", "summary"]), &defs()).unwrap();
        let ids: Vec<&str> = resolved.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["customfield_10016", "status", "summary"]);
    }

    #[test]
    fn the_query_value_joins_ids_in_order() {
        let selection = FieldSelection::Explicit(
            match_fields(&tokens(&["status", "Story Points"]), &defs()).unwrap(),
        );
        assert_eq!(selection.query_value(), "status,customfield_10016");
        assert_eq!(FieldSelection::Raw("*all".into()).query_value(), "*all");
    }

    fn raw_issue(fields: Value) -> RawIssue {
        RawIssue {
            key: "DEV-1".to_string(),
            fields: fields.as_object().unwrap().clone(),
        }
    }

    #[test]
    fn projection_puts_key_first_and_keeps_the_requested_order() {
        let issue = raw_issue(json!({"status": {"name": "Done"}, "summary": "Fix it"}));
        let selection = FieldSelection::Explicit(
            match_fields(&tokens(&["summary", "status"]), &defs()).unwrap(),
        );

        let (row, columns) = project_issue(&issue, &selection);

        assert_eq!(columns, vec!["key", "summary", "status"]);
        assert_eq!(row["key"], json!("DEV-1"));
        assert_eq!(row["summary"], json!("Fix it"));
    }

    #[test]
    fn projection_yields_null_for_a_field_the_response_omitted() {
        let issue = raw_issue(json!({"summary": "Fix it"}));
        let selection = FieldSelection::Explicit(
            match_fields(&tokens(&["summary", "Story Points"]), &defs()).unwrap(),
        );

        let (row, columns) = project_issue(&issue, &selection);

        assert_eq!(columns, vec!["key", "summary", "Story Points"]);
        assert_eq!(row["Story Points"], Value::Null);
    }

    /// `key` lives at the top level of the response, not inside `fields`, so
    /// asking for it must not produce a second, empty column.
    #[test]
    fn projection_does_not_duplicate_the_key_column() {
        let issue = raw_issue(json!({"summary": "Fix it"}));
        let selection = FieldSelection::Explicit(vec![
            ResolvedField {
                token: "key".into(),
                id: "key".into(),
            },
            ResolvedField {
                token: "summary".into(),
                id: "summary".into(),
            },
        ]);

        let (_, columns) = project_issue(&issue, &selection);

        assert_eq!(columns, vec!["key", "summary"]);
    }

    #[test]
    fn friendly_scalar_passes_scalars_through() {
        assert_eq!(friendly_scalar(&json!("text")), "text");
        assert_eq!(friendly_scalar(&json!(5)), "5");
        assert_eq!(friendly_scalar(&json!(true)), "true");
        assert_eq!(friendly_scalar(&Value::Null), "");
    }

    #[test]
    fn friendly_scalar_reads_the_usual_wrapper_objects() {
        assert_eq!(friendly_scalar(&json!({"value": "Internal"})), "Internal");
        assert_eq!(friendly_scalar(&json!({"name": "High"})), "High");
        assert_eq!(friendly_scalar(&json!({"displayName": "Ada"})), "Ada");
        assert_eq!(friendly_scalar(&json!({"filename": "log.txt"})), "log.txt");
        assert_eq!(friendly_scalar(&json!({"key": "DEV-2"})), "DEV-2");
    }

    /// A user object carries both, and the display name is the readable one.
    #[test]
    fn friendly_scalar_prefers_display_name_over_a_sibling_name() {
        let user = json!({"name": "ada.l", "displayName": "Ada Lovelace"});
        assert_eq!(friendly_scalar(&user), "Ada Lovelace");
    }

    #[test]
    fn friendly_scalar_extracts_rich_text() {
        let adf = json!({
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{"type": "text", "text": "Hello there"}]
            }]
        });
        assert_eq!(friendly_scalar(&adf).trim(), "Hello there");
    }

    #[test]
    fn friendly_scalar_joins_arrays_of_options() {
        let labels = json!([{"value": "one"}, {"value": "two"}]);
        assert_eq!(friendly_scalar(&labels), "one, two");
        assert_eq!(friendly_scalar(&json!(["a", "b"])), "a, b");
        assert_eq!(friendly_scalar(&json!([])), "");
    }

    /// Never lose data: an object with no label we know still shows its
    /// contents, exactly as the renderer would have.
    #[test]
    fn friendly_scalar_falls_back_to_compact_json() {
        let odd = json!({"unexpected": 1});
        assert_eq!(friendly_scalar(&odd), "{\"unexpected\":1}");
    }

    #[test]
    fn humanize_leaves_json_untouched_and_flattens_tables() {
        let row = json!({"points": {"value": "Internal"}});

        let as_json = humanize(&row, OutputFormat::Json);
        assert_eq!(as_json["points"], json!({"value": "Internal"}));

        let as_table = humanize(&row, OutputFormat::Table);
        assert_eq!(as_table["points"], json!("Internal"));
    }
}
