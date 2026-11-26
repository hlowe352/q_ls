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

/// Text of the first direct `Ident` child token of `node`.
fn ident_text(node: &SyntaxNode) -> Option<String> {
    node.children_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .find(|t| t.kind() == SyntaxKind::Ident)
        .map(|t| t.text().to_string())
}

/// Is `bin` a `BinExpr` whose operator is `:` or `::`?
pub fn is_assignment(bin: &SyntaxNode) -> bool {
    bin.kind() == SyntaxKind::BinExpr
        && bin
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .any(|t| matches!(t.kind(), SyntaxKind::Colon | SyntaxKind::ColonColon))
}

/// Is `node` the LHS of an assignment — i.e. a definition, not a reference?
pub fn is_assignment_lhs(node: &SyntaxNode) -> bool {
    node.parent()
        .is_some_and(|p| is_assignment(&p) && p.first_child().as_ref() == Some(node))
}

pub fn has_ancestor(node: &SyntaxNode, kind: SyntaxKind) -> bool {
    node.ancestors().any(|n| n.kind() == kind)
}

/// Is `node` inside a qSQL statement? Statement-level qSQL has its own node
/// kinds; inside lambdas or `$[…]` the parser may emit `ApplyExpr` chains
/// with `select`/`exec`/`update`/`delete` in function position.
pub fn in_qsql(node: &SyntaxNode) -> bool {
    node.ancestors().any(|n| match n.kind() {
        SyntaxKind::SelectExpr
        | SyntaxKind::UpdateExpr
        | SyntaxKind::ExecExpr
        | SyntaxKind::DeleteExpr => true,
        SyntaxKind::ApplyExpr => n
            .first_child()
            .filter(|f| f.kind() == SyntaxKind::IdentExpr)
            .and_then(|f| ident_text(&f))
            .is_some_and(|t| matches!(t.as_str(), "select" | "exec" | "update" | "delete")),
        _ => false,
    })
}

/// Is `node` an `ApplyExpr` whose function is `IdentExpr("from")`?
pub fn is_from_apply(node: &SyntaxNode) -> bool {
    node.kind() == SyntaxKind::ApplyExpr
        && node
            .first_child()
            .filter(|f| f.kind() == SyntaxKind::IdentExpr)
            .and_then(|f| ident_text(&f))
            .is_some_and(|t| t == "from")
}

/// Is `node` a qSQL statement node?
pub fn is_qsql_stmt(node: &SyntaxNode) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::SelectExpr
            | SyntaxKind::UpdateExpr
            | SyntaxKind::ExecExpr
            | SyntaxKind::DeleteExpr
    )
}
