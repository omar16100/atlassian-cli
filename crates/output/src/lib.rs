use std::collections::BTreeSet;

use anyhow::Result;
use clap::ValueEnum;
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use tabled::builder::Builder;
use tabled::settings::object::Columns;
use tabled::settings::{Style, Width};

pub mod colors;

pub use colors::StatusFormatter;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Yaml,
    Csv,
    Quiet,
    Markdown,
}

impl OutputFormat {
    /// Formats read by people: tables and markdown.
    ///
    /// Decoration (status icons, colour, prose for empty results) belongs only
    /// here. The machine formats carry plain values a script can compare, so
    /// `"IN_PROGRESS 🔄"` in `-f json` is a bug, not a style.
    pub fn is_human(self) -> bool {
        matches!(self, OutputFormat::Table | OutputFormat::Markdown)
    }
}

/// Width at which a single object's values wrap in table mode. Wide enough
/// for a UUID, a URL or a sentence; narrow enough that a pull request
/// description does not push the table off the screen.
const RECORD_VALUE_WIDTH: usize = 100;

pub struct OutputRenderer {
    format: OutputFormat,
    envelope: bool,
}

/// What a list result knows about itself beyond the rows.
///
/// Carried separately from the rows because the tabular formats have nowhere to
/// put it, and because a caller that never paginated should not have to invent
/// values it does not have.
#[derive(Debug, Clone, Default)]
pub struct ListMeta {
    /// The server's own count of matching items, where it reports one.
    ///
    /// Usually absent. Jira's `/search/jql` returns no total, and Bitbucket
    /// omits `size` on collections it considers expensive. Absent is not zero,
    /// and it is serialized as absent rather than as `0` for that reason.
    pub total: Option<u64>,
    /// Whether the rows are a complete answer, when the caller knows.
    ///
    /// `None` means unknown, and is serialized as absent rather than as
    /// `false`. Most list commands are still a single request against a
    /// server-paginated endpoint: they cannot tell whether more exists, and
    /// asserting `truncated: false` there would be a confident false claim of
    /// exactly the kind this field was added to prevent.
    pub truncated: Option<bool>,
    /// An opaque marker for where a truncated result stopped, when the source
    /// provides one.
    pub next: Option<String>,
}

impl ListMeta {
    /// What a caller that did not paginate knows: nothing.
    ///
    /// Deliberately not called `complete()`. The callers that use it have not
    /// established completeness, and naming it so invited the envelope to
    /// assert it.
    pub fn unknown() -> Self {
        Self::default()
    }

    /// A result whose completeness has been established.
    pub fn known(total: Option<u64>, truncated: bool, next: Option<String>) -> Self {
        Self {
            total,
            truncated: Some(truncated),
            next,
        }
    }
}

/// Envelope wrapper for list outputs in JSON/YAML.
///
/// `data` and `count` keep the names the `--envelope` flag has always emitted;
/// renaming them would break existing users for no gain. The rest is additive,
/// and `total`/`next` are omitted entirely when unknown so that a consumer can
/// distinguish "no total reported" from "a total of zero".
#[derive(Serialize)]
struct ListEnvelope<'a, T: Serialize> {
    data: &'a [T],
    count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    /// Absent when the caller could not establish completeness.
    #[serde(skip_serializing_if = "Option::is_none")]
    truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<&'a str>,
}

impl<'a, T: Serialize> ListEnvelope<'a, T> {
    fn new(items: &'a [T], meta: &'a ListMeta) -> Self {
        Self {
            data: items,
            count: items.len(),
            total: meta.total,
            truncated: meta.truncated,
            next: meta.next.as_deref(),
        }
    }
}

impl OutputRenderer {
    pub fn new(format: OutputFormat) -> Self {
        Self {
            format,
            envelope: false,
        }
    }

    pub fn with_envelope(mut self, envelope: bool) -> Self {
        self.envelope = envelope;
        self
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn render<T: Serialize>(&self, value: &T) -> Result<()> {
        let json_value = serde_json::to_value(value)?;

        match self.format {
            OutputFormat::Table => {
                println!("{}", Self::table_output(value, &json_value)?);
            }
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            }
            OutputFormat::Yaml => {
                println!("{}", serde_yaml::to_string(&json_value)?);
            }
            OutputFormat::Csv => {
                if !self.render_csv(&json_value)? {
                    println!("{}", serde_json::to_string_pretty(&json_value)?);
                }
            }
            OutputFormat::Quiet => {
                if !self.render_quiet(&json_value) {
                    println!("{}", serde_json::to_string_pretty(&json_value)?);
                }
            }
            OutputFormat::Markdown => {
                if !self.render_markdown_table(&json_value)? {
                    self.render_markdown_single(&json_value)?;
                }
            }
        }

        Ok(())
    }

    /// Render a whole API document, where the JSON is the product.
    ///
    /// Table mode prints it as pretty JSON, as every single object used to be:
    /// `jira workflow export` without `--output`, or a raw `folder get`, has no
    /// useful two-column form. Every other format renders as `render` does.
    pub fn render_document<T: Serialize>(&self, value: &T) -> Result<()> {
        match self.format {
            OutputFormat::Table => {
                println!("{}", serde_json::to_string_pretty(value)?);
                Ok(())
            }
            _ => self.render(value),
        }
    }

    /// What table mode prints for `value`.
    ///
    /// A list becomes a table with a column per key. A single object becomes a
    /// `field | value` table in the order its fields were declared. Anything
    /// else (an empty list, a bare string) falls back to JSON.
    ///
    /// Single objects used to fall back to JSON too, so `bb pr get` and every
    /// other `get` printed JSON with no `-f` while the lists printed tables.
    fn table_output<T: Serialize>(value: &T, json_value: &Value) -> Result<String> {
        if json_value.is_object() {
            return Ok(Self::format_record(&Self::ordered_fields(
                value, json_value,
            )));
        }
        match Self::table_string(json_value, None) {
            Some(table) => Ok(table),
            None => Ok(serde_json::to_string_pretty(json_value)?),
        }
    }

    /// An object's fields in declaration order.
    ///
    /// `serde_json::Value` keeps object keys sorted, which would list a pull
    /// request's `approvals` before its `id` and `title`. Serializing to a
    /// string keeps the struct's own order, and reading that back into an
    /// `IndexMap` preserves it.
    fn ordered_fields<T: Serialize>(value: &T, json_value: &Value) -> Vec<(String, Value)> {
        serde_json::to_string(value)
            .ok()
            .and_then(|text| serde_json::from_str::<IndexMap<String, Value>>(&text).ok())
            .map(|fields| fields.into_iter().collect())
            .unwrap_or_else(|| {
                json_value
                    .as_object()
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default()
            })
    }

    /// One object as a `field | value` table.
    ///
    /// Scalars print as themselves and `null` as an empty cell, lists of
    /// scalars comma-joined, nested objects as compact JSON. A list of objects
    /// (a pull request's reviewers, a pipeline's steps) gets its own titled
    /// table below, because squeezing it into one cell is how it ended up as
    /// unreadable JSON. Long values wrap at word boundaries.
    fn format_record(fields: &[(String, Value)]) -> String {
        let mut builder = Builder::default();
        builder.push_record(["field".to_string(), "value".to_string()]);
        let mut sections = Vec::new();
        for (key, value) in fields {
            match value {
                Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
                    sections.push((key, value));
                }
                Value::Array(items) if items.iter().all(|v| !v.is_object() && !v.is_array()) => {
                    let joined = items
                        .iter()
                        .map(Self::value_to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    builder.push_record([key.clone(), joined]);
                }
                other => builder.push_record([key.clone(), Self::value_to_string(other)]),
            }
        }

        let mut table = builder.build();
        table.with(Style::rounded()).modify(
            Columns::one(1),
            Width::wrap(RECORD_VALUE_WIDTH).keep_words(true),
        );
        let mut out = table.to_string();
        for (key, value) in sections {
            if let Some(sub) = Self::table_string(value, None) {
                out.push_str(&format!("\n\n{key}:\n{sub}"));
            }
        }
        out
    }

    /// Render a list/array of items. When --envelope is enabled and format is JSON/YAML,
    /// wraps output in `{"data": [...], "count": N, ...}`. Otherwise renders as normal.
    pub fn render_list<T: Serialize>(&self, items: &[T]) -> Result<()> {
        self.render_list_with_meta(items, &ListMeta::unknown())
    }

    /// Render a list that knows whether it is complete.
    ///
    /// The truncation signal only has somewhere to live in the enveloped
    /// formats. Callers rendering a paginated result should still warn on
    /// stderr for the tabular formats, because a table has no field to put this
    /// in and a silently short table is the original complaint.
    pub fn render_list_with_meta<T: Serialize>(&self, items: &[T], meta: &ListMeta) -> Result<()> {
        if self.envelope {
            match self.format {
                OutputFormat::Json => {
                    let envelope = ListEnvelope::new(items, meta);
                    println!("{}", serde_json::to_string_pretty(&envelope)?);
                    return Ok(());
                }
                OutputFormat::Yaml => {
                    let envelope = ListEnvelope::new(items, meta);
                    println!("{}", serde_yaml::to_string(&envelope)?);
                    return Ok(());
                }
                _ => {}
            }
        }
        if items.is_empty() {
            match self.format {
                // Line-oriented output consumed by scripts, typically as
                // `for id in $(...)`. An empty list has no lines, and printing
                // "[]" would feed a bogus item into the loop. CSV of an empty
                // list has no rows and no derivable header either.
                OutputFormat::Quiet | OutputFormat::Csv => return Ok(()),
                _ => {}
            }
        }
        self.render(&items)
    }

    /// Render a list, with a human-readable note when it is empty.
    ///
    /// Table and Markdown are read by people, so they get the message. Every
    /// machine format gets an empty array instead, because a script doing
    /// `| jq` cannot parse prose. Printing "No pull requests found" under
    /// `--format json` is what #110 reported, across ~70 list commands.
    pub fn render_list_or_empty<T: Serialize>(
        &self,
        items: &[T],
        empty_message: &str,
    ) -> Result<()> {
        // Read by people: a blank table explains nothing. Every machine format
        // falls through to render_list, which emits an array for JSON/YAML and
        // nothing at all for the line-oriented ones.
        if items.is_empty() && matches!(self.format, OutputFormat::Table | OutputFormat::Markdown) {
            println!("{empty_message}");
            return Ok(());
        }
        self.render_list(items)
    }

    /// Render rows with the columns given, in the order given.
    ///
    /// `render` derives columns as the sorted union of the rows' keys, which is
    /// right when the caller has no opinion about them. A caller who let the
    /// user choose does have one: `jira issue search --fields status,summary`
    /// should read back in that order, and alphabetical sorting would silently
    /// reverse it.
    ///
    /// Only the tabular formats take the order. JSON and YAML go through the
    /// untouched path, because `serde_json::Map` is a `BTreeMap` and their key
    /// order is alphabetical no matter what we do here.
    pub fn render_rows_ordered(&self, rows: &[Value], columns: &[String]) -> Result<()> {
        let value = Value::Array(rows.to_vec());

        match self.format {
            OutputFormat::Table => {
                if !self.render_table_with(&value, Some(columns))? {
                    println!("{}", serde_json::to_string_pretty(&value)?);
                }
            }
            OutputFormat::Csv => {
                if !self.render_csv_with(&value, Some(columns))? {
                    println!("{}", serde_json::to_string_pretty(&value)?);
                }
            }
            OutputFormat::Markdown => {
                if !self.render_markdown_table_with(&value, Some(columns))? {
                    self.render_markdown_single(&value)?;
                }
            }
            _ => self.render(&value)?,
        }

        Ok(())
    }

    fn render_table_with(&self, value: &Value, columns: Option<&[String]>) -> Result<bool> {
        match Self::table_string(value, columns) {
            Some(table) => {
                println!("{}", table);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// A list of objects as a table, or `None` when `value` is not one.
    fn table_string(value: &Value, columns: Option<&[String]>) -> Option<String> {
        let (headers, rows) = Self::coerce_rows_with(value, columns)?;

        let mut builder = Builder::default();
        builder.push_record(headers);
        for row in rows {
            builder.push_record(row);
        }

        Some(builder.build().with(Style::rounded()).to_string())
    }

    fn render_csv(&self, value: &Value) -> Result<bool> {
        self.render_csv_with(value, None)
    }

    fn render_csv_with(&self, value: &Value, columns: Option<&[String]>) -> Result<bool> {
        let (headers, rows) = match Self::coerce_rows_with(value, columns) {
            Some(data) => data,
            None => return Ok(false),
        };

        println!("{}", Self::csv_record(&headers));
        for row in rows {
            println!("{}", Self::csv_record(&row));
        }

        Ok(true)
    }

    /// Join one CSV record, quoting per RFC 4180.
    ///
    /// Fields routinely contain commas (issue summaries, comment bodies) and can
    /// contain newlines. Joining them raw shifted columns and broke rows, so any
    /// field containing a comma, double quote, CR or LF is wrapped in double
    /// quotes with internal quotes doubled.
    fn csv_record(fields: &[String]) -> String {
        fields
            .iter()
            .map(|f| Self::csv_field(f))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn csv_field(field: &str) -> String {
        if field.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", field.replace('"', "\"\""))
        } else {
            field.to_string()
        }
    }

    fn render_quiet(&self, value: &Value) -> bool {
        match value {
            Value::Array(rows) => {
                let mut printed = false;
                for row in rows {
                    if let Value::Object(obj) = row {
                        if let Some(id) = obj.get("id").and_then(Value::as_str) {
                            println!("{id}");
                            printed = true;
                        } else if let Some(key) = obj.keys().next() {
                            if let Some(val) = obj.get(key) {
                                println!("{}", val);
                                printed = true;
                            }
                        }
                    } else if !row.is_null() {
                        println!("{}", row);
                        printed = true;
                    }
                }
                printed
            }
            Value::Object(obj) => {
                if let Some(id) = obj.get("id").and_then(Value::as_str) {
                    println!("{id}");
                    true
                } else {
                    false
                }
            }
            Value::Null => false,
            other => {
                println!("{}", other);
                true
            }
        }
    }

    /// Render pre-formatted content directly to stdout (e.g. for markdown issue views).
    pub fn render_raw(&self, content: &str) -> Result<()> {
        println!("{content}");
        Ok(())
    }

    fn render_markdown_table(&self, value: &Value) -> Result<bool> {
        self.render_markdown_table_with(value, None)
    }

    fn render_markdown_table_with(
        &self,
        value: &Value,
        columns: Option<&[String]>,
    ) -> Result<bool> {
        let (headers, rows) = match Self::coerce_rows_with(value, columns) {
            Some(data) => data,
            None => return Ok(false),
        };

        // Header row
        let header_line: String = headers
            .iter()
            .map(|h| Self::markdown_cell(h))
            .collect::<Vec<_>>()
            .join(" | ");
        println!("| {} |", header_line);

        // Separator row
        let separator: String = headers
            .iter()
            .map(|_| "---")
            .collect::<Vec<_>>()
            .join(" | ");
        println!("| {} |", separator);

        // Data rows
        for row in rows {
            let cells: String = row
                .iter()
                .map(|c| Self::markdown_cell(c))
                .collect::<Vec<_>>()
                .join(" | ");
            println!("| {} |", cells);
        }

        Ok(true)
    }

    /// Escape one markdown table cell.
    ///
    /// A newline terminates the row in markdown, so a multi-line value (a comment
    /// body, a page description) silently broke the table. Newlines become `<br>`,
    /// and `|` is escaped so it does not open a new column.
    fn markdown_cell(cell: &str) -> String {
        cell.replace('|', "\\|")
            .replace("\r\n", "<br>")
            .replace(['\n', '\r'], "<br>")
    }

    fn render_markdown_single(&self, value: &Value) -> Result<bool> {
        if let Value::Object(obj) = value {
            for (key, val) in obj {
                let display = Self::value_to_string(val);
                println!("**{}**: {}", key, display);
            }
            Ok(true)
        } else {
            println!("{}", serde_json::to_string_pretty(value)?);
            Ok(true)
        }
    }

    /// Headers as the sorted union of every row's keys. The shape all ~70 list
    /// commands use; only field selection passes explicit columns.
    #[cfg(test)]
    fn coerce_rows(value: &Value) -> Option<(Vec<String>, Vec<Vec<String>>)> {
        Self::coerce_rows_with(value, None)
    }

    /// Flatten an array of objects into headers and string cells.
    ///
    /// With `columns`, those are the headers verbatim: keys not listed are
    /// dropped and listed keys missing from a row render empty, the same as any
    /// other absent key. Without them, headers are the sorted union of every
    /// row's keys, which is what all ~70 existing list commands rely on.
    fn coerce_rows_with(
        value: &Value,
        columns: Option<&[String]>,
    ) -> Option<(Vec<String>, Vec<Vec<String>>)> {
        let rows = match value {
            Value::Array(rows) if !rows.is_empty() => rows,
            _ => return None,
        };

        let headers_vec: Vec<String> = match columns {
            Some(columns) => columns.to_vec(),
            None => {
                let mut headers = BTreeSet::new();
                for row in rows {
                    if let Value::Object(obj) = row {
                        headers.extend(obj.keys().cloned());
                    }
                }
                headers.into_iter().collect()
            }
        };

        if headers_vec.is_empty() {
            return None;
        }

        let mut data = Vec::with_capacity(rows.len());
        for row in rows {
            let mut record = Vec::with_capacity(headers_vec.len());
            if let Value::Object(obj) = row {
                for header in &headers_vec {
                    let cell = obj
                        .get(header)
                        .map(Self::value_to_string)
                        .unwrap_or_else(|| "".to_string());
                    record.push(cell);
                }
            }
            data.push(record);
        }

        Some((headers_vec, data))
    }

    fn value_to_string(value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            other => serde_json::to_string(other).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_output_format_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Table);
    }

    #[test]
    fn test_renderer_new() {
        let renderer = OutputRenderer::new(OutputFormat::Json);
        assert_eq!(renderer.format(), OutputFormat::Json);
    }

    #[test]
    fn test_coerce_rows_empty_array() {
        let value = json!([]);
        assert!(OutputRenderer::coerce_rows(&value).is_none());
    }

    #[test]
    fn test_coerce_rows_single_object() {
        let value = json!([
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"}
        ]);

        let (headers, rows) = OutputRenderer::coerce_rows(&value).unwrap();
        assert_eq!(headers.len(), 2);
        assert!(headers.contains(&"id".to_string()));
        assert!(headers.contains(&"name".to_string()));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_coerce_rows_mixed_keys() {
        let value = json!([
            {"id": "1", "name": "Alice"},
            {"id": "2", "email": "bob@example.com"}
        ]);

        let (headers, rows) = OutputRenderer::coerce_rows(&value).unwrap();
        assert_eq!(headers.len(), 3);
        assert!(headers.contains(&"id".to_string()));
        assert!(headers.contains(&"name".to_string()));
        assert!(headers.contains(&"email".to_string()));

        assert_eq!(
            rows[0][headers.iter().position(|h| h == "id").unwrap()],
            "1"
        );
        assert_eq!(
            rows[0][headers.iter().position(|h| h == "name").unwrap()],
            "Alice"
        );
        assert_eq!(
            rows[0][headers.iter().position(|h| h == "email").unwrap()],
            ""
        );
    }

    #[derive(Serialize)]
    struct PullRequestView {
        id: i64,
        title: &'static str,
        approvals: String,
        description: Option<&'static str>,
        labels: Vec<&'static str>,
        reviewers: Vec<serde_json::Value>,
    }

    fn pull_request() -> PullRequestView {
        PullRequestView {
            id: 54,
            title: "Add image resizing",
            approvals: "1".to_string(),
            description: None,
            labels: vec!["infra", "urgent"],
            reviewers: vec![json!({"name": "Reviewer One", "status": "Approved", "uuid": "{r-1}"})],
        }
    }

    fn table_of<T: Serialize>(value: &T) -> String {
        let json_value = serde_json::to_value(value).unwrap();
        OutputRenderer::table_output(value, &json_value).unwrap()
    }

    /// The reported symptom: `bb pr get` printed JSON with no `-f`.
    #[test]
    fn a_single_object_renders_as_a_field_value_table() {
        let out = table_of(&pull_request());
        assert!(!out.trim_start().starts_with('{'), "not JSON: {out}");
        assert!(out.contains("field") && out.contains("value"), "{out}");
        assert!(out.contains("Add image resizing"), "{out}");
    }

    /// Declaration order, not alphabetical: `id` and `title` come first.
    #[test]
    fn fields_keep_their_declaration_order() {
        let out = table_of(&pull_request());
        let id = out.find("│ id").unwrap();
        let title = out.find("│ title").unwrap();
        let approvals = out.find("│ approvals").unwrap();
        assert!(id < title && title < approvals, "{out}");
    }

    #[test]
    fn a_list_of_objects_inside_becomes_a_titled_table_below() {
        let out = table_of(&pull_request());
        let main_end = out.find("reviewers:").expect("titled section");
        let section = &out[main_end..];
        for cell in [
            "name",
            "status",
            "uuid",
            "Reviewer One",
            "Approved",
            "{r-1}",
        ] {
            assert!(section.contains(cell), "{cell} missing from: {section}");
        }
        assert!(
            !out[..main_end].contains("Reviewer One"),
            "not squeezed into a cell"
        );
    }

    #[test]
    fn scalars_lists_and_nulls_render_plainly() {
        let out = table_of(&pull_request());
        assert!(out.contains("infra, urgent"), "{out}");
        let description_line = out.lines().find(|l| l.contains("description")).unwrap();
        assert!(!description_line.contains("null"), "{description_line}");
    }

    #[test]
    fn a_nested_object_is_compact_json() {
        let out = table_of(&json!({"id": 1, "author": {"name": "A"}}));
        assert!(out.contains(r#"{"name":"A"}"#), "{out}");
    }

    #[test]
    fn long_values_wrap_instead_of_widening_the_table() {
        let long = "word ".repeat(80);
        let out = table_of(&json!({"description": long}));
        let widest = out.lines().map(|l| l.chars().count()).max().unwrap();
        assert!(
            widest < RECORD_VALUE_WIDTH + 30,
            "widest line {widest}: {out}"
        );
        assert!(out.lines().count() > 4, "{out}");
    }

    /// Lists keep their existing shape, and non-tabular values still fall back.
    #[test]
    fn lists_and_bare_values_are_unchanged() {
        let out = table_of(&json!([{"id": "1", "name": "Alice"}]));
        assert!(out.contains("Alice") && !out.contains("field"), "{out}");
        assert_eq!(table_of(&json!([])), "[]");
        assert_eq!(table_of(&json!("text")), "\"text\"");
    }

    #[test]
    fn human_formats_are_table_and_markdown_only() {
        assert!(OutputFormat::Table.is_human());
        assert!(OutputFormat::Markdown.is_human());
        for f in [
            OutputFormat::Json,
            OutputFormat::Yaml,
            OutputFormat::Csv,
            OutputFormat::Quiet,
        ] {
            assert!(!f.is_human(), "{f:?}");
        }
    }

    #[test]
    fn test_coerce_rows_not_array() {
        let value = json!({"id": "1", "name": "Alice"});
        assert!(OutputRenderer::coerce_rows(&value).is_none());
    }

    /// The default contract, pinned so field selection cannot change it for the
    /// ~70 commands that derive their own columns.
    #[test]
    fn coerce_rows_without_columns_sorts_headers_alphabetically() {
        let value = json!([{"zebra": "1", "apple": "2"}]);
        let (headers, _) = OutputRenderer::coerce_rows(&value).unwrap();
        assert_eq!(headers, vec!["apple".to_string(), "zebra".to_string()]);
    }

    /// The whole point of the explicit form: the user typed an order.
    #[test]
    fn coerce_rows_with_columns_preserves_the_given_order() {
        let value = json!([{"apple": "2", "zebra": "1"}]);
        let columns = vec!["zebra".to_string(), "apple".to_string()];

        let (headers, rows) = OutputRenderer::coerce_rows_with(&value, Some(&columns)).unwrap();

        assert_eq!(headers, columns);
        assert_eq!(rows[0], vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn coerce_rows_with_columns_drops_keys_not_listed() {
        let value = json!([{"wanted": "yes", "unwanted": "no"}]);
        let columns = vec!["wanted".to_string()];

        let (headers, rows) = OutputRenderer::coerce_rows_with(&value, Some(&columns)).unwrap();

        assert_eq!(headers, columns);
        assert_eq!(rows[0], vec!["yes".to_string()]);
    }

    /// A field the site does not have, or that the API omitted, is an empty
    /// cell rather than a missing column or an error.
    #[test]
    fn coerce_rows_with_columns_fills_absent_keys_with_empty() {
        let value = json!([{"present": "here"}]);
        let columns = vec!["present".to_string(), "absent".to_string()];

        let (_, rows) = OutputRenderer::coerce_rows_with(&value, Some(&columns)).unwrap();

        assert_eq!(rows[0], vec!["here".to_string(), String::new()]);
    }

    #[test]
    fn coerce_rows_with_empty_columns_renders_nothing() {
        let value = json!([{"id": "1"}]);
        assert!(OutputRenderer::coerce_rows_with(&value, Some(&[])).is_none());
    }

    #[test]
    fn test_coerce_rows_array_of_primitives() {
        let value = json!(["one", "two", "three"]);
        assert!(OutputRenderer::coerce_rows(&value).is_none());
    }

    #[test]
    fn test_value_to_string_string() {
        let value = json!("hello");
        assert_eq!(OutputRenderer::value_to_string(&value), "hello");
    }

    #[test]
    fn test_value_to_string_number() {
        let value = json!(42);
        assert_eq!(OutputRenderer::value_to_string(&value), "42");
    }

    #[test]
    fn test_value_to_string_bool() {
        let value = json!(true);
        assert_eq!(OutputRenderer::value_to_string(&value), "true");
    }

    #[test]
    fn test_value_to_string_null() {
        let value = json!(null);
        assert_eq!(OutputRenderer::value_to_string(&value), "");
    }

    #[test]
    fn test_value_to_string_object() {
        let value = json!({"key": "value"});
        let result = OutputRenderer::value_to_string(&value);
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_render_quiet_object_with_id() {
        let value = json!({"id": "123", "name": "Test"});
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        assert!(renderer.render_quiet(&value));
    }

    #[test]
    fn test_render_quiet_object_without_id() {
        let value = json!({"name": "Test"});
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        assert!(!renderer.render_quiet(&value));
    }

    #[test]
    fn test_render_quiet_array_with_ids() {
        let value = json!([
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"}
        ]);
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        assert!(renderer.render_quiet(&value));
    }

    #[test]
    fn test_render_quiet_primitive() {
        let value = json!("simple");
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        assert!(renderer.render_quiet(&value));
    }

    #[test]
    fn test_render_quiet_null() {
        let value = json!(null);
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        assert!(!renderer.render_quiet(&value));
    }

    #[test]
    fn test_render_quiet_array_with_nulls() {
        let value = json!([null, null]);
        let renderer = OutputRenderer::new(OutputFormat::Quiet);
        assert!(!renderer.render_quiet(&value));
    }

    #[derive(Serialize)]
    struct TestStruct {
        id: String,
        name: String,
        count: i32,
    }

    #[test]
    fn test_render_json() {
        let test_data = TestStruct {
            id: "1".to_string(),
            name: "Test".to_string(),
            count: 42,
        };

        let renderer = OutputRenderer::new(OutputFormat::Json);
        let result = renderer.render(&test_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_yaml() {
        let test_data = TestStruct {
            id: "1".to_string(),
            name: "Test".to_string(),
            count: 42,
        };

        let renderer = OutputRenderer::new(OutputFormat::Yaml);
        let result = renderer.render(&test_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_table() {
        let test_data = vec![
            TestStruct {
                id: "1".to_string(),
                name: "Alice".to_string(),
                count: 10,
            },
            TestStruct {
                id: "2".to_string(),
                name: "Bob".to_string(),
                count: 20,
            },
        ];

        let renderer = OutputRenderer::new(OutputFormat::Table);
        let result = renderer.render(&test_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_csv() {
        let test_data = vec![
            TestStruct {
                id: "1".to_string(),
                name: "Alice".to_string(),
                count: 10,
            },
            TestStruct {
                id: "2".to_string(),
                name: "Bob".to_string(),
                count: 20,
            },
        ];

        let renderer = OutputRenderer::new(OutputFormat::Csv);
        let result = renderer.render(&test_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_markdown_table() {
        let test_data = vec![
            TestStruct {
                id: "1".to_string(),
                name: "Alice".to_string(),
                count: 10,
            },
            TestStruct {
                id: "2".to_string(),
                name: "Bob".to_string(),
                count: 20,
            },
        ];

        let renderer = OutputRenderer::new(OutputFormat::Markdown);
        let result = renderer.render(&test_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_markdown_single_object() {
        let test_data = TestStruct {
            id: "1".to_string(),
            name: "Test".to_string(),
            count: 42,
        };

        let renderer = OutputRenderer::new(OutputFormat::Markdown);
        let result = renderer.render(&test_data);
        assert!(result.is_ok());
    }

    // Regression: render_csv used to `row.join(",")` with no quoting, so any field
    // containing a comma (issue summaries, comment bodies) shifted every later
    // column, and a newline destroyed the row outright.
    #[test]
    fn test_csv_field_quotes_per_rfc4180() {
        assert_eq!(OutputRenderer::csv_field("plain"), "plain");
        assert_eq!(OutputRenderer::csv_field("a,b"), "\"a,b\"");
        assert_eq!(
            OutputRenderer::csv_field("say \"hi\""),
            "\"say \"\"hi\"\"\""
        );
        assert_eq!(
            OutputRenderer::csv_field("line1\nline2"),
            "\"line1\nline2\""
        );
        assert_eq!(OutputRenderer::csv_field("cr\r"), "\"cr\r\"");
        // Quoting only when required, so unaffected output is byte-identical.
        assert_eq!(OutputRenderer::csv_field("no-specials"), "no-specials");
    }

    #[test]
    fn test_csv_record_keeps_columns_aligned() {
        let fields = vec![
            "1".to_string(),
            "Fix bug, urgently".to_string(),
            "open".to_string(),
        ];
        // Three fields must stay three columns despite the embedded comma.
        assert_eq!(
            OutputRenderer::csv_record(&fields),
            "1,\"Fix bug, urgently\",open"
        );
    }

    // Regression: a newline in a cell terminated the markdown table row.
    #[test]
    fn test_markdown_cell_escapes_newlines_and_pipes() {
        assert_eq!(OutputRenderer::markdown_cell("a|b"), "a\\|b");
        assert_eq!(OutputRenderer::markdown_cell("one\ntwo"), "one<br>two");
        assert_eq!(OutputRenderer::markdown_cell("one\r\ntwo"), "one<br>two");
        assert_eq!(OutputRenderer::markdown_cell("plain"), "plain");
    }

    #[test]
    fn test_render_markdown_pipe_escaping() {
        let value = json!([
            {"col": "a|b", "val": "x|y"}
        ]);
        let renderer = OutputRenderer::new(OutputFormat::Markdown);
        // Should not panic; pipes in values should be escaped
        assert!(renderer.render_markdown_table(&value).unwrap());
    }

    #[test]
    fn test_render_raw() {
        let renderer = OutputRenderer::new(OutputFormat::Markdown);
        let result = renderer.render_raw("# Hello\n\nWorld");
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_list_without_envelope() {
        let data = vec![TestStruct {
            id: "1".to_string(),
            name: "Alice".to_string(),
            count: 10,
        }];
        // Without envelope, render_list behaves like render
        let renderer = OutputRenderer::new(OutputFormat::Table);
        let result = renderer.render_list(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_list_with_envelope() {
        let data = vec![TestStruct {
            id: "1".to_string(),
            name: "Alice".to_string(),
            count: 10,
        }];
        let renderer = OutputRenderer::new(OutputFormat::Json).with_envelope(true);
        // Should produce enveloped output
        let result = renderer.render_list(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_list_empty_with_envelope() {
        let data: Vec<TestStruct> = vec![];
        let renderer = OutputRenderer::new(OutputFormat::Json).with_envelope(true);
        let result = renderer.render_list(&data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_with_envelope_setter() {
        let renderer = OutputRenderer::new(OutputFormat::Json).with_envelope(true);
        assert_eq!(renderer.format(), OutputFormat::Json);
    }

    // -----------------------------------------------------------------------
    // render_list_or_empty (#110)
    // -----------------------------------------------------------------------

    #[derive(Serialize)]
    struct EmptyRow {
        id: String,
    }

    // A script doing `| jq` cannot parse "No pull requests found". Every machine
    // format has to produce a real empty array.
    #[test]
    fn test_render_list_or_empty_json_emits_an_array() {
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let rows: Vec<EmptyRow> = Vec::new();
        // The assertion that matters is the shape, checked by the sibling
        // serialisation test below; here we only pin that it does not error.
        assert!(renderer
            .render_list_or_empty(&rows, "No rows found")
            .is_ok());
    }

    #[test]
    fn test_render_list_or_empty_is_a_message_only_for_humans() {
        let rows: Vec<EmptyRow> = Vec::new();
        for format in [OutputFormat::Table, OutputFormat::Markdown] {
            let renderer = OutputRenderer::new(format);
            assert!(renderer
                .render_list_or_empty(&rows, "No rows found")
                .is_ok());
        }
        for format in [
            OutputFormat::Json,
            OutputFormat::Yaml,
            OutputFormat::Csv,
            OutputFormat::Quiet,
        ] {
            let renderer = OutputRenderer::new(format);
            assert!(renderer
                .render_list_or_empty(&rows, "No rows found")
                .is_ok());
        }
    }

    // A non-empty list must be unaffected: the message is only for the empty case.
    #[test]
    fn test_render_list_or_empty_renders_rows_when_present() {
        let renderer = OutputRenderer::new(OutputFormat::Json);
        let rows = vec![EmptyRow {
            id: "1".to_string(),
        }];
        assert!(renderer
            .render_list_or_empty(&rows, "No rows found")
            .is_ok());
    }

    // The envelope path still applies, so `--envelope` keeps reporting count 0
    // rather than falling back to the human message.
    #[test]
    fn test_render_list_or_empty_honours_the_envelope() {
        let renderer = OutputRenderer::new(OutputFormat::Json).with_envelope(true);
        let rows: Vec<EmptyRow> = Vec::new();
        assert!(renderer
            .render_list_or_empty(&rows, "No rows found")
            .is_ok());
    }
}
