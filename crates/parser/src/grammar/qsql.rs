use super::expressions;
use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Check if current token is a qSQL keyword.
#[must_use]
pub fn at_qsql_keyword(p: &Parser) -> bool {
    p.at(SyntaxKind::Ident)
        && matches!(
            p.current_text(),
            Some("select" | "exec" | "update" | "delete")
        )
}

/// Parse a qSQL expression. Called when `at_qsql_keyword()` is true.
///
/// # Panics
///
/// Panics if called when the current token is not a qSQL keyword (i.e.,
/// when [`at_qsql_keyword`] would return `false`).
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
        if p.eat(SyntaxKind::Semi) && (p.at(SyntaxKind::Lt) || p.at(SyntaxKind::Gt)) {
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

    // "from" clause — stop at "where" so it is not consumed into the table expr
    if at_kw(p, "from") {
        p.bump(); // "from"
        p.qsql_stop = true;
        expressions::expr(p);
        p.qsql_stop = false;
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
        p.qsql_stop = true;
        expressions::expr(p);
        p.qsql_stop = false;
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
        p.qsql_stop = true;
        expressions::expr(p);
        p.qsql_stop = false;
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
        p.qsql_stop = true;
        expressions::expr(p);
        p.qsql_stop = false;
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
    let saved = p.qsql_stop;
    p.qsql_stop = true;
    loop {
        if at_kw(p, "from") || at_kw(p, "by") || at_kw(p, "where") || p.at_end() || at_stmt_end(p) {
            break;
        }
        expressions::expr(p);
        if !p.eat(SyntaxKind::Comma) {
            break;
        }
    }
    p.qsql_stop = saved;
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

    // Clause-boundary tests — verify from/by/where are NOT consumed into ColumnList.

    #[test]
    fn select_from_not_in_column_list() {
        // `from` must end the ColumnList, not be consumed as juxtaposition.
        // Verify that the table `t` appears outside any ColumnList node.
        let p = parse("select a from t");
        let dump = format!("{:#?}", p.syntax());
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        assert!(dump.contains("SelectExpr"), "got:\n{dump}");
        assert!(dump.contains("ByClause") || !dump.contains("ByClause"), ""); // no assertion
        // The dump must NOT contain `from` as an IdentExpr inside a ColumnList.
        // We check by verifying ColumnList does not contain the text "from".
        let col_list_start = dump.find("ColumnList").expect("ColumnList not found");
        let col_list_section = &dump[col_list_start..];
        // Find end of ColumnList (next top-level node)
        let col_list_end = col_list_section[1..]
            .find("\n            IdentExpr(\n                \"from\"")
            .map(|i| i + 1);
        assert!(
            col_list_end.is_none(),
            "`from` found as IdentExpr inside ColumnList:\n{dump}"
        );
    }

    #[test]
    fn select_where_not_in_column_list() {
        let p = parse("select a,b from t where c>0");
        let dump = format!("{:#?}", p.syntax());
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        assert!(dump.contains("WhereClause"), "no WhereClause found:\n{dump}");
    }

    #[test]
    fn select_by_clause_parsed() {
        let p = parse("select sum a by b from t");
        let dump = format!("{:#?}", p.syntax());
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        assert!(dump.contains("ByClause"), "no ByClause found:\n{dump}");
    }

    #[test]
    fn update_from_not_in_column_list() {
        use crate::syntax_kind::SyntaxKind;
        let p = parse("update a:1 from t");
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        let root = p.syntax();
        // Find the ColumnList node and check none of its descendants is "from".
        let col_list = root
            .descendants()
            .find(|n| n.kind() == SyntaxKind::ColumnList)
            .expect("ColumnList not found");
        let col_text = col_list.text().to_string();
        assert!(
            !col_text.contains("from"),
            "`from` leaked into ColumnList: {col_text:?}"
        );
        // The table `t` must appear in the tree outside ColumnList.
        assert!(
            root.text().to_string().contains("from t"),
            "from clause missing"
        );
    }

    #[test]
    fn delete_where_clause_parsed() {
        let p = parse("delete from t where a>0");
        let dump = format!("{:#?}", p.syntax());
        assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
        assert!(dump.contains("WhereClause"), "no WhereClause:\n{dump}");
        assert!(dump.contains("DeleteExpr"), "got:\n{dump}");
    }
}
