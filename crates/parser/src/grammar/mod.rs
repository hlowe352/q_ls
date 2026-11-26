pub mod expressions;
pub mod keywords;
pub mod qsql;

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse the top-level file: a sequence of statements.
pub fn root(p: &mut Parser) {
    while !p.at_end() {
        statement(p);
        p.eat_trivia();
    }
}

/// Parse a single statement.
pub fn statement(p: &mut Parser) {
    // Skip semicolon separators (newlines are trivia).
    while p.eat(SyntaxKind::Semi) {}
    let single = match p.current() {
        None => return,
        Some(SyntaxKind::DslLine) => Some(SyntaxKind::DslStmt),
        Some(SyntaxKind::SystemCmd | SyntaxKind::Exit) => Some(SyntaxKind::SystemCmdStmt),
        Some(_) => None,
    };
    let m = p.start();
    if let Some(kind) = single {
        p.bump();
        m.complete(p, kind);
    } else {
        expressions::expr(p);
        m.complete(p, SyntaxKind::ExprStmt);
    }
}
