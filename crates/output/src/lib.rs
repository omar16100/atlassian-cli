use std::collections::BTreeSet;

use anyhow::Result;
use clap::ValueEnum;
use serde::Serialize;
use serde_json::Value;
use tabled::builder::Builder;
use tabled::settings::Style;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Table,
    Json,
    Yaml,
    Csv,
    Quiet,
}

pub struct OutputRenderer {
    format: OutputFormat,
}

impl OutputRenderer {
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    pub fn format(&self) -> OutputFormat {
        self.format
    }

    pub fn render<T: Serialize>(&self, value: &T) -> Result<()> {
        let json_value = serde_json::to_value(value)?;

        match self.format {
            OutputFormat::Table => {
                if !self.render_table(&json_value)? {
                    println!("{}", serde_json::to_string_pretty(&json_value)?);
                }
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
        }

        Ok(())
    }

    fn render_table(&self, value: &Value) -> Result<bool> {
        let (headers, rows) = match Self::coerce_rows(value) {
            Some(data) => data,
            None => return Ok(false),
        };

        let mut builder = Builder::default();
        builder.push_record(headers);
        for row in rows {
            builder.push_record(row);
        }

        let table = builder.build().with(Style::rounded()).to_string();
        println!("{}", table);
        Ok(true)
    }

    fn render_csv(&self, value: &Value) -> Result<bool> {
        let (headers, rows) = match Self::coerce_rows(value) {
            Some(data) => data,
            None => return Ok(false),
        };

        println!("{}", headers.join(","));
        for row in rows {
            println!("{}", row.join(","));
        }

        Ok(true)
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

    fn coerce_rows(value: &Value) -> Option<(Vec<String>, Vec<Vec<String>>)> {
        let rows = match value {
            Value::Array(rows) if !rows.is_empty() => rows,
            _ => return None,
        };

        let mut headers = BTreeSet::new();
        for row in rows {
            if let Value::Object(obj) = row {
                headers.extend(obj.keys().cloned());
            }
        }

        if headers.is_empty() {
            return None;
        }

        let headers_vec: Vec<String> = headers.into_iter().collect();
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

    #[test]
    fn test_coerce_rows_not_array() {
        let value = json!({"id": "1", "name": "Alice"});
        assert!(OutputRenderer::coerce_rows(&value).is_none());
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
}
