//! CST query helpers shared by the LSP features.

use q_parser::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

/// All tokens under `node`, in source order (trivia included).
pub fn tokens(node: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> + use<> {
    node.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
}

/// First non-trivia token under `node`. rowan attaches whitespace as
/// leading trivia, so `node.first_token()` may be whitespace.
pub fn first_token(node: &SyntaxNode) -> Option<SyntaxToken> {
    tokens(node).find(|t| !t.kind().is_trivia())
}

/// Is `node` the left side of a `:` or `::` assignment?
pub fn is_assignment_lhs(node: &SyntaxNode) -> bool {
    node.parent().is_some_and(|p| {
        p.kind() == SyntaxKind::BinExpr
            && p.first_child().as_ref() == Some(node)
            && p.children_with_tokens()
                .filter_map(SyntaxElement::into_token)
                .any(|t| matches!(t.kind(), SyntaxKind::Colon | SyntaxKind::ColonColon))
    })
}

pub fn has_ancestor(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    node.ancestors().any(|n| n.kind() == kind)
}

fn is_qsql_stmt(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::SelectExpr
            | SyntaxKind::UpdateExpr
            | SyntaxKind::ExecExpr
            | SyntaxKind::DeleteExpr
    )
}

/// Is `node` in qSQL row context, where bare names are column names? True
/// when its nearest enclosing qSQL construct is a statement rather than a
/// `FromClause`, so `from f[x]` is ordinary code but a query nested inside
/// that `from` has its own columns.
pub fn in_row_context(node: &SyntaxNode) -> bool {
    node.ancestors()
        .find(|n| n.kind() == SyntaxKind::FromClause || is_qsql_stmt(n.kind()))
        .is_some_and(|n| n.kind() != SyntaxKind::FromClause)
}
