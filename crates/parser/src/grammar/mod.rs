pub mod expressions;

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
    // Skip bare newlines
    while p.at(SyntaxKind::Newline) {
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

    // Expression or assignment
    let m = p.start();
    expressions::expr(p);

    if p.at(SyntaxKind::Colon) || p.at(SyntaxKind::ColonColon) {
        p.bump();
        expressions::expr(p);
        m.complete(p, SyntaxKind::AssignStmt);
    } else {
        m.complete(p, SyntaxKind::ExprStmt);
    }
}
