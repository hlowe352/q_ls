pub mod event;
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
    // Temporary: just consume all tokens (grammar comes in Task 5)
    while !p.at_end() {
        p.bump();
    }
    p.eat_trivia();
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
        let node = parse.syntax();
        assert_eq!(node.text().to_string(), source);
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
    fn parse_returns_root() {
        let parse = parse("1+2");
        assert_eq!(parse.syntax().kind(), SyntaxKind::Root);
    }
}
