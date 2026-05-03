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

        // Binary operator
        let Some(_op) = binary_op(p) else {
            break;
        };

        // Right-to-left: l_bp=1, r_bp=0 → right-associative at equal level
        let (l_bp, r_bp) = (1u8, 0u8);
        if l_bp < min_bp {
            break;
        }

        let m = lhs.precede(p);
        p.bump(); // consume operator
        expr_bp(p, r_bp);
        lhs = m.complete(p, SyntaxKind::BinExpr);
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
        | SyntaxKind::Time
        | SyntaxKind::Timestamp => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralExpr))
        }

        // Identifiers
        SyntaxKind::Ident | SyntaxKind::DottedIdent => {
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
        | SyntaxKind::At
        | SyntaxKind::Query => {
            let m = p.start();
            p.bump();
            // If immediately followed by an adverb, don't consume operand now.
            if !is_adverb(p) {
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
            expr_bp(p, 100);
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

    // Table literal: ([] ...)
    if p.at(SyntaxKind::LBracket) && p.nth(1) == Some(SyntaxKind::RBracket) {
        p.bump(); // [
        p.bump(); // ]
        while !p.at(SyntaxKind::RParen) && !p.at_end() {
            expr(p);
            if !p.eat(SyntaxKind::Semi) {
                break;
            }
        }
        p.expect(SyntaxKind::RParen);
        return Some(m.complete(p, SyntaxKind::TableExpr));
    }

    // First expression
    expr(p);

    if p.at(SyntaxKind::Semi) {
        // List: (expr; expr; ...)
        while p.eat(SyntaxKind::Semi) {
            if !p.at(SyntaxKind::RParen) {
                expr(p);
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

fn parse_lambda(p: &mut Parser) -> Option<CompletedMarker> {
    let m = p.start();
    p.bump(); // {

    // Optional parameter list: [x;y;z]
    if p.at(SyntaxKind::LBracket) {
        let pm = p.start();
        p.bump(); // [
        while !p.at(SyntaxKind::RBracket) && !p.at_end() {
            p.expect(SyntaxKind::Ident);
            if !p.eat(SyntaxKind::Semi) {
                break;
            }
        }
        p.expect(SyntaxKind::RBracket);
        pm.complete(p, SyntaxKind::ParamList);
    }

    // Body: expressions separated by ; (newlines are trivia inside braces)
    while !p.at(SyntaxKind::RBrace) && !p.at_end() {
        expr(p);
        if !p.eat(SyntaxKind::Semi) {
            break;
        }
    }
    p.expect(SyntaxKind::RBrace);
    Some(m.complete(p, SyntaxKind::Lambda))
}

/// Parse bracketed argument list: [expr;expr;...]
pub fn parse_arg_list(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LBracket);
    while !p.at(SyntaxKind::RBracket) && !p.at_end() {
        expr(p);
        if !p.eat(SyntaxKind::Semi) {
            break;
        }
    }
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::ArgList);
}

/// Returns the current token if it is a binary (dyadic) operator.
fn binary_op(p: &Parser) -> Option<SyntaxKind> {
    let kind = p.current()?;
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
        | SyntaxKind::At
        | SyntaxKind::Query
        | SyntaxKind::Dot
        | SyntaxKind::Comma
        | SyntaxKind::Eq
        | SyntaxKind::Lt
        | SyntaxKind::Gt => Some(kind),
        _ => None,
    }
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
