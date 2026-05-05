use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode};
use crate::document::Document;

pub fn goto_definition(doc: &Document, pos: Position, uri: &Url) -> Option<GotoDefinitionResponse> {
    let offset = doc.offset_of(pos);
    let target_name = get_identifier_at(doc.text(), offset)?;
    let root = doc.parse().syntax();
    let def_offset = find_definition(&root, &target_name)?;
    let def_pos = doc.position_of(def_offset);

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: Range::new(def_pos, def_pos),
    }))
}

/// Find the byte offset where `name` is defined (assigned to).
/// Assignments parse as BinExpr(ident, Colon, rhs).
fn find_definition(root: &SyntaxNode, name: &str) -> Option<usize> {
    for node in root.descendants() {
        if node.kind() == SyntaxKind::BinExpr {
            // Check if operator is Colon (assignment)
            let has_colon = node.children_with_tokens()
                .filter_map(|el| el.into_token())
                .any(|t| t.kind() == SyntaxKind::Colon || t.kind() == SyntaxKind::ColonColon);
            if !has_colon { continue; }

            // Get the first child node (LHS, typically IdentExpr)
            if let Some(lhs) = node.first_child() {
                let name_token = lhs.descendants_with_tokens()
                    .filter_map(|el| el.into_token())
                    .find(|t| !t.kind().is_trivia());
                if let Some(token) = name_token
                    && token.text() == name {
                    return Some(token.text_range().start().into());
                }
            }
        }
    }
    None
}

fn get_identifier_at(text: &str, offset: usize) -> Option<String> {
    if offset >= text.len() { return None; }
    let bytes = text.as_bytes();
    let mut start = offset;
    let mut end = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) { start -= 1; }
    while end < bytes.len() && is_ident_char(bytes[end]) { end += 1; }
    if start == end { return None; }
    Some(text[start..end].to_string())
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}
