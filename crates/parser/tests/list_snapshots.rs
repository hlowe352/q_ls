//! Snapshot tests for space-separated vector / list literal parsing.

use q_parser::{SyntaxNode, SyntaxToken, parse};
use rowan::NodeOrToken;
use std::fmt::Write as _;

fn dump(node: &SyntaxNode) -> String {
    let mut out = String::new();
    dump_node(&mut out, node, 0);
    out
}

fn dump_node(out: &mut String, node: &SyntaxNode, depth: usize) {
    let indent = "  ".repeat(depth);
    writeln!(out, "{indent}{:?}", node.kind()).unwrap();
    for child in node.children_with_tokens() {
        match child {
            NodeOrToken::Node(n) => dump_node(out, &n, depth + 1),
            NodeOrToken::Token(t) => dump_token(out, &t, depth + 1),
        }
    }
}

fn dump_token(out: &mut String, token: &SyntaxToken, depth: usize) {
    if token.kind().is_trivia() {
        return;
    }
    let indent = "  ".repeat(depth);
    writeln!(out, "{indent}{:?} {:?}", token.kind(), token.text()).unwrap();
}

fn tree(src: &str) -> String {
    let p = parse(src);
    assert!(
        p.errors.is_empty(),
        "unexpected parse errors for {src:?}: {:?}",
        p.errors
    );
    dump(&p.syntax()).trim().to_string()
}

// ---------------------------------------------------------------------------
// Basic vector literals
// ---------------------------------------------------------------------------

#[test]
fn integer_vector() {
    assert_eq!(
        tree("1 2 3"),
        r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Integer "1"
      LiteralExpr
        Integer "2"
      LiteralExpr
        Integer "3""#
    );
}

#[test]
fn float_vector() {
    assert_eq!(
        tree("1.0 2.0 3.0"),
        r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Float "1.0"
      LiteralExpr
        Float "2.0"
      LiteralExpr
        Float "3.0""#
    );
}

#[test]
fn symbol_vector() {
    assert_eq!(
        tree("`a `b `c"),
        r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Symbol "`a"
      LiteralExpr
        Symbol "`b"
      LiteralExpr
        Symbol "`c""#
    );
}

#[test]
fn boolean_vector() {
    assert_eq!(
        tree("0b 1b 0b"),
        r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Boolean "0b"
      LiteralExpr
        Boolean "1b"
      LiteralExpr
        Boolean "0b""#
    );
}

// ---------------------------------------------------------------------------
// Vector in larger expressions
// ---------------------------------------------------------------------------

#[test]
fn vector_in_assignment() {
    // t: 1 2 3 — the user's reported case
    assert_eq!(
        tree("t: 1 2 3"),
        r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "t"
      Colon ":"
      VectorExpr
        LiteralExpr
          Integer "1"
        LiteralExpr
          Integer "2"
        LiteralExpr
          Integer "3""#
    );
}

#[test]
fn func_applied_to_vector() {
    // f 1 2 3 — f applied to the vector 1 2 3
    assert_eq!(
        tree("f 1 2 3"),
        r#"Root
  ExprStmt
    ApplyExpr
      IdentExpr
        Ident "f"
      VectorExpr
        LiteralExpr
          Integer "1"
        LiteralExpr
          Integer "2"
        LiteralExpr
          Integer "3""#
    );
}

#[test]
fn vector_then_binop() {
    // 1 2 3 + 1 — vector addition
    assert_eq!(
        tree("1 2 3 + 1"),
        r#"Root
  ExprStmt
    BinExpr
      VectorExpr
        LiteralExpr
          Integer "1"
        LiteralExpr
          Integer "2"
        LiteralExpr
          Integer "3"
      Plus "+"
      LiteralExpr
        Integer "1""#
    );
}

#[test]
fn vector_indexed() {
    // 1 2 3 4 f — vector indexed/applied with f
    assert_eq!(
        tree("1 2 3 4 f"),
        r#"Root
  ExprStmt
    ApplyExpr
      VectorExpr
        LiteralExpr
          Integer "1"
        LiteralExpr
          Integer "2"
        LiteralExpr
          Integer "3"
        LiteralExpr
          Integer "4"
      IdentExpr
        Ident "f""#
    );
}

// ---------------------------------------------------------------------------
// Negative literals in vectors: `-23` (no space after minus) → one token
// ---------------------------------------------------------------------------

#[test]
fn vector_with_negative_elements() {
    // 1 2 3 -23 — space before minus, no space after → -23 is a negative literal
    assert_eq!(
        tree("1 2 3 -23"),
        r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Integer "1"
      LiteralExpr
        Integer "2"
      LiteralExpr
        Integer "3"
      LiteralExpr
        Integer "-23""#
    );
}

#[test]
fn vector_negative_floats() {
    assert_eq!(
        tree("1.0 -2.5 3.0"),
        r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Float "1.0"
      LiteralExpr
        Float "-2.5"
      LiteralExpr
        Float "3.0""#
    );
}

#[test]
fn subtraction_not_merged_when_space_after_minus() {
    // 1 2 3 - 23 — space on BOTH sides of minus → subtraction
    let p = q_parser::parse("1 2 3 - 23");
    assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
    let dump = format!("{:#?}", p.syntax());
    assert!(dump.contains("BinExpr"), "expected BinExpr for subtraction:\n{dump}");
    assert!(!dump.contains("Integer(\"-"), "minus should not be merged:\n{dump}");
}

#[test]
fn subtraction_not_merged_when_no_space_before_minus() {
    // 3-1 — no space before minus → subtraction
    let p = q_parser::parse("3-1");
    assert!(p.errors.is_empty(), "unexpected errors: {:?}", p.errors);
    let dump = format!("{:#?}", p.syntax());
    assert!(dump.contains("BinExpr"), "expected BinExpr:\n{dump}");
}

#[test]
fn negative_literal_after_paren() {
    // (-23) — after LParen → merge into single Integer "-23" token
    assert_eq!(
        tree("(-23)"),
        r#"Root
  ExprStmt
    ParenExpr
      LParen "("
      ExprStmt
        LiteralExpr
          Integer "-23"
      RParen ")""#
    );
}

// ---------------------------------------------------------------------------
// Non-regression: single literal and ident juxtaposition stay as ApplyExpr
// ---------------------------------------------------------------------------

#[test]
fn func_applied_to_single_arg_not_vector() {
    // f x — still ApplyExpr, not VectorExpr
    assert_eq!(
        tree("f x"),
        r#"Root
  ExprStmt
    ApplyExpr
      IdentExpr
        Ident "f"
      IdentExpr
        Ident "x""#
    );
}
