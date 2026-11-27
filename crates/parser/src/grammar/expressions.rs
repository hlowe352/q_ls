use crate::parser::{CompletedMarker, Parser};
// Glob import: grammar code names dozens of kinds; `SyntaxKind::` everywhere buries the logic.
#[allow(clippy::enum_glob_use)]
use crate::syntax_kind::SyntaxKind::{self, *};

/// Parse an expression.
pub fn expr(p: &mut Parser) {
    expr_bp(p, 0);
}

/// Parse an expression wrapped in an `ExprStmt` (list entries, arguments,
/// lambda body statements — anywhere an assignment like `x:42` may appear).
pub(crate) fn stmt(p: &mut Parser) {
    let m = p.start();
    expr(p);
    m.complete(p, ExprStmt);
}

/// Run `f` with the qSQL clause flags set to `(stop, comma_stop)`, restoring
/// the previous values afterwards. Bracketed sub-expressions run with both
/// flags cleared so clause-boundary behaviour never leaks into them.
pub(crate) fn with_qsql_flags<T>(
    p: &mut Parser,
    stop: bool,
    comma_stop: bool,
    f: impl FnOnce(&mut Parser) -> T,
) -> T {
    let saved = (p.qsql_stop, p.qsql_comma_stop);
    (p.qsql_stop, p.qsql_comma_stop) = (stop, comma_stop);
    let out = f(p);
    (p.qsql_stop, p.qsql_comma_stop) = saved;
    out
}

/// Pratt parser with binding power for right-to-left evaluation.
///
/// In q, ALL operators have equal precedence and are right-associative.
/// `2*3+4` evaluates as `2*(3+4)` = 14.
///
/// We use binding power (`l_bp=1`, `r_bp=0)`: since we recurse with `r_bp=0`,
/// any operator to the right will always win (its `l_bp=1` >= `min_bp=0`),
/// so it gets consumed into the RHS — giving right-associativity. Prefix
/// operands recurse with `min_bp=100`, which stops at the first infix.
fn expr_bp(p: &mut Parser, min_bp: u8) {
    const L_BP: u8 = 1;
    const R_BP: u8 = 0;
    let Some(mut lhs) = atom(p) else {
        return;
    };

    loop {
        // Postfix: adverbs (' / \ ': /: \:) bind tight to the left operand
        if is_adverb(p) {
            let m = lhs.precede(p);
            p.bump();
            lhs = m.complete(p, AdverbExpr);
            continue;
        }

        // Postfix: indexing expr[...]
        if p.at(LBracket) {
            let m = lhs.precede(p);
            parse_bracketed(p, ArgList);
            lhs = m.complete(p, IndexExpr);
            continue;
        }

        // Binary operator (or projection if no RHS: `1+` is valid q)
        if at_binary_op(p) {
            if L_BP < min_bp {
                break;
            }
            let m = lhs.precede(p);
            p.bump(); // operator
            let decorated = is_adverb(p);
            if decorated {
                p.bump(); // adverb modifier
            }
            // No RHS at a boundary: this is a projection (e.g. `1+` or `2*`).
            let had_rhs = !at_expr_boundary(p) && {
                let before = p.events.len();
                expr_bp(p, R_BP);
                p.events.len() > before
            };
            let kind = match (decorated, had_rhs) {
                (true, _) => InfixModExpr,
                (false, true) => BinExpr,
                (false, false) => InfixProjection,
            };
            lhs = m.complete(p, kind);
            continue;
        }

        // Juxtaposition: `f x` — implicit function application.
        if can_juxtapose(p) {
            if L_BP < min_bp {
                break;
            }
            let m = lhs.precede(p);
            expr_bp(p, R_BP);
            lhs = m.complete(p, ApplyExpr);
            continue;
        }

        // A SystemCmd token on the same line as an expression (e.g. `f\x`) is
        // most likely a mistyped scan adverb. kdb+ only allows system commands
        // at line start; after a newline it belongs to the next statement.
        if p.at(SystemCmd) && !p.has_preceding_newline() {
            let text = p.current_text().unwrap_or("\\...").to_string();
            p.error(format!(
                "unexpected `{text}`; did you mean a scan adverb? \
                 Use `(f\\) x` or `f\\[x]` for scan-apply"
            ));
            continue;
        }

        break;
    }
}

/// Parse an atomic expression (leaves and prefix constructs).
fn atom(p: &mut Parser) -> Option<CompletedMarker> {
    let kind = p.current()?;
    match kind {
        LBracket => return Some(parse_bracketed(p, PrognExpr)),
        LParen => return Some(parse_paren(p)),
        LBrace => return Some(parse_lambda(p)),
        // qSQL keywords in expression position: select/exec/update/delete
        Ident if super::qsql::at_qsql_keyword(p) => return Some(super::qsql::parse_qsql(p)),
        // Adjacent scalar literals on one line form a vector (`1 2 3`).
        k if is_scalar_literal(k)
            && p.nth(1).is_some_and(is_scalar_literal)
            && !p.has_newline_after_current() =>
        {
            return Some(parse_vector(p));
        }
        _ => {}
    }
    let m = p.start();
    let node = match kind {
        k if matches!(k, String | Symbol) || is_scalar_literal(k) => {
            p.bump();
            LiteralExpr
        }

        FileSymbol => {
            p.bump();
            FileSymbolExpr
        }

        Ident | DottedIdent => {
            // Control words: if[...], do[...], while[...]
            let ctrl = match p.current_text() {
                Some("if") => Some(IfExpr),
                Some("do") => Some(DoExpr),
                Some("while") => Some(WhileExpr),
                _ => None,
            };
            if let Some(ctrl) = ctrl
                && kind == Ident
                && p.nth(1) == Some(LBracket)
            {
                p.bump();
                parse_bracketed(p, ArgList);
                return Some(m.complete(p, ctrl));
            }
            // `.q` / `.z` alone is a bare namespace; `.q.f` is a member.
            let bare_ns = kind == DottedIdent
                && p.current_text()
                    .is_some_and(|t| t.matches('.').count() == 1);
            p.bump();
            if bare_ns { Namespace } else { IdentExpr }
        }

        // `'[f;g]` composition, `'expr` signal, or bare `'` (e.g. postfix).
        Each => {
            p.bump();
            if p.at(LBracket) {
                parse_bracketed(p, ArgList);
                Composition
            } else if !is_adverb(p) && !at_expr_boundary(p) {
                expr_bp(p, 100);
                SignalExpr
            } else {
                UnaryExpr
            }
        }

        // Monadic verbs. Leave the operand alone when followed by an adverb
        // (`+/x`: the postfix loop wraps the bare verb), by `[` (functional
        // form `op[args]`), or at a boundary (projection like `1+;`).
        k if is_monadic_verb(k) => {
            p.bump();
            if !is_adverb(p) && !at_expr_boundary(p) && !p.at(LBracket) {
                expr_bp(p, 100);
            }
            UnaryExpr
        }

        // `$[cond;true;false]` conditional, or monadic `$`.
        Dollar => {
            p.bump();
            if p.at(LBracket) {
                parse_bracketed(p, ArgList);
                CondExpr
            } else {
                expr_bp(p, 100);
                UnaryExpr
            }
        }

        // `:expr` return, or bare `:` (identity).
        Colon => {
            p.bump();
            if !at_expr_boundary(p) && !p.at(LBracket) {
                expr_bp(p, 100);
                ReturnExpr
            } else {
                UnaryExpr
            }
        }

        _ => {
            m.abandon();
            let msg = if kind == SystemCmd {
                format!(
                    "system command `{}` must appear at the start of a line",
                    p.current_text().unwrap_or("\\...")
                )
            } else {
                format!("unexpected token: {kind:?}")
            };
            p.error(msg);
            return None;
        }
    };
    Some(m.complete(p, node))
}

/// `1 2 3`: run of same-line scalar literals. Leading trivia is emitted
/// before opening the node so its range starts at the first literal.
fn parse_vector(p: &mut Parser) -> CompletedMarker {
    p.eat_trivia();
    let m = p.start();
    while p.current().is_some_and(is_scalar_literal) && !p.has_preceding_newline() {
        let em = p.start();
        p.bump();
        em.complete(p, LiteralExpr);
    }
    m.complete(p, VectorExpr)
}

/// `[stmt; stmt; ...]` as `kind` (`ArgList` or `PrognExpr`). Empty entries
/// (`$[c;t;]`) are allowed.
fn parse_bracketed(p: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
    let m = p.start();
    with_qsql_flags(p, false, false, |p| {
        p.expect(LBracket);
        while !p.at(RBracket) && !p.at_end() {
            if p.eat(Semi) {
                continue;
            }
            stmt(p);
            if !p.eat(Semi) {
                break;
            }
        }
        p.expect(RBracket);
    });
    m.complete(p, kind)
}

/// `(expr)`, `(a;b;c)`, `()`, or a table `([keys] col:val; ...)`.
fn parse_paren(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    let kind = with_qsql_flags(p, false, false, |p| {
        p.bump(); // (
        if p.eat(RParen) {
            return ListExpr;
        }
        if p.at(LBracket) {
            if p.nth(1) == Some(RBracket) {
                p.bump(); // [
                p.bump(); // ]
            } else {
                let km = p.start();
                parse_bracketed(p, ArgList);
                km.complete(p, TableKeys);
            }
            while !p.at(RParen) && !p.at_end() {
                stmt(p);
                if !p.eat(Semi) {
                    break;
                }
            }
            p.expect(RParen);
            return TableExpr;
        }
        stmt(p);
        let kind = if p.at(Semi) {
            while p.eat(Semi) {
                if !p.at(RParen) {
                    stmt(p);
                }
            }
            ListExpr
        } else {
            ParenExpr
        };
        p.expect(RParen);
        kind
    });
    m.complete(p, kind)
}

/// `{[params] body}` or `{body}`.
fn parse_lambda(p: &mut Parser) -> CompletedMarker {
    let m = p.start();
    with_qsql_flags(p, false, false, |p| {
        p.bump(); // {
        if p.at(LBracket) {
            let pm = p.start();
            p.bump(); // [
            while !p.at(RBracket) && !p.at_end() {
                // tree-sitter-q also allows literal parameter "names".
                if matches!(p.current(), Some(Ident | Integer | Float | Symbol)) {
                    p.bump();
                } else {
                    p.expect(Ident);
                }
                // Optional type annotation: `name:type`
                if p.eat(Colon) && !p.at(Semi) && !p.at(RBracket) {
                    expr(p);
                }
                if !p.eat(Semi) {
                    break;
                }
            }
            p.expect(RBracket);
            pm.complete(p, ParamList);
        }
        while !p.at(RBrace) && !p.at_end() {
            stmt(p);
            if !p.eat(Semi) {
                break;
            }
        }
        p.expect(RBrace);
    });
    m.complete(p, Lambda)
}

/// Is the current token a binary (dyadic) operator?
fn at_binary_op(p: &Parser) -> bool {
    let Some(kind) = p.current() else {
        return false;
    };
    // In qSQL clause lists comma is a separator, not the join dyad.
    if p.qsql_comma_stop && kind == Comma {
        return false;
    }
    // `op[args]` is a functional form (`@[t;c;:;v]`, `$[c;t;f]`), not dyadic.
    if p.nth(1) == Some(LBracket) {
        return false;
    }
    // Builtin infix verbs: `x mmu y`, `a in b`, `t lj u`, etc.
    if kind == Ident {
        return p
            .current_text()
            .is_some_and(super::keywords::is_builtin_infix);
    }
    is_verb(kind) || matches!(kind, Dollar | Colon)
}

/// At a statement/expression boundary (no more RHS to parse)?
fn at_expr_boundary(p: &Parser) -> bool {
    matches!(p.current(), None | Some(Semi | RBrace | RBracket | RParen))
}

/// Should juxtaposition (`f x`) be parsed at the current position?
fn can_juxtapose(p: &Parser) -> bool {
    let Some(kind) = p.current() else {
        return false;
    };
    let can_start_expr = is_scalar_literal(kind)
        || is_monadic_verb(kind)
        || matches!(
            kind,
            String
                | Symbol
                | FileSymbol
                | Ident
                | DottedIdent
                | LParen
                | LBrace
                | LBracket
                | Dollar
                | Colon
                | Each
        );
    let at_clause_boundary = (p.qsql_stop
        && matches!(p.current_text(), Some("from" | "by" | "where")))
        || (p.qsql_comma_stop && kind == Comma);
    can_start_expr && !p.has_preceding_newline() && !at_clause_boundary
}

/// Literals that form a vector when space-separated (`1 2 3`, `0b 1b`).
/// Strings and symbols are literals but not vector elements: `` `a`b `` is
/// one token, and `` `a `b `` is application.
fn is_scalar_literal(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        Integer
            | Float
            | Boolean
            | Date
            | Month
            | Guid
            | Timespan
            | Datetime
            | Minute
            | Second
            | Time
            | Timestamp
            | ByteList
    )
}

/// Primitive verbs valid in both monadic and dyadic position.
fn is_verb(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        Plus | Minus
            | Star
            | Percent
            | Bang
            | Amp
            | Pipe
            | Caret
            | Hash
            | Underscore
            | Tilde
            | At
            | Query
            | Dot
            | Comma
            | Eq
            | NotEq
            | LtEq
            | GtEq
            | Lt
            | Gt
            | CompoundAssign
            | FileOp0
            | FileOp1
            | FileOp2
            | ColonColon
            | Slash
            | Backslash
    )
}

/// Tokens parsed as a monadic `UnaryExpr` prefix.
fn is_monadic_verb(kind: SyntaxKind) -> bool {
    is_verb(kind) || matches!(kind, EachPrior | EachRight | EachLeft)
}

/// Is the current token an adverb / iterator?
fn is_adverb(p: &Parser) -> bool {
    matches!(
        p.current(),
        Some(Slash | Backslash | Each | EachPrior | EachRight | EachLeft)
    )
}
