pub mod event;
pub mod grammar;
pub mod parser;
pub mod syntax_kind;
pub mod sink;

pub use syntax_kind::{QLang, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxElement};
pub use parser::ParseError;

use rowan::GreenNode;

/// Parse q source and return a lossless syntax tree + errors.
pub fn parse(source: &str) -> Parse {
    let mut p = parser::Parser::new(source);
    let m = p.start();
    grammar::root(&mut p);
    p.eat_trivia(); // trailing trivia
    m.complete(&mut p, SyntaxKind::Root);

    let (events, errors) = p.finish();
    let (green, errors) = sink::Sink::new(events, errors).finish();
    Parse { green, errors }
}

#[derive(Debug)]
pub struct Parse {
    green: GreenNode,
    pub errors: Vec<ParseError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
    pub fn green(&self) -> &GreenNode {
        &self.green
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_preserves_text() {
        let source = "x:42";
        let parse = parse(source);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn parse_with_whitespace() {
        let source = "x : 42 + 3";
        let parse = parse(source);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn parse_multiline() {
        let source = "a:1\nb:2";
        let parse = parse(source);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn parse_integer_literal() {
        let parse = parse("42");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_binary_expr() {
        let parse = parse("1+2");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), "1+2");
    }

    #[test]
    fn parse_assignment() {
        let parse = parse("x:42");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_lambda() {
        let parse = parse("{[x;y] x+y}");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), "{[x;y] x+y}");
    }

    #[test]
    fn parse_lambda_no_params() {
        let parse = parse("{x*x}");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_list() {
        let parse = parse("(1;2;3)");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_empty_list() {
        let parse = parse("()");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_conditional() {
        let parse = parse("$[x>0;x;0]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_right_to_left() {
        // Structure should be 2*(3+4), not (2*3)+4
        let parse = parse("2*3+4");
        assert!(parse.errors.is_empty());
        assert_eq!(parse.syntax().text().to_string(), "2*3+4");
    }

    #[test]
    fn parse_adverb() {
        let parse = parse("+/x");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_system_cmd() {
        let parse = parse("\\l file.q");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_error_recovery() {
        // Should not panic on malformed input
        let parse = parse(")invalid");
        assert!(!parse.errors.is_empty());
        // Still lossless
        assert_eq!(parse.syntax().text().to_string(), ")invalid");
    }
}
