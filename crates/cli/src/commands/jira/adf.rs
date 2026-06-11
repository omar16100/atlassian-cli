//! Markdown -> Atlassian Document Format (ADF) conversion.
//!
//! Jira Cloud stores rich text as ADF. The CLI previously wrapped `--description`
//! and comment text in a single ADF paragraph, flattening all structure. This
//! module parses CommonMark markdown and emits structured ADF so headings, lists,
//! bold/italic/inline code, links and code blocks survive into Jira. See issue #39.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde_json::{json, Value};

/// A node under construction. `content` holds child nodes (block or inline).
struct NodeBuilder {
    node_type: &'static str,
    attrs: Option<Value>,
    content: Vec<Value>,
    /// True when this paragraph was opened implicitly to hold inline content that
    /// appeared directly inside a block container (e.g. a tight list item, where
    /// pulldown-cmark emits text without a wrapping paragraph).
    auto: bool,
}

impl NodeBuilder {
    fn new(node_type: &'static str) -> Self {
        Self {
            node_type,
            attrs: None,
            content: Vec::new(),
            auto: false,
        }
    }

    fn into_value(self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("type".into(), Value::String(self.node_type.into()));
        if let Some(attrs) = self.attrs {
            obj.insert("attrs".into(), attrs);
        }
        if !self.content.is_empty() {
            obj.insert("content".into(), Value::Array(self.content));
        }
        Value::Object(obj)
    }
}

/// Convert markdown `text` into an ADF `doc` value (version 1).
///
/// Plain text with no markdown syntax becomes a single paragraph, matching the
/// previous behaviour. Structural markdown (headings, lists, emphasis, code,
/// links) is mapped to the corresponding ADF nodes.
pub fn markdown_to_adf(text: &str) -> Value {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = Parser::new_ext(text, options);

    let mut stack: Vec<NodeBuilder> = vec![NodeBuilder::new("doc")];
    let mut marks: Vec<Value> = Vec::new();

    for event in parser {
        match event {
            Event::Start(tag) => handle_start(tag, &mut stack, &mut marks),
            Event::End(tag) => handle_end(tag, &mut stack, &mut marks),
            Event::Text(t) => push_text(&mut stack, &t, &marks),
            Event::Code(t) => {
                // inline code: a text node carrying a `code` mark. ADF only allows
                // `code` to combine with `link`, so drop any other active marks.
                let mut m: Vec<Value> = marks
                    .iter()
                    .filter(|mk| mk["type"] == "link")
                    .cloned()
                    .collect();
                m.push(json!({ "type": "code" }));
                push_text(&mut stack, &t, &m);
            }
            Event::SoftBreak | Event::HardBreak => push_break(&mut stack),
            Event::Html(h) | Event::InlineHtml(h) => push_text(&mut stack, &h, &marks),
            Event::Rule => {
                close_auto_paragraph(&mut stack);
                // `rule` is not allowed inside list items or blockquotes.
                if !in_restricted_parent(&stack) {
                    append_child(&mut stack, NodeBuilder::new("rule").into_value());
                }
            }
            _ => {}
        }
    }

    close_auto_paragraph(&mut stack);
    let mut doc = stack.pop().expect("doc node always present");
    // ADF requires a non-empty doc; emit an empty paragraph for empty input.
    if doc.content.is_empty() {
        doc.content.push(NodeBuilder::new("paragraph").into_value());
    }
    json!({
        "type": "doc",
        "version": 1,
        "content": doc.content,
    })
}

/// Block tags push a container node; inline emphasis/link tags push a mark that
/// applies to subsequent text until the matching end tag.
fn handle_start(tag: Tag, stack: &mut Vec<NodeBuilder>, marks: &mut Vec<Value>) {
    match tag {
        Tag::Paragraph => {
            close_auto_paragraph(stack);
            stack.push(NodeBuilder::new("paragraph"));
        }
        Tag::Heading { level, .. } => {
            close_auto_paragraph(stack);
            // ADF disallows headings inside list items and blockquotes; downgrade
            // to a plain paragraph there.
            if in_restricted_parent(stack) {
                stack.push(NodeBuilder::new("paragraph"));
            } else {
                let mut n = NodeBuilder::new("heading");
                n.attrs = Some(json!({ "level": heading_level(level) }));
                stack.push(n);
            }
        }
        Tag::BlockQuote(_) => {
            close_auto_paragraph(stack);
            // ADF disallows nested blockquotes and blockquotes inside list items;
            // make the wrapper transparent so its blocks attach to the parent.
            if in_restricted_parent(stack) {
                stack.push(NodeBuilder::new("__transparent__"));
            } else {
                stack.push(NodeBuilder::new("blockquote"));
            }
        }
        Tag::CodeBlock(kind) => {
            close_auto_paragraph(stack);
            let mut n = NodeBuilder::new("codeBlock");
            if let CodeBlockKind::Fenced(lang) = kind {
                if !lang.is_empty() {
                    n.attrs = Some(json!({ "language": lang.to_string() }));
                }
            }
            stack.push(n);
        }
        Tag::List(start) => {
            close_auto_paragraph(stack);
            match start {
                Some(n) => {
                    let mut node = NodeBuilder::new("orderedList");
                    if n != 1 {
                        node.attrs = Some(json!({ "order": n }));
                    }
                    stack.push(node);
                }
                None => stack.push(NodeBuilder::new("bulletList")),
            }
        }
        Tag::Item => {
            close_auto_paragraph(stack);
            stack.push(NodeBuilder::new("listItem"));
        }
        Tag::Emphasis => marks.push(json!({ "type": "em" })),
        Tag::Strong => marks.push(json!({ "type": "strong" })),
        Tag::Strikethrough => marks.push(json!({ "type": "strike" })),
        Tag::Link { dest_url, .. } => {
            // ADF requires href to be a URI. Skip the mark for empty destinations
            // (keeping the label as plain text) but push a null placeholder so the
            // matching End(Link) still balances the mark stack.
            if dest_url.is_empty() {
                marks.push(Value::Null);
            } else {
                marks.push(json!({ "type": "link", "attrs": { "href": dest_url.to_string() } }));
            }
        }
        // Images: keep the alt text as plain text, drop the wrapper.
        _ => {}
    }
}

/// True when the current parent container forbids heading/blockquote/rule
/// children. ADF restricts `listItem` and `blockquote` content.
fn in_restricted_parent(stack: &[NodeBuilder]) -> bool {
    matches!(
        stack.last().map(|n| n.node_type),
        Some("listItem") | Some("blockquote")
    )
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn handle_end(tag: TagEnd, stack: &mut Vec<NodeBuilder>, marks: &mut Vec<Value>) {
    match tag {
        TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::CodeBlock => pop_and_append(stack),
        TagEnd::Item | TagEnd::BlockQuote(_) => {
            close_auto_paragraph(stack);
            // listItem / blockquote require non-empty content.
            ensure_nonempty_block_container(stack);
            pop_and_append(stack);
        }
        TagEnd::List(_) => {
            close_auto_paragraph(stack);
            pop_and_append(stack);
        }
        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
            marks.pop();
        }
        _ => {}
    }
}

fn top_accepts_inline(stack: &[NodeBuilder]) -> bool {
    matches!(
        stack.last().map(|n| n.node_type),
        Some("paragraph") | Some("heading") | Some("codeBlock")
    )
}

/// Auto-open a paragraph when inline content appears inside a block container.
fn ensure_inline_container(stack: &mut Vec<NodeBuilder>) {
    if !top_accepts_inline(stack) {
        let mut p = NodeBuilder::new("paragraph");
        p.auto = true;
        stack.push(p);
    }
}

fn close_auto_paragraph(stack: &mut Vec<NodeBuilder>) {
    let is_auto = stack
        .last()
        .map(|n| n.node_type == "paragraph" && n.auto)
        .unwrap_or(false);
    if is_auto {
        pop_and_append(stack);
    }
}

fn append_child(stack: &mut [NodeBuilder], node: Value) {
    if let Some(top) = stack.last_mut() {
        top.content.push(node);
    }
}

fn pop_and_append(stack: &mut Vec<NodeBuilder>) {
    // Never pop the root doc node.
    if stack.len() <= 1 {
        return;
    }
    let node = stack.pop().unwrap();
    if node.node_type == "__transparent__" {
        // Downgraded blockquote: splice its blocks directly into the parent.
        if let Some(parent) = stack.last_mut() {
            parent.content.extend(node.content);
        }
        return;
    }
    append_child(stack, node.into_value());
}

/// Ensure a `listItem`/`blockquote` has at least one block child (ADF requires it).
fn ensure_nonempty_block_container(stack: &mut [NodeBuilder]) {
    if let Some(top) = stack.last_mut() {
        if top.content.is_empty() {
            top.content.push(NodeBuilder::new("paragraph").into_value());
        }
    }
}

fn push_text(stack: &mut Vec<NodeBuilder>, text: &str, marks: &[Value]) {
    if text.is_empty() {
        return;
    }
    ensure_inline_container(stack);
    let top = stack.last_mut().expect("inline container present");

    if top.node_type == "codeBlock" {
        // Code blocks hold raw text with literal newlines; merge consecutive runs.
        if let Some(last) = top.content.last_mut() {
            if let Some(existing) = last.get("text").and_then(Value::as_str) {
                let merged = format!("{existing}{text}");
                last["text"] = Value::String(merged);
                return;
            }
        }
        top.content.push(json!({ "type": "text", "text": text }));
        return;
    }

    let mut node = json!({ "type": "text", "text": text });
    // Null entries are placeholders for skipped (empty-href) links.
    let active: Vec<Value> = marks.iter().filter(|m| !m.is_null()).cloned().collect();
    if !active.is_empty() {
        node["marks"] = Value::Array(active);
    }
    top.content.push(node);
}

fn push_break(stack: &mut Vec<NodeBuilder>) {
    // Inside code blocks newlines arrive as Text events, so a break is a no-op.
    if stack.last().map(|n| n.node_type) == Some("codeBlock") {
        return;
    }
    ensure_inline_container(stack);
    append_child(stack, json!({ "type": "hardBreak" }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(doc: &Value) -> &Vec<Value> {
        doc["content"].as_array().unwrap()
    }

    #[test]
    fn plain_text_is_single_paragraph() {
        let doc = markdown_to_adf("just plain text");
        assert_eq!(doc["type"], "doc");
        assert_eq!(doc["version"], 1);
        let c = content(&doc);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["type"], "paragraph");
        assert_eq!(c[0]["content"][0]["text"], "just plain text");
    }

    #[test]
    fn empty_input_yields_empty_paragraph() {
        let doc = markdown_to_adf("");
        let c = content(&doc);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0]["type"], "paragraph");
    }

    #[test]
    fn heading_levels() {
        let doc = markdown_to_adf("# Title\n\n### Sub");
        let c = content(&doc);
        assert_eq!(c[0]["type"], "heading");
        assert_eq!(c[0]["attrs"]["level"], 1);
        assert_eq!(c[0]["content"][0]["text"], "Title");
        assert_eq!(c[1]["attrs"]["level"], 3);
    }

    #[test]
    fn bold_and_italic_marks() {
        let doc = markdown_to_adf("**bold** and *italic*");
        let para = &content(&doc)[0];
        let inline = para["content"].as_array().unwrap();
        assert_eq!(inline[0]["text"], "bold");
        assert_eq!(inline[0]["marks"][0]["type"], "strong");
        // " and " then italic
        let italic = inline.iter().find(|n| n["text"] == "italic").unwrap();
        assert_eq!(italic["marks"][0]["type"], "em");
    }

    #[test]
    fn inline_code_mark() {
        let doc = markdown_to_adf("call `foo()` now");
        let inline = content(&doc)[0]["content"].as_array().unwrap();
        let code = inline.iter().find(|n| n["text"] == "foo()").unwrap();
        assert_eq!(code["marks"][0]["type"], "code");
    }

    #[test]
    fn link_mark_with_href() {
        let doc = markdown_to_adf("[site](https://example.com)");
        let inline = content(&doc)[0]["content"].as_array().unwrap();
        assert_eq!(inline[0]["text"], "site");
        assert_eq!(inline[0]["marks"][0]["type"], "link");
        assert_eq!(
            inline[0]["marks"][0]["attrs"]["href"],
            "https://example.com"
        );
    }

    #[test]
    fn bullet_list_items_wrapped_in_paragraph() {
        let doc = markdown_to_adf("- one\n- two");
        let list = &content(&doc)[0];
        assert_eq!(list["type"], "bulletList");
        let items = list["content"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["type"], "listItem");
        // listItem content must be a block node (paragraph), not raw text.
        assert_eq!(items[0]["content"][0]["type"], "paragraph");
        assert_eq!(items[0]["content"][0]["content"][0]["text"], "one");
    }

    #[test]
    fn ordered_list_with_nondefault_start() {
        let doc = markdown_to_adf("3. third\n4. fourth");
        let list = &content(&doc)[0];
        assert_eq!(list["type"], "orderedList");
        assert_eq!(list["attrs"]["order"], 3);
    }

    #[test]
    fn ordered_list_default_start_has_no_order_attr() {
        let doc = markdown_to_adf("1. a\n2. b");
        let list = &content(&doc)[0];
        assert_eq!(list["type"], "orderedList");
        assert!(list.get("attrs").is_none());
    }

    #[test]
    fn fenced_code_block_keeps_language_and_newlines() {
        let doc = markdown_to_adf("```rust\nlet x = 1;\nlet y = 2;\n```");
        let cb = &content(&doc)[0];
        assert_eq!(cb["type"], "codeBlock");
        assert_eq!(cb["attrs"]["language"], "rust");
        assert_eq!(cb["content"][0]["text"], "let x = 1;\nlet y = 2;\n");
        // code block text carries no marks
        assert!(cb["content"][0].get("marks").is_none());
    }

    #[test]
    fn inline_code_inside_emphasis_drops_em_keeps_code() {
        // ADF only allows `code` to combine with `link`; em must be dropped.
        let doc = markdown_to_adf("*em `x` more*");
        let inline = content(&doc)[0]["content"].as_array().unwrap();
        let code = inline.iter().find(|n| n["text"] == "x").unwrap();
        let marks = code["marks"].as_array().unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0]["type"], "code");
    }

    #[test]
    fn heading_in_list_item_downgrades_to_paragraph() {
        let doc = markdown_to_adf("- # Title");
        let item = &content(&doc)[0]["content"][0];
        assert_eq!(item["type"], "listItem");
        assert_eq!(item["content"][0]["type"], "paragraph");
        assert_eq!(item["content"][0]["content"][0]["text"], "Title");
    }

    #[test]
    fn heading_in_blockquote_downgrades_to_paragraph() {
        let doc = markdown_to_adf("> # Quote");
        let bq = &content(&doc)[0];
        assert_eq!(bq["type"], "blockquote");
        assert_eq!(bq["content"][0]["type"], "paragraph");
    }

    #[test]
    fn blockquote_in_list_item_is_flattened() {
        let doc = markdown_to_adf("- > quoted");
        let item = &content(&doc)[0]["content"][0];
        assert_eq!(item["type"], "listItem");
        // No nested blockquote; the quote's paragraph attaches to the listItem.
        assert_eq!(item["content"][0]["type"], "paragraph");
        assert_eq!(item["content"][0]["content"][0]["text"], "quoted");
    }

    #[test]
    fn empty_link_destination_keeps_plain_text() {
        let doc = markdown_to_adf("[label]()");
        let text = &content(&doc)[0]["content"][0];
        assert_eq!(text["text"], "label");
        assert!(text.get("marks").is_none());
    }

    #[test]
    fn roundtrip_issue_example() {
        let input = "Summary\nThis is a description of the work.\n\nWhat was added\n- Cloud NAT with a reserved static IP\n- A small VM for SFTP access";
        let doc = markdown_to_adf(input);
        let c = content(&doc);
        // First paragraph contains both lines joined by a hardBreak.
        assert_eq!(c[0]["type"], "paragraph");
        let first_inline = c[0]["content"].as_array().unwrap();
        assert!(first_inline.iter().any(|n| n["type"] == "hardBreak"));
        // Last node is the bullet list with two items.
        let list = c.last().unwrap();
        assert_eq!(list["type"], "bulletList");
        assert_eq!(list["content"].as_array().unwrap().len(), 2);
    }
}
