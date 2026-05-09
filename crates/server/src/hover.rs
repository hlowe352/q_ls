use tower_lsp::lsp_types::*;
use crate::builtins::lookup_doc;
use crate::document::Document;

pub fn hover(doc: &Document, pos: Position) -> Option<Hover> {
    let offset = doc.offset_of(pos);
    let word = get_word_at(doc.text(), offset)?;

    if let Some(detail) = lookup_doc(&word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}** - {}", word, detail),
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

    None
}

fn get_word_at(text: &str, offset: usize) -> Option<String> {
    if offset >= text.len() { return None; }
    let bytes = text.as_bytes();
    let mut start = offset;
    let mut end = offset;
    while start > 0 && is_word_char(bytes[start - 1]) { start -= 1; }
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
