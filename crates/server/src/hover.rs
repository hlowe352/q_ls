use tower_lsp_server::ls_types::*;
use crate::builtins::lookup_doc;
use crate::document::Document;

pub fn hover(doc: &Document, pos: Position) -> Option<Hover> {
    let offset = doc.offset_of(pos);

    // Try the word at cursor (handles bare idents and dotted idents).
    let word = get_word_at(doc.text(), offset)?;

    if let Some(detail) = lookup_doc(&word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{word}** - {detail}"),
            }),
            range: None,
        });
    }

    if let Some(doc_str) = operator_doc(&word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc_str.to_string(),
            }),
            range: None,
        });
    }

    // User-defined name: resolve via sym_table.
    // Strip a leading backtick so hovering on `` `.cache.cache `` works.
    let name = word.strip_prefix('`').unwrap_or(word.as_str());
    if !name.is_empty() {
        let table = doc.sym_table();
        if table.resolve(offset, name).is_some() {
            // Don't qualify names that are column definitions inside a table
            // constructor — they look like globals but are column names.
            let in_table = cursor_in_table_expr(doc, offset);
            let display = if in_table {
                name.to_string()
            } else {
                table
                    .qualified_for(offset, name)
                    .map(|q| q.to_string())
                    .unwrap_or_else(|| name.to_string())
            };
            let value = if display != name {
                format!("`{name}` → **`{display}`**")
            } else {
                format!("**`{display}`**")
            };
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: None,
            });
        }
    }

    None
}

fn cursor_in_table_expr(doc: &Document, offset: usize) -> bool {
    use q_parser::SyntaxKind;
    let root = doc.parse().syntax();
    let pos = q_parser::TextSize::from(offset as u32);
    let token = root.token_at_offset(pos).left_biased();
    token
        .and_then(|t| t.parent())
        .map(|n| n.ancestors().any(|a| a.kind() == SyntaxKind::TableExpr))
        .unwrap_or(false)
}

fn get_word_at(text: &str, offset: usize) -> Option<String> {
    if offset >= text.len() { return None; }
    let bytes = text.as_bytes();

    // Include a leading backtick so `` `.cache.cache `` is returned as
    // `` `.cache.cache `` (stripping happens later in callers that need the name).
    let mut start = offset;
    let mut end = offset;
    while start > 0 && is_word_char(bytes[start - 1]) { start -= 1; }
    // Back up one more if the char before start is a backtick.
    if start > 0 && bytes[start - 1] == b'`' { start -= 1; }
    while end < bytes.len() && is_word_char(bytes[end]) { end += 1; }
    if start == end {
        // Maybe a single-char operator
        end = (offset + 1).min(bytes.len());
        start = offset;
    }
    Some(text[start..end].to_string())
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

fn operator_doc(op: &str) -> Option<&'static str> {
    match op {
        "+" => Some("**`+`** - Add (dyadic) / Flip (monadic)"),
        "-" => Some("**`-`** - Subtract (dyadic) / Negate (monadic)"),
        "*" => Some("**`*`** - Multiply (dyadic) / First (monadic)"),
        "%" => Some("**`%`** - Divide (dyadic) / Reciprocal (monadic)"),
        "!" => Some("**`!`** - Key/mod (dyadic) / Til/enum (monadic)"),
        "&" => Some("**`&`** - And/min (dyadic) / Where (monadic)"),
        "|" => Some("**`|`** - Or/max (dyadic) / Reverse (monadic)"),
        "^" => Some("**`^`** - Fill (dyadic) / Null (monadic)"),
        "#" => Some("**`#`** - Take (dyadic) / Count (monadic)"),
        "_" => Some("**`_`** - Drop/cut (dyadic) / Floor (monadic)"),
        "~" => Some("**`~`** - Match (dyadic) / Not (monadic)"),
        "$" => Some("**`$`** - Cast/pad (dyadic) / String (monadic)"),
        "?" => Some("**`?`** - Find/rand (dyadic) / Distinct/type (monadic)"),
        "@" => Some("**`@`** - Apply/index (dyadic) / Type (monadic)"),
        "." => Some("**`.`** - Apply deep (dyadic) / Value (monadic)"),
        "," => Some("**`,`** - Join (dyadic) / Enlist (monadic)"),
        "=" => Some("**`=`** - Equal (dyadic) / Group (monadic)"),
        "<" => Some("**`<`** - Less than (dyadic) / Iasc (monadic)"),
        ">" => Some("**`>`** - Greater than (dyadic) / Idesc (monadic)"),
        ":" => Some("**`:`** - Assign (dyadic) / Identity (monadic)"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn hover_text(src: &str, cursor: usize) -> Option<String> {
        let doc = Document::new(src.to_string(), 0);
        let pos = doc.position_of(cursor);
        hover(&doc, pos).map(|h| match h.contents {
            HoverContents::Markup(m) => m.value,
            _ => String::new(),
        })
    }

    #[test]
    fn bare_name_in_d_block_shows_qualified() {
        let src = "\\d .cache\ncache:1\n\\d .";
        // hover on `cache` (the reference side — but here it's the def)
        let cursor = src.find("cache:1").unwrap();
        // `cache` is in `\d .cache` context so qualified_for returns .cache.cache
        // Hover is on the LHS of assignment — but hover resolves it fine
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(text.contains(".cache.cache"), "got: {text}");
    }

    #[test]
    fn backtick_symbol_resolves_to_table() {
        let src = "\\d .cache\ncache:1\n\\d .\n`.cache.cache upsert 2";
        let cursor = src.find("`.cache.cache").unwrap() + 1; // on `.cache.cache`
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(text.contains(".cache.cache"), "got: {text}");
    }

    #[test]
    fn table_col_def_not_qualified() {
        // Column names inside table constructor are not globals.
        let src = "\\d .cache\ncache:([id:`u#`long$()] size:`long$())\n\\d .";
        let cursor = src.find("id:`u").unwrap(); // hover on `id` col def
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(!text.contains(".cache.id"), "table col falsely qualified; got: {text}");
    }

    #[test]
    fn param_shadowing_global_not_qualified() {
        // `id` is both a global (.cache.id) and a lambda param.
        // Hover over the param should NOT show .cache.id.
        let src = "\\d .cache\nid:0j\nadd:{[id] id+1}\n\\d .";
        let cursor = src.find("id+1").unwrap(); // ref to the param inside lambda
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(!text.contains(".cache.id"), "param falsely qualified; got: {text}");
    }

    #[test]
    fn builtin_still_works() {
        let src = "count x";
        let cursor = src.find("count").unwrap();
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(text.to_lowercase().contains("count"), "got: {text}");
    }
}
