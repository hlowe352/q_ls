use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode, SyntaxElement};
use crate::document::Document;

pub fn document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let root = doc.parse().syntax();
    let mut symbols = Vec::new();

    // Only look at top-level statements (direct children of Root)
    for node in root.children() {
        if node.kind() == SyntaxKind::AssignStmt {
            if let Some(sym) = extract_symbol(doc, &node) {
                symbols.push(sym);
            }
        }
    }

    symbols
}

fn extract_symbol(doc: &Document, node: &SyntaxNode) -> Option<DocumentSymbol> {
    // Get the name from first child (should be IdentExpr or token)
    let first = node.first_child_or_token()?;
    let name = match first {
        SyntaxElement::Node(n) => n.first_token()?.text().to_string(),
        SyntaxElement::Token(t) => t.text().to_string(),
    };

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

fn has_lambda(node: &SyntaxNode) -> bool {
    for child in node.descendants() {
        if child.kind() == SyntaxKind::Lambda {
            return true;
        }
    }
    false
}
