use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode};
use crate::document::Document;

pub fn document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let root = doc.parse().syntax();
    let mut symbols = Vec::new();

    // Top-level statements: assignments are BinExpr(ident, Colon, rhs) inside ExprStmt
    for node in root.children() {
        if node.kind() == SyntaxKind::ExprStmt
            && let Some(bin) = node.first_child()
            && bin.kind() == SyntaxKind::BinExpr && is_assignment(&bin)
            && let Some(sym) = extract_symbol(doc, &bin)
        {
            symbols.push(sym);
        }
    }

    symbols
}

fn first_non_trivia_token(node: &SyntaxNode) -> Option<q_parser::SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| !t.kind().is_trivia())
}

fn extract_symbol(doc: &Document, node: &SyntaxNode) -> Option<DocumentSymbol> {
    // Get the name from the first non-trivia token of the LHS (IdentExpr)
    let first_child = node.first_child()?;
    let name_token = first_non_trivia_token(&first_child)?;
    let name = name_token.text().to_string();

    let range = node.text_range();
    let start = doc.position_of(range.start().into());
    let end = doc.position_of(range.end().into());
    let full_range = Range::new(start, end);

    // Determine if value is a lambda (function) or variable
    let kind = if has_lambda(node) {
        SymbolKind::FUNCTION
    } else {
        SymbolKind::VARIABLE
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: full_range,
        selection_range: full_range,
        children: None,
    })
}

fn is_assignment(node: &SyntaxNode) -> bool {
    node.children_with_tokens()
        .filter_map(|el| el.into_token())
        .any(|t| t.kind() == SyntaxKind::Colon || t.kind() == SyntaxKind::ColonColon)
}

fn has_lambda(node: &SyntaxNode) -> bool {
    for child in node.descendants() {
        if child.kind() == SyntaxKind::Lambda {
            return true;
        }
    }
    false
}
