use super::expressions::{expr, with_qsql_flags};
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind;

/// Is the current token a qSQL keyword (`select`/`exec`/`update`/`delete`)?
#[must_use]
pub fn at_qsql_keyword(p: &Parser) -> bool {
    p.at(SyntaxKind::Ident)
        && matches!(
            p.current_text(),
            Some("select" | "exec" | "update" | "delete")
        )
}

/// Parse a qSQL expression. Call only when [`at_qsql_keyword`] is true.
///
/// `select[n;>col]` limits are select-only, `distinct` is select/exec-only,
/// `by` is valid everywhere but `delete`; every form takes optional `from`
/// and `where`.
pub fn parse_qsql(p: &mut Parser) -> CompletedMarker {
    let kind = match p.current_text() {
        Some("select") => SyntaxKind::SelectExpr,
        Some("exec") => SyntaxKind::ExecExpr,
        Some("update") => SyntaxKind::UpdateExpr,
        _ => SyntaxKind::DeleteExpr,
    };
    let has_by = kind != SyntaxKind::DeleteExpr;
    let m = p.start();
    p.bump(); // keyword

    if kind == SyntaxKind::SelectExpr && p.at(SyntaxKind::LBracket) {
        let lm = p.start();
        p.bump(); // [
        if !p.at(SyntaxKind::RBracket) && !p.at(SyntaxKind::Semi) {
            if at_order(p) {
                parse_order(p);
            } else {
                expr(p);
            }
        }
        if p.eat(SyntaxKind::Semi) && at_order(p) {
            parse_order(p);
        }
        p.expect(SyntaxKind::RBracket);
        lm.complete(p, SyntaxKind::LimitClause);
    }

    if matches!(kind, SyntaxKind::SelectExpr | SyntaxKind::ExecExpr) && at_kw(p, "distinct") {
        p.bump();
    }

    let at_clause_end =
        at_kw(p, "from") || (has_by && at_kw(p, "by")) || p.at_end() || p.at(SyntaxKind::Semi);
    if !at_clause_end {
        parse_column_list(p);
    }

    if has_by && at_kw(p, "by") {
        let bm = p.start();
        p.bump();
        parse_column_list(p);
        bm.complete(p, SyntaxKind::ByClause);
    }

    // Table expression: stop juxtaposition at `where`.
    if at_kw(p, "from") {
        let fm = p.start();
        p.bump();
        let comma = p.qsql_comma_stop;
        with_qsql_flags(p, true, comma, expr);
        fm.complete(p, SyntaxKind::FromClause);
    }

    if at_kw(p, "where") {
        let wm = p.start();
        p.bump();
        let stop = p.qsql_stop;
        with_qsql_flags(p, stop, true, |p| comma_separated(p, false));
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, kind)
}

/// Comma-separated column expressions, stopping at qSQL keywords.
fn parse_column_list(p: &mut Parser) {
    let m = p.start();
    with_qsql_flags(p, true, true, |p| comma_separated(p, true));
    m.complete(p, SyntaxKind::ColumnList);
}

/// Parse `expr, expr, ...` until a statement end (or, if `stop_at_kw`, a
/// `from`/`by`/`where` keyword).
fn comma_separated(p: &mut Parser, stop_at_kw: bool) {
    loop {
        let at_kw = stop_at_kw && (at_kw(p, "from") || at_kw(p, "by") || at_kw(p, "where"));
        if at_kw || p.at_end() || p.at(SyntaxKind::Semi) {
            break;
        }
        expr(p);
        if !p.eat(SyntaxKind::Comma) {
            break;
        }
    }
}

/// Is the current token the contextual keyword `kw`?
fn at_kw(p: &Parser, kw: &str) -> bool {
    p.at(SyntaxKind::Ident) && p.current_text() == Some(kw)
}

fn at_order(p: &Parser) -> bool {
    p.at(SyntaxKind::Lt) || p.at(SyntaxKind::Gt)
}

/// `>col` / `<col` inside a `select[...]` limit.
fn parse_order(p: &mut Parser) {
    let om = p.start();
    p.bump(); // > or <
    expr(p);
    om.complete(p, SyntaxKind::OrderClause);
}
