use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind;

/// Parse an expression.
pub fn expr(p: &mut Parser) {
    expr_bp(p, 0);
}

/// Pratt parser with binding power for right-to-left evaluation.
///
/// In q, ALL operators have equal precedence and are right-associative.
/// `2*3+4` evaluates as `2*(3+4)` = 14.
///
/// We use binding power (l_bp=1, r_bp=0): since we recurse with r_bp=0,
/// any operator to the right will always win (its l_bp=1 >= min_bp=0),
/// so it gets consumed into the RHS — giving right-associativity.
fn expr_bp(p: &mut Parser, min_bp: u8) {
    let Some(mut lhs) = atom(p) else {
        return;
    };

    loop {
        // Postfix: adverbs (' / \ ': /: \:) bind tight to the left operand
        if is_adverb(p) {
            let m = lhs.precede(p);
            p.bump();
            lhs = m.complete(p, SyntaxKind::AdverbExpr);
            continue;
        }

        // Postfix: indexing expr[...]
        if p.at(SyntaxKind::LBracket) {
            let m = lhs.precede(p);
            parse_arg_list(p);
            lhs = m.complete(p, SyntaxKind::IndexExpr);
            continue;
        }

        // Binary operator (or projection if no RHS: `1+` is valid q)
        if let Some(_op) = binary_op(p) {
            // Right-to-left: l_bp=1, r_bp=0 → right-associative at equal level
            let (l_bp, r_bp) = (1u8, 0u8);
            if l_bp < min_bp {
                break;
            }

            let m = lhs.precede(p);
            p.bump(); // consume operator
            // Only parse RHS if next token can be part of an expression.
            // Otherwise this is a projection (e.g., `1+` or `2*`).
            if !at_expr_boundary(p) {
                expr_bp(p, r_bp);
            }
            lhs = m.complete(p, SyntaxKind::BinExpr);
            continue;
        }

        // Juxtaposition: `f x` — implicit function application.
        // If the next token can start an expression (atom), treat it as
        // applying `lhs` to the next expression.
        if can_start_expr(p) {
            let (l_bp, r_bp) = (1u8, 0u8);
            if l_bp < min_bp {
                break;
            }
            let m = lhs.precede(p);
            expr_bp(p, r_bp);
            lhs = m.complete(p, SyntaxKind::ApplyExpr);
            continue;
        }

        break;
    }
}

/// Parse an atomic expression (leaves and prefix constructs).
fn atom(p: &mut Parser) -> Option<CompletedMarker> {
    let kind = p.current()?;
    match kind {
        // Literals
        SyntaxKind::Integer
        | SyntaxKind::Float
        | SyntaxKind::Boolean
        | SyntaxKind::String
        | SyntaxKind::Symbol
        | SyntaxKind::Date
        | SyntaxKind::Month
        | SyntaxKind::Guid
        | SyntaxKind::Timespan
        | SyntaxKind::Datetime
        | SyntaxKind::Minute
        | SyntaxKind::Second
        | SyntaxKind::Time
        | SyntaxKind::Timestamp
        | SyntaxKind::ByteList => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralExpr))
        }

        // FileSymbol is a distinct expression kind
        SyntaxKind::FileSymbol => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::FileSymbolExpr))
        }

        // Identifiers (with control word detection)
        SyntaxKind::Ident | SyntaxKind::DottedIdent => {
            // Control words: if[...], do[...], while[...]
            if kind == SyntaxKind::Ident && p.nth(1) == Some(SyntaxKind::LBracket)
                && let Some(text) = p.current_text()
            {
                let ctrl_kind = match text.as_str() {
                    "if" => Some(SyntaxKind::IfExpr),
                    "do" => Some(SyntaxKind::DoExpr),
                    "while" => Some(SyntaxKind::WhileExpr),
                    _ => None,
                };
                if let Some(sk) = ctrl_kind {
                    return parse_control_word(p, sk);
                }
            }
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::IdentExpr))
        }

        // Monadic (unary prefix) operators — consume and parse their operand
        // with high bp so they bind tightly.
        //
        // Special case: if the very next non-trivia token after the operator
        // is an adverb (e.g. `+/x`), we do NOT recurse for an operand here.
        // Instead we return the operator as a bare UnaryExpr with no operand,
        // and let the caller's postfix loop attach the adverb.  The adverb
        // then wraps the whole thing, and the argument (`x`) becomes part of
        // the surrounding binary expression.
        SyntaxKind::Minus
        | SyntaxKind::Plus
        | SyntaxKind::Star
        | SyntaxKind::Percent
        | SyntaxKind::Bang
        | SyntaxKind::Amp
        | SyntaxKind::Pipe
        | SyntaxKind::Caret
        | SyntaxKind::Hash
        | SyntaxKind::Underscore
        | SyntaxKind::Tilde
        | SyntaxKind::Eq
        | SyntaxKind::Lt
        | SyntaxKind::Gt
        | SyntaxKind::NotEq
        | SyntaxKind::LtEq
        | SyntaxKind::GtEq
        | SyntaxKind::At
        | SyntaxKind::Query
        | SyntaxKind::Dot
        | SyntaxKind::Comma
        | SyntaxKind::ColonColon
        | SyntaxKind::CompoundAssign
        | SyntaxKind::FileOp0
        | SyntaxKind::FileOp1
        | SyntaxKind::FileOp2
        | SyntaxKind::Each
        | SyntaxKind::EachPrior
        | SyntaxKind::EachRight
        | SyntaxKind::EachLeft => {
            let m = p.start();
            p.bump();
            // Don't consume operand if:
            // - followed by adverb (e.g. `+/x` — let postfix handle it)
            // - followed by `[` (e.g. `'[f;g]` — let postfix indexing handle it)
            // - at expression boundary (e.g. `1+;` — projection)
            if !is_adverb(p)
                && !at_expr_boundary(p)
                && p.current() != Some(SyntaxKind::LBracket)
            {
                expr_bp(p, 100); // high bp: bind tightly to next token
            }
            Some(m.complete(p, SyntaxKind::UnaryExpr))
        }

        // Parenthesised expression, list, or table
        SyntaxKind::LParen => parse_paren(p),

        // Lambda: {[params] body} or {body}
        SyntaxKind::LBrace => parse_lambda(p),

        // Conditional $[cond;true;false] or monadic $
        SyntaxKind::Dollar => {
            if p.nth(1) == Some(SyntaxKind::LBracket) {
                let m = p.start();
                p.bump(); // $
                parse_arg_list(p);
                Some(m.complete(p, SyntaxKind::CondExpr))
            } else {
                // monadic $
                let m = p.start();
                p.bump();
                expr_bp(p, 100);
                Some(m.complete(p, SyntaxKind::UnaryExpr))
            }
        }

        // Colon as monadic (identity / return)
        SyntaxKind::Colon => {
            let m = p.start();
            p.bump();
            if !at_expr_boundary(p) && p.current() != Some(SyntaxKind::LBracket) {
                expr_bp(p, 100);
            }
            Some(m.complete(p, SyntaxKind::UnaryExpr))
        }

        _ => {
            p.error(format!("unexpected token: {:?}", kind));
            None
        }
    }
}

fn parse_paren(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(); // (

    // Empty list: ()
    if p.at(SyntaxKind::RParen) {
        p.bump();
        return Some(m.complete(p, SyntaxKind::ListExpr));
    }

    // Table literal: ([] ...) or keyed table: ([key:val;...] ...)
    if p.at(SyntaxKind::LBracket) {
        if p.nth(1) == Some(SyntaxKind::RBracket) {
            // Simple table: ([] col:val; ...)
            p.bump(); // [
            p.bump(); // ]
        } else {
            // Keyed table: ([key:val;...] col:val; ...)
            parse_arg_list(p);
        }
        while !p.at(SyntaxKind::RParen) && !p.at_end() {
            parse_list_entry(p);
            if !p.eat(SyntaxKind::Semi) {
                break;
            }
        }
        p.expect(SyntaxKind::RParen);
        return Some(m.complete(p, SyntaxKind::TableExpr));
    }

    // First entry (expression or assignment)
    parse_list_entry(p);

    if p.at(SyntaxKind::Semi) {
        // List: (expr; expr; ...)
        while p.eat(SyntaxKind::Semi) {
            if !p.at(SyntaxKind::RParen) {
                parse_list_entry(p);
            }
        }
        p.expect(SyntaxKind::RParen);
        Some(m.complete(p, SyntaxKind::ListExpr))
    } else {
        // Simple paren: (expr)
        p.expect(SyntaxKind::RParen);
        Some(m.complete(p, SyntaxKind::ParenExpr))
    }
}

/// Parse a control word: if[...], do[...], while[...]
fn parse_control_word(p: &mut Parser, kind: SyntaxKind) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(); // keyword (if/do/while)
    parse_arg_list(p);
    Some(m.complete(p, kind))
}

fn parse_lambda(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(); // {

    // Optional parameter list: [x;y;z] or [x:type;y:type;z]
    if p.at(SyntaxKind::LBracket) {
        let pm = p.start();
        p.bump(); // [
        while !p.at(SyntaxKind::RBracket) && !p.at_end() {
            p.expect(SyntaxKind::Ident);
            // Optional type annotation: name:type
            if p.at(SyntaxKind::Colon) {
                p.bump(); // :
                // Type can be an identifier, symbol, or expression
                if !p.at(SyntaxKind::Semi) && !p.at(SyntaxKind::RBracket) {
                    expr(p);
                }
            }
            if !p.eat(SyntaxKind::Semi) {
                break;
            }
        }
        p.expect(SyntaxKind::RBracket);
        pm.complete(p, SyntaxKind::ParamList);
    }

    // Body: statements separated by ; (assignments allowed inside lambdas)
    while !p.at(SyntaxKind::RBrace) && !p.at_end() {
        parse_lambda_stmt(p);
        if !p.eat(SyntaxKind::Semi) {
            break;
        }
    }
    p.expect(SyntaxKind::RBrace);
    Some(m.complete(p, SyntaxKind::Lambda))
}

/// Parse an entry that may be an expression or assignment.
/// Since `:` and `::` are now binary operators, assignments like `x:42`
/// parse as BinExpr(x, :, 42) inside expr().
fn parse_stmt_or_expr(p: &mut Parser) {
    let m = p.start();
    expr(p);
    m.complete(p, SyntaxKind::ExprStmt);
}

/// Parse a statement inside a lambda body (expression or assignment).
fn parse_lambda_stmt(p: &mut Parser) {
    parse_stmt_or_expr(p);
}

/// Parse a list entry (expression or assignment, e.g. `0=pos: loc mod 100`).
fn parse_list_entry(p: &mut Parser) {
    parse_stmt_or_expr(p);
}

/// Parse bracketed argument list: [expr;expr;...]
/// Arguments can be expressions or assignments (e.g. `$[x:cond;true;false]`).
pub fn parse_arg_list(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LBracket);
    while !p.at(SyntaxKind::RBracket) && !p.at_end() {
        // Allow empty args (trailing semicolons like $[cond;true;])
        if p.at(SyntaxKind::Semi) {
            p.bump();
            continue;
        }
        parse_arg_entry(p);
        if !p.eat(SyntaxKind::Semi) {
            break;
        }
    }
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::ArgList);
}

/// Parse a single argument entry (expression or assignment).
fn parse_arg_entry(p: &mut Parser) {
    parse_stmt_or_expr(p);
}

/// Returns the current token if it is a binary (dyadic) operator.
fn binary_op(p: &Parser) -> Option<SyntaxKind> {
    let kind = p.current()?;
    // Operator followed by `[` is functional form (op[args]), not dyadic.
    // e.g. `@[tab;col;:;val]` is amend, `$[cond;t;f]` is conditional.
    if p.nth(1) == Some(SyntaxKind::LBracket) {
        return None;
    }
    match kind {
        SyntaxKind::Plus
        | SyntaxKind::Minus
        | SyntaxKind::Star
        | SyntaxKind::Percent
        | SyntaxKind::Bang
        | SyntaxKind::Amp
        | SyntaxKind::Pipe
        | SyntaxKind::Caret
        | SyntaxKind::Hash
        | SyntaxKind::Underscore
        | SyntaxKind::Tilde
        | SyntaxKind::Dollar
        | SyntaxKind::At
        | SyntaxKind::Query
        | SyntaxKind::Dot
        | SyntaxKind::Comma
        | SyntaxKind::Eq
        | SyntaxKind::NotEq
        | SyntaxKind::LtEq
        | SyntaxKind::GtEq
        | SyntaxKind::Lt
        | SyntaxKind::Gt
        | SyntaxKind::CompoundAssign
        | SyntaxKind::FileOp0
        | SyntaxKind::FileOp1
        | SyntaxKind::FileOp2
        | SyntaxKind::Colon
        | SyntaxKind::ColonColon => Some(kind),
        _ => None,
    }
}

/// Returns `true` if we're at a statement/expression boundary (no more RHS to parse).
fn at_expr_boundary(p: &Parser) -> bool {
    match p.current() {
        None => true,
        Some(k) => matches!(
            k,
            SyntaxKind::Semi
                | SyntaxKind::RBrace
                | SyntaxKind::RBracket
                | SyntaxKind::RParen
        ),
    }
}

/// Returns `true` if the current token can start an expression (for juxtaposition).
/// This includes anything handled by `atom()`.
fn can_start_expr(p: &Parser) -> bool {
    matches!(
        p.current(),
        Some(
            // Literals
            SyntaxKind::Integer
                | SyntaxKind::Float
                | SyntaxKind::Boolean
                | SyntaxKind::String
                | SyntaxKind::Symbol
                | SyntaxKind::FileSymbol
                | SyntaxKind::Date
                | SyntaxKind::Month
                | SyntaxKind::Guid
                | SyntaxKind::Timespan
                | SyntaxKind::Datetime
                | SyntaxKind::Minute
                | SyntaxKind::Second
                | SyntaxKind::Time
                | SyntaxKind::Timestamp
                | SyntaxKind::ByteList
                // Identifiers
                | SyntaxKind::Ident
                | SyntaxKind::DottedIdent
                // Delimiters
                | SyntaxKind::LParen
                | SyntaxKind::LBrace
                // Operators (monadic/functional forms)
                | SyntaxKind::Minus
                | SyntaxKind::Plus
                | SyntaxKind::Star
                | SyntaxKind::Percent
                | SyntaxKind::Bang
                | SyntaxKind::Amp
                | SyntaxKind::Pipe
                | SyntaxKind::Caret
                | SyntaxKind::Hash
                | SyntaxKind::Underscore
                | SyntaxKind::Tilde
                | SyntaxKind::Eq
                | SyntaxKind::Lt
                | SyntaxKind::Gt
                | SyntaxKind::NotEq
                | SyntaxKind::LtEq
                | SyntaxKind::GtEq
                | SyntaxKind::At
                | SyntaxKind::Query
                | SyntaxKind::Dot
                | SyntaxKind::Dollar
                | SyntaxKind::Colon
                | SyntaxKind::ColonColon
                | SyntaxKind::Comma
                | SyntaxKind::CompoundAssign
                | SyntaxKind::FileOp0
                | SyntaxKind::FileOp1
                | SyntaxKind::FileOp2
                | SyntaxKind::Each
                | SyntaxKind::EachPrior
                | SyntaxKind::EachRight
                | SyntaxKind::EachLeft
        )
    )
}

/// Returns `true` if the current token is an adverb / iterator.
fn is_adverb(p: &Parser) -> bool {
    matches!(
        p.current(),
        Some(
            SyntaxKind::Slash
                | SyntaxKind::Backslash
                | SyntaxKind::Each
                | SyntaxKind::EachPrior
                | SyntaxKind::EachRight
                | SyntaxKind::EachLeft
        )
    )
}

#[cfg(test)]
mod literal_tests {
    use crate::parse;

    #[test]
    fn parse_temporal_literals_are_atoms() {
        for (src, kind_name) in [
            ("0Nm",       "Month"),
            ("0Ng",       "Guid"),
            ("0Nn",       "Timespan"),
            ("12:30",     "Minute"),
            ("12:30:45",  "Second"),
            ("0Nz",       "Datetime"),
            ("0xABCD",    "ByteList"),
        ] {
            let parse = parse(src);
            let dump = format!("{:#?}", parse.syntax());
            assert!(dump.contains(kind_name), "expected {kind_name} in:\n{dump}");
            assert!(parse.errors.is_empty(), "errors for {src}: {:?}", parse.errors);
        }
    }

    #[test]
    fn parse_file_symbol_makes_file_symbol_expr() {
        let parse = parse("`:foo.csv");
        let dump = format!("{:#?}", parse.syntax());
        assert!(dump.contains("FileSymbolExpr"), "got:\n{dump}");
        assert!(!dump.contains("LiteralExpr"), "should not be LiteralExpr:\n{dump}");
    }
}
