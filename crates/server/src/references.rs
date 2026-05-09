//! `textDocument/references` — find all references to the symbol under
//! the cursor by scanning every ident in the document and asking the
//! cached symbol table where each one resolves.

use tower_lsp_server::ls_types::*;
use q_parser::SyntaxKind;

use crate::document::Document;

pub fn find_references(
    doc: &Document,
    pos: Position,
    include_declaration: bool,
    uri: &Uri,
) -> Vec<Location> {
    let cursor = doc.offset_of(pos);
    let table = doc.sym_table();
    let name = ident_at(doc.text(), cursor);
    let Some(name) = name else { return Vec::new() };

    // Anchor: where does the cursor's name resolve to?
    let Some(def_off) = table.resolve(cursor, &name) else {
        return Vec::new();
    };

    let root = doc.parse().syntax();
    let mut out = Vec::new();
    for token in root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
    {
        let tk = token.kind();
        if !matches!(tk, SyntaxKind::Ident | SyntaxKind::DottedIdent) {
            continue;
        }
        if token.text() != name {
            continue;
        }
        let off: usize = token.text_range().start().into();

        let parent_kind = token.parent().map(|p| p.kind());
        let in_param_list = is_in_kind(&token, SyntaxKind::ParamList);

        // Two cases for inclusion:
        // - this ident *resolves* to the same def as the cursor, or
        // - this ident *is* the def itself (ParamList entry, list-pattern
        //   element, assign LHS) — recognised by offset == def_off.
        let resolves_to_def = (off as usize) == def_off
            || (!in_param_list
                && matches!(parent_kind, Some(SyntaxKind::IdentExpr | SyntaxKind::Namespace))
                && table.resolve(off, &name).map(|d| d == def_off).unwrap_or(false));

        if !resolves_to_def {
            continue;
        }

        let is_decl = (off as usize) == def_off;
        if is_decl && !include_declaration {
            continue;
        }

        let start = doc.position_of(off);
        let end = doc.position_of(off + name.len());
        out.push(Location {
            uri: uri.clone(),
            range: Range::new(start, end),
        });
    }

    out
}

fn ident_at(text: &str, offset: usize) -> Option<String> {
    if offset > text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut start = offset;
    let mut end = offset;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(text[start..end].to_string())
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

fn is_in_kind(token: &q_parser::SyntaxToken, kind: SyntaxKind) -> bool {
    let mut cur = token.parent();
    while let Some(node) = cur {
        if node.kind() == kind {
            return true;
        }
        cur = node.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(src: &str, cursor: usize, include_decl: bool) -> Vec<usize> {
        let doc = Document::new(src.to_string(), 0);
        let uri: Uri = "file:///x.q".parse().unwrap();
        let pos = doc.position_of(cursor);
        find_references(&doc, pos, include_decl, &uri)
            .into_iter()
            .map(|loc| {
                doc.offset_of(loc.range.start)
            })
            .collect()
    }

    #[test]
    fn finds_global_uses() {
        let src = "x:1; y:x+x; z:x";
        let cursor = src.find("x:1").unwrap();
        let r = refs(src, cursor, true);
        assert_eq!(
            r,
            vec![
                src.find("x:1").unwrap(),
                src.find("x+x").unwrap(),
                src.find("x+x").unwrap() + 2,
                src.rfind('x').unwrap(),
            ]
        );
    }

    #[test]
    fn excludes_declaration_when_asked() {
        let src = "x:1; y:x+x";
        let cursor = src.find("x:1").unwrap();
        let r = refs(src, cursor, false);
        // Only the two uses, not the def.
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|&o| o != src.find("x:1").unwrap()));
    }

    #[test]
    fn lambda_param_scope() {
        let src = "x:99; f:{[x] x+1}; y:x";
        // Cursor on the parameter `[x]`.
        let cursor = src.find("[x]").unwrap() + 1;
        let r = refs(src, cursor, true);
        // Param def + the `x+1` reference. The outer `x:99` and `y:x`
        // refer to a *different* x.
        assert_eq!(r.len(), 2);
        assert!(r.contains(&(src.find("[x]").unwrap() + 1)));
        assert!(r.contains(&src.find("x+1").unwrap()));
    }

    #[test]
    fn cursor_off_ident_returns_empty() {
        let src = "a:1; b:2;";
        // Cursor on the leading semicolon — no ident here.
        let cursor = src.find(';').unwrap();
        let r = refs(src, cursor, true);
        assert!(r.is_empty(), "got {r:?}");
    }
}
