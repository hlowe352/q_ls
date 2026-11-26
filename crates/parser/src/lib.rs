//! Parser for q/kdb+ 4.1 source text.
//!
//! Produces a lossless concrete syntax tree (CST) via
//! [rowan](https://docs.rs/rowan). Every byte of the input is preserved in
//! the tree, including whitespace and comments.
//!
//! # Quick start
//! ```
//! let src = "select avg price by sym from trade";
//! let parse = q_parser::parse(src);
//! assert!(parse.errors.is_empty());
//! assert_eq!(parse.syntax().text().to_string(), src);
//! ```
//!
//! # Architecture
//! - `grammar` — Pratt-style grammar rules (right-to-left, equal precedence)
//! - `parser` — event-driven parser driver and token-stream fix-ups
//! - `sink` — converts events into a rowan `GreenNode`
//! - `syntax_kind` — all token + node kind discriminants

mod grammar;
mod parser;
mod sink;
mod syntax_kind;

pub use parser::ParseError;
pub use rowan::{TextRange, TextSize};
pub use syntax_kind::{QLang, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

use rowan::GreenNode;

/// Parse q source text into a lossless syntax tree.
///
/// Always succeeds — parse errors are accumulated in [`Parse::errors`] rather
/// than returned as a `Result`. The returned tree covers 100 % of the input
/// bytes regardless of errors.
#[must_use]
pub fn parse(source: &str) -> Parse {
    let mut p = parser::Parser::new(source);
    let m = p.start();
    grammar::root(&mut p);
    p.eat_trivia(); // trailing trivia
    m.complete(&mut p, SyntaxKind::Root);

    let (events, errors) = p.finish();
    Parse {
        green: sink::build(events),
        errors,
    }
}

/// Result of parsing a q source file.
///
/// The tree is always lossless: `parse.syntax().text() == original_source`.
#[derive(Debug)]
pub struct Parse {
    green: GreenNode,
    /// Syntax errors encountered during parsing. Non-empty does not prevent
    /// tree construction — the CST is always returned.
    pub errors: Vec<ParseError>,
}

impl Parse {
    /// Root [`SyntaxNode`] of the concrete syntax tree.
    #[must_use]
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}
