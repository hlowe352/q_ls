use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode, SyntaxToken};
use crate::document::Document;

pub fn document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let root = doc.parse().syntax();
    let mut out = Vec::new();
    for node in root.children() {
        if node.kind() == SyntaxKind::ExprStmt
            && let Some(bin) = node.first_child()
            && bin.kind() == SyntaxKind::BinExpr
            && let Some(sym) = symbol_for_assign(doc, &bin)
        {
            out.push(sym);
        }
    }
    out
}

/// Build a DocumentSymbol for an assignment BinExpr, including any nested
/// assignments inside the RHS lambda body as children.
fn symbol_for_assign(doc: &Document, bin: &SyntaxNode) -> Option<DocumentSymbol> {
    if !is_assignment(bin) {
        return None;
    }
    let name_tok = first_lhs_name(bin)?;
    let name = name_tok.text().to_string();

    let range = bin.text_range();
    let start = doc.position_of(range.start().into());
    let end = doc.position_of(range.end().into());
    let full_range = Range::new(start, end);

    let sel_range = {
        let r = name_tok.text_range();
        Range::new(doc.position_of(r.start().into()), doc.position_of(r.end().into()))
    };

    let lambda = find_rhs_lambda(bin);
    let kind = if lambda.is_some() {
        SymbolKind::FUNCTION
    } else {
        SymbolKind::VARIABLE
    };

    let children = lambda.as_ref().map(|l| collect_lambda_body_symbols(doc, l));

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: full_range,
        selection_range: sel_range,
        children,
    })
}

fn is_assignment(bin: &SyntaxNode) -> bool {
    bin.children_with_tokens()
        .filter_map(|el| el.into_token())
        .any(|t| t.kind() == SyntaxKind::Colon || t.kind() == SyntaxKind::ColonColon)
}

fn first_lhs_name(bin: &SyntaxNode) -> Option<SyntaxToken> {
    let lhs = bin.first_child()?;
    lhs.descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::DottedIdent))
}

/// True if the BinExpr's RHS is (or contains a top-level) lambda.
fn find_rhs_lambda(bin: &SyntaxNode) -> Option<SyntaxNode> {
    // Skip the LHS, find the next sibling node that is or contains a lambda.
    let mut iter = bin.children();
    let _lhs = iter.next();
    for rhs in iter {
        if rhs.kind() == SyntaxKind::Lambda {
            return Some(rhs);
        }
        // Lambda may be wrapped (e.g. compose with each: `'[{...}; enlist]`).
        if let Some(found) = rhs.descendants().find(|n| n.kind() == SyntaxKind::Lambda) {
            return Some(found);
        }
    }
    None
}

/// Walk a lambda body and emit a child DocumentSymbol for each plain
/// assignment, recursing into nested lambdas.
fn collect_lambda_body_symbols(doc: &Document, lambda: &SyntaxNode) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    // Iterative DFS, but stop descending into nested lambdas — those become
    // their own children (handled via symbol_for_assign recursion).
    let mut work: Vec<SyntaxNode> = lambda.children().collect::<Vec<_>>();
    work.reverse();
    while let Some(node) = work.pop() {
        if node.kind() == SyntaxKind::Lambda {
            // Encountered without an enclosing assignment — skip; its body
            // would normally be reached only through a binding.
            continue;
        }
        if node.kind() == SyntaxKind::BinExpr
            && let Some(sym) = symbol_for_assign(doc, &node)
        {
            out.push(sym);
            continue;
        }
        let children: Vec<SyntaxNode> = node.children().collect();
        for c in children.into_iter().rev() {
            work.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(syms: &[DocumentSymbol]) -> Vec<&str> {
        syms.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn top_level_assigns_emitted() {
        let doc = Document::new("a:1; b:2; c:{x+1}".to_string(), 0);
        let syms = document_symbols(&doc);
        assert_eq!(names(&syms), vec!["a", "b", "c"]);
        assert!(matches!(syms[2].kind, SymbolKind::FUNCTION));
        assert!(matches!(syms[0].kind, SymbolKind::VARIABLE));
    }

    #[test]
    fn lambda_locals_become_children() {
        let doc = Document::new("f:{[p] x:p+1; y:x*2; y}".to_string(), 0);
        let syms = document_symbols(&doc);
        assert_eq!(names(&syms), vec!["f"]);
        let kids = syms[0].children.as_ref().expect("f has children");
        assert_eq!(names(kids), vec!["x", "y"]);
    }

    #[test]
    fn nested_lambda_emits_grandchildren() {
        let doc = Document::new("f:{[a] g:{[b] z:b+1; z}; g a}".to_string(), 0);
        let syms = document_symbols(&doc);
        let f_kids = syms[0].children.as_ref().expect("f has kids");
        assert_eq!(names(f_kids), vec!["g"]);
        let g_kids = f_kids[0].children.as_ref().expect("g has kids");
        assert_eq!(names(g_kids), vec!["z"]);
    }
}
