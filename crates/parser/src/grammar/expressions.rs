//! Expressions. q has no precedence: an expression is a run of terms read
//! right to left. A *term* is an atom followed by any `[args]` and adverbs;
//! it is a *verb* if it is a primitive (`+`, `in`) or ends in an adverb
//! (`f/`, `+'`), and a *noun* otherwise (`x`, `f[x]`, `+/[x]`, `(+)`).
//!
//! - noun verb expr: infix (`x+y`, `x f/ y`); without `expr`, a projection (`x+`)
//! - noun expr: application (`f x`)
//! - verb expr: an error — q has no prefix verbs; write `neg x` or `(+/)x`
//!
//! An expression ending in a verb with nothing to its right (`f+`, `+/`) is
//! *verb-valued*. Applied to one, or as the right operand of a verb, q builds
//! a composition instead (`g f+`, `x+y+`), except as the value of a plain
//! assignment (`f:+/`). `f::+` composes too: in a lambda it reads `f` and
//! assigns nothing. Brackets and parentheses delimit their own
//! expressions, so `f[g+]` and `g (x+)` are plain application.
//!
//! The only prefix forms are `'x` (signal) and `:x` (return).

use crate::parser::{CompletedMarker, Parser};
// Glob import: grammar code names dozens of kinds; `SyntaxKind::` everywhere buries the logic.
#[allow(clippy::enum_glob_use)]
use crate::syntax_kind::SyntaxKind::{self, *};

/// Parse an expression (empty before `;`: `(;1)`, `f[;1]`), returning
/// whether it is verb-valued.
pub fn expr(p: &mut Parser) -> bool {
    if p.at(Semi) {
        return false;
    }
    let start = p.token_index();
    term(p).is_some_and(|(lhs, verb)| rest(p, lhs, verb, start))
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

/// Continue an expression after its first term `lhs` (a verb if `verb`),
/// which began at token index `start`. Returns whether it is verb-valued.
fn rest(p: &mut Parser, lhs: CompletedMarker, verb: bool, start: usize) -> bool {
    // A `\cmd` on the same line as an expression (`f\x`) is most likely a
    // mistyped scan; system commands only start a line.
    while p.at(SystemCmd) && !at_boundary(p) {
        let text = p.current_text().unwrap_or("\\...").to_string();
        p.error(format!(
            "unexpected `{text}`; did you mean a scan adverb? \
             Use `(f\\) x` or `f\\[x]` for scan-apply"
        ));
    }
    if at_boundary(p) {
        return verb;
    }
    let m = lhs.precede(p);
    let (kind, verb_valued) = if verb {
        p.error_since("q has no prefix verbs: write `(f) x` or `f[x]`", start);
        expr(p);
        (ApplyExpr, false)
    } else if at_primitive(p) {
        let assign = p.at(Colon);
        p.bump();
        infix(p, assign)
    } else {
        let start = p.token_index();
        let (t, t_verb) = term(p).expect("a term follows when not at a boundary");
        if t_verb {
            infix(p, false)
        } else if rest(p, t, false, start) {
            (Composition, true)
        } else {
            (ApplyExpr, false)
        }
    };
    m.complete(p, kind);
    verb_valued
}

/// After `noun verb`: the right operand, or a projection (`x+`) without one.
/// A verb-valued right operand composes, unless the verb assigns.
fn infix(p: &mut Parser, assign: bool) -> (SyntaxKind, bool) {
    if at_boundary(p) {
        return (InfixProjection, true);
    }
    let verb_valued = expr(p);
    let kind = if verb_valued && !assign {
        Composition
    } else {
        BinExpr
    };
    (kind, verb_valued)
}

/// An atom with its `[args]` and adverbs, and whether it is a verb.
fn term(p: &mut Parser) -> Option<(CompletedMarker, bool)> {
    let (mut t, mut verb) = atom(p)?;
    loop {
        let (kind, is_verb) = if p.current().is_some_and(is_adverb) {
            (AdverbExpr, true)
        } else if p.at(LBracket) {
            (IndexExpr, false)
        } else {
            break;
        };
        if p.has_preceding_newline() {
            break;
        }
        let m = t.precede(p);
        if is_verb {
            p.bump();
        } else {
            parse_bracketed(p, ArgList);
        }
        (t, verb) = (m.complete(p, kind), is_verb);
    }
    Some((t, verb))
}

/// Parse an atom, and whether it is a verb. `None` only at end of input.
fn atom(p: &mut Parser) -> Option<(CompletedMarker, bool)> {
    let kind = p.current()?;
    let noun = |m| Some((m, false));
    match kind {
        LBracket => return noun(parse_bracketed(p, PrognExpr)),
        LParen => return noun(parse_paren(p)),
        LBrace => return noun(parse_lambda(p)),
        // qSQL keywords in expression position: select/exec/update/delete
        Ident if super::qsql::at_qsql_keyword(p) => return noun(super::qsql::parse_qsql(p)),
        // Adjacent scalar literals on one line form a vector (`1 2 3`).
        k if is_scalar_literal(k)
            && p.nth(1).is_some_and(is_scalar_literal)
            && !p.has_newline_after_current() =>
        {
            return noun(parse_vector(p));
        }
        _ => {}
    }
    let m = p.start();
    let (node, verb) = match kind {
        k if matches!(k, String | Symbol | Boolean | ByteList) || is_scalar_literal(k) => {
            p.bump();
            (LiteralExpr, false)
        }

        FileSymbol => {
            p.bump();
            (FileSymbolExpr, false)
        }

        Ident | DottedIdent => {
            // `.q` / `.z` alone is a bare namespace; `.q.f` is a member.
            let bare_ns = kind == DottedIdent
                && p.current_text()
                    .is_some_and(|t| t.matches('.').count() == 1);
            let verb = at_infix_word(p);
            p.bump();
            (if bare_ns { Namespace } else { IdentExpr }, verb)
        }

        // `'x` signal and `:x` return; bare, they are verbs (`x:'y`, `'[f;g]`).
        Each | Colon => {
            p.bump();
            if at_boundary(p) || modifies(p.current()) {
                (VerbExpr, true)
            } else {
                expr(p);
                (if kind == Each { SignalExpr } else { ReturnExpr }, false)
            }
        }

        k if is_glyph(k) || is_adverb(k) => {
            p.bump();
            (VerbExpr, true)
        }

        _ => {
            let msg = if kind == SystemCmd {
                format!(
                    "system command `{}` must appear at the start of a line",
                    p.current_text().unwrap_or("\\...")
                )
            } else {
                format!("unexpected `{}`", p.current_text().unwrap_or_default())
            };
            p.error(msg);
            (Error, false)
        }
    };
    Some((m.complete(p, node), verb))
}

/// `1 2 3`: run of same-line scalar literals. Leading trivia is emitted
/// before opening the node so its range starts at the first literal. As in
/// q, only the last item may carry a type letter (`1 2j`, not `1j 2`), and
/// a number cannot follow a time (`12:00 1`).
fn parse_vector(p: &mut Parser) -> CompletedMarker {
    p.eat_trivia();
    let m = p.start();
    let (mut suffixed, mut seen_time) = (None, false);
    while p.current().is_some_and(is_scalar_literal) && !p.has_preceding_newline() {
        if let Some(i) = suffixed.take() {
            p.error_since("only a vector's last item may have a type letter", i);
        }
        let (i, number) = (
            p.token_index(),
            matches!(p.current(), Some(Integer | Float)),
        );
        let typed = number
            && p.current_text()
                .is_some_and(|t| t.ends_with(['i', 'j', 'h', 'f', 'e']));
        let em = p.start();
        p.bump();
        em.complete(p, LiteralExpr);
        if number && seen_time {
            p.error_since("a vector cannot have a number after a time", i);
        }
        seen_time |= !number;
        suffixed = typed.then_some(i);
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

/// At a bare primitive verb (`+`, `:`, `in`) with no adverb or `[args]`?
/// After a noun it is infix with the verb as a token of the `BinExpr`.
fn at_primitive(p: &Parser) -> bool {
    (p.current().is_some_and(is_glyph) || at_infix_word(p)) && !modifies(p.nth(1))
}

/// At a builtin word that q treats as a verb (`x in y`, `x each`)?
fn at_infix_word(p: &Parser) -> bool {
    p.at(Ident)
        && p.current_text()
            .is_some_and(super::keywords::is_builtin_infix)
}

/// At the end of the current expression: a closer, a column-0 line (a new
/// statement), or a qSQL clause boundary.
fn at_boundary(p: &Parser) -> bool {
    matches!(p.current(), None | Some(Semi | RBrace | RBracket | RParen))
        || p.has_preceding_newline()
        || (p.qsql_stop && matches!(p.current_text(), Some("from" | "by" | "where")))
        || (p.qsql_comma_stop && p.at(Comma))
}

/// Literals that form a vector when space-separated (`1 2 3`). Other
/// literals juxtapose as application: `` `a `b ``, `1b 0b`, `0x01 0x02`
/// (their vectors are single tokens: `` `a`b ``, `10b`, `0x0102`).
pub(crate) fn is_scalar_literal(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        Integer
            | Float
            | Date
            | Month
            | Guid
            | Timespan
            | Datetime
            | Minute
            | Second
            | Time
            | Timestamp
    )
}

/// Primitive verb glyphs.
fn is_glyph(kind: SyntaxKind) -> bool {
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
            | Dollar
            | Colon
            | ColonColon
            | CompoundAssign
            | FileOp0
            | FileOp1
            | FileOp2
    )
}

/// Does `kind` (the token after a verb) modify it: `[args]` or an adverb?
fn modifies(kind: Option<SyntaxKind>) -> bool {
    kind.is_some_and(|k| k == LBracket || is_adverb(k))
}

/// Adverbs / iterators.
fn is_adverb(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        Slash | Backslash | Each | EachPrior | EachRight | EachLeft
    )
}
