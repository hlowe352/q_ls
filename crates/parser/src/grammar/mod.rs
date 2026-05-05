pub mod expressions;
pub mod qsql;

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse the top-level file: a sequence of statements.
pub fn root(p: &mut Parser) {
    while !p.at_end() {
        statement(p);
    }
}

/// Parse a single statement.
pub fn statement(p: &mut Parser) {
    // Skip bare newlines and semicolons (statement separators)
    while p.at(SyntaxKind::Newline) || p.at(SyntaxKind::Semi) {
        p.bump();
    }
    if p.at_end() {
        return;
    }

    // System command
    if p.at(SyntaxKind::SystemCmd) || p.at(SyntaxKind::Exit) {
        let m = p.start();
        p.bump();
        m.complete(p, SyntaxKind::SystemCmdStmt);
        return;
    }

    // qSQL
    if qsql::at_qsql_keyword(p) {
        let m = p.start();
        qsql::parse_qsql(p);
        m.complete(p, SyntaxKind::ExprStmt);
        return;
    }

    // Expression (assignments like `x:42` are parsed as BinExpr(x, :, 42))
    let m = p.start();
    expressions::expr(p);
    m.complete(p, SyntaxKind::ExprStmt);
}
