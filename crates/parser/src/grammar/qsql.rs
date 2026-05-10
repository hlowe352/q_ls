use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;
use super::expressions;

/// Check if current token is a qSQL keyword.
pub fn at_qsql_keyword(p: &Parser) -> bool {
    p.at(SyntaxKind::Ident) && matches!(
        p.current_text(),
        Some("select" | "exec" | "update" | "delete")
    )
}

/// Parse a qSQL expression. Called when at_qsql_keyword() is true.
pub fn parse_qsql(p: &mut Parser) {
    let text = p.current_text().unwrap();
    match text {
        "select" => parse_select(p),
        "exec" => parse_exec(p),
        "update" => parse_update(p),
        "delete" => parse_delete(p),
        _ => unreachable!(),
    }
}

fn parse_select(p: &mut Parser) {
    let m = p.start();
    p.bump(); // "select"

    // Optional limit: select[n] or select[n;>col]
    if p.at(SyntaxKind::LBracket) {
        let lm = p.start();
        p.bump(); // [
        if !p.at(SyntaxKind::RBracket) && !p.at(SyntaxKind::Semi) {
            if p.at(SyntaxKind::Lt) || p.at(SyntaxKind::Gt) {
                parse_order(p);
            } else {
                expressions::expr(p);
            }
        }
        if p.eat(SyntaxKind::Semi)
            && (p.at(SyntaxKind::Lt) || p.at(SyntaxKind::Gt))
        {
            parse_order(p);
        }
        p.expect(SyntaxKind::RBracket);
        lm.complete(p, SyntaxKind::LimitClause);
    }

    // Optional "distinct"
    if at_kw(p, "distinct") {
        p.bump();
    }

    // Optional columns (if not immediately "from" or "by")
    if !at_kw(p, "from") && !at_kw(p, "by") && !p.at_end() && !at_stmt_end(p) {
        parse_column_list(p);
    }

    // Optional "by" clause
    if at_kw(p, "by") {
        let bm = p.start();
        p.bump(); // "by"
        parse_column_list(p);
        bm.complete(p, SyntaxKind::ByClause);
    }

    // "from" clause
    if at_kw(p, "from") {
        p.bump(); // "from"
        expressions::expr(p);
    }

    // Optional "where" clause
    if at_kw(p, "where") {
        let wm = p.start();
        p.bump(); // "where"
        parse_where_list(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::SelectExpr);
}

fn parse_exec(p: &mut Parser) {
    let m = p.start();
    p.bump(); // "exec"

    // Optional "distinct"
    if at_kw(p, "distinct") {
        p.bump();
    }

    if !at_kw(p, "from") && !at_kw(p, "by") && !p.at_end() && !at_stmt_end(p) {
        parse_column_list(p);
    }

    if at_kw(p, "by") {
        let bm = p.start();
        p.bump();
        parse_column_list(p);
        bm.complete(p, SyntaxKind::ByClause);
    }

    if at_kw(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_kw(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_list(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::ExecExpr);
}

fn parse_update(p: &mut Parser) {
    let m = p.start();
    p.bump(); // "update"

    if !at_kw(p, "from") && !p.at_end() && !at_stmt_end(p) {
        parse_column_list(p);
    }

    if at_kw(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_kw(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_list(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::UpdateExpr);
}

fn parse_delete(p: &mut Parser) {
    let m = p.start();
    p.bump(); // "delete"

    if !at_kw(p, "from") && !p.at_end() && !at_stmt_end(p) {
        parse_column_list(p);
    }

    if at_kw(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_kw(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_list(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::DeleteExpr);
}

/// Parse comma-separated column expressions, stopping at qSQL keywords.
fn parse_column_list(p: &mut Parser) {
    let m = p.start();
    loop {
        if at_kw(p, "from") || at_kw(p, "by") || at_kw(p, "where") || p.at_end() || at_stmt_end(p) {
            break;
        }
        expressions::expr(p);
        if !p.eat(SyntaxKind::Comma) {
            break;
        }
    }
    m.complete(p, SyntaxKind::ColumnList);
}

/// Parse comma-separated where conditions.
fn parse_where_list(p: &mut Parser) {
    loop {
        if p.at_end() || at_stmt_end(p) {
            break;
        }
        expressions::expr(p);
        if !p.eat(SyntaxKind::Comma) {
            break;
        }
    }
}

/// Check if current token is a specific contextual keyword.
fn at_kw(p: &Parser, kw: &str) -> bool {
    p.at(SyntaxKind::Ident) && p.current_text() == Some(kw)
}

/// Check if we are at a statement boundary (newline or semicolon).
fn at_stmt_end(p: &Parser) -> bool {
    p.at(SyntaxKind::Semi) || p.at(SyntaxKind::Newline)
}

fn parse_order(p: &mut Parser) {
    let om = p.start();
    p.bump(); // > or <
    expressions::expr(p);
    om.complete(p, SyntaxKind::OrderClause);
}

#[cfg(test)]
mod tests {
    use crate::parse;

    #[test]
    fn parse_select_limit() {
        let parse = parse("select[5] col from t");
        let dump = format!("{:#?}", parse.syntax());
        assert!(dump.contains("LimitClause"), "got:\n{dump}");
    }

    #[test]
    fn parse_select_limit_with_order() {
        let parse = parse("select[5;>price] col from t");
        let dump = format!("{:#?}", parse.syntax());
        assert!(dump.contains("LimitClause"), "got:\n{dump}");
        assert!(dump.contains("OrderClause"), "got:\n{dump}");
    }

    #[test]
    fn parse_select_order_only() {
        let parse = parse("select[>price] col from t");
        let dump = format!("{:#?}", parse.syntax());
        assert!(dump.contains("OrderClause"), "got:\n{dump}");
    }
}
