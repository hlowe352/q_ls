//! Snapshot tests for adverb / iterator CST shapes.
//!
//! Each test asserts the *exact* tree produced by the parser so that
//! future grammar changes cannot silently alter adverb precedence or
//! associativity.  Only non-trivia tokens are shown in the dump.

use q_parser::{SyntaxNode, SyntaxToken, parse};
use rowan::NodeOrToken;
use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// Tree dumper
// ---------------------------------------------------------------------------

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

/// Parse `src` and return a trimmed tree dump, asserting no parse errors.
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
// Postfix adverbs (AdverbExpr wraps the function + adverb token)
// ---------------------------------------------------------------------------

#[test]
fn adverb_over_on_ident() {
    // f/  →  AdverbExpr(IdentExpr(f), /)
    assert_eq!(
        tree("f/"),
        r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      Slash "/""#
    );
}

#[test]
fn adverb_scan_on_ident() {
    // f\  →  AdverbExpr(IdentExpr(f), \)
    assert_eq!(
        tree("f\\"),
        r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      Backslash "\\""#
    );
}

#[test]
fn adverb_each_on_ident() {
    // f'  →  AdverbExpr(IdentExpr(f), ')
    assert_eq!(
        tree("f'"),
        r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      Each "'""#
    );
}

#[test]
fn adverb_each_prior_on_ident() {
    // f':  →  AdverbExpr(IdentExpr(f), ':)
    assert_eq!(
        tree("f':"),
        r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      EachPrior "':""#
    );
}

#[test]
fn adverb_each_right_on_ident() {
    // f/:  →  AdverbExpr(IdentExpr(f), /:)
    assert_eq!(
        tree("f/:"),
        r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      EachRight "/:""#
    );
}

#[test]
fn adverb_each_left_on_ident() {
    // f\:  →  AdverbExpr(IdentExpr(f), \:)
    assert_eq!(
        tree("f\\:"),
        r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      EachLeft "\\:""#
    );
}

// ---------------------------------------------------------------------------
// Adverb on a builtin operator (the + / - etc. start as UnaryExpr
// when immediately followed by an adverb, then get wrapped)
// ---------------------------------------------------------------------------

#[test]
fn adverb_over_on_builtin() {
    // +/  →  AdverbExpr(UnaryExpr(+), /)
    assert_eq!(
        tree("+/"),
        r#"Root
  ExprStmt
    AdverbExpr
      UnaryExpr
        Plus "+"
      Slash "/""#
    );
}

#[test]
fn adverb_scan_on_builtin() {
    // +\  →  AdverbExpr(UnaryExpr(+), \)
    assert_eq!(
        tree("+\\"),
        r#"Root
  ExprStmt
    AdverbExpr
      UnaryExpr
        Plus "+"
      Backslash "\\""#
    );
}

// ---------------------------------------------------------------------------
// Applying an adverbed function: AdverbExpr then juxtaposed argument
// ---------------------------------------------------------------------------

#[test]
fn over_applied_to_list() {
    // +/x  →  ApplyExpr(AdverbExpr(UnaryExpr(+), /), IdentExpr(x))
    assert_eq!(
        tree("+/x"),
        r#"Root
  ExprStmt
    ApplyExpr
      AdverbExpr
        UnaryExpr
          Plus "+"
        Slash "/"
      IdentExpr
        Ident "x""#
    );
}

#[test]
fn each_applied_to_function_and_arg() {
    // count'x  →  ApplyExpr(AdverbExpr(IdentExpr(count), '), IdentExpr(x))
    assert_eq!(
        tree("count'x"),
        r#"Root
  ExprStmt
    ApplyExpr
      AdverbExpr
        IdentExpr
          Ident "count"
        Each "'"
      IdentExpr
        Ident "x""#
    );
}

// ---------------------------------------------------------------------------
// InfixModExpr: builtin operator + adverb used infix  (1 +/ 2)
// ---------------------------------------------------------------------------

#[test]
fn infix_mod_over() {
    // 1+/2  →  InfixModExpr(1, +, /, 2)
    assert_eq!(
        tree("1+/2"),
        r#"Root
  ExprStmt
    InfixModExpr
      LiteralExpr
        Integer "1"
      Plus "+"
      Slash "/"
      LiteralExpr
        Integer "2""#
    );
}

#[test]
fn infix_mod_each_right() {
    // x,/:y  →  InfixModExpr(x, ,, /:, y)
    assert_eq!(
        tree("x,/:y"),
        r#"Root
  ExprStmt
    InfixModExpr
      IdentExpr
        Ident "x"
      Comma ","
      EachRight "/:"
      IdentExpr
        Ident "y""#
    );
}

#[test]
fn infix_mod_each_left() {
    // x,\:y  →  InfixModExpr(x, ,, \:, y)
    assert_eq!(
        tree("x,\\:y"),
        r#"Root
  ExprStmt
    InfixModExpr
      IdentExpr
        Ident "x"
      Comma ","
      EachLeft "\\:"
      IdentExpr
        Ident "y""#
    );
}

#[test]
fn infix_mod_each_prior() {
    // x-':y  →  InfixModExpr(x, -, ':, y)
    assert_eq!(
        tree("x-':y"),
        r#"Root
  ExprStmt
    InfixModExpr
      IdentExpr
        Ident "x"
      Minus "-"
      EachPrior "':"
      IdentExpr
        Ident "y""#
    );
}

// ---------------------------------------------------------------------------
// Chained adverbs: each adverb wraps the previous AdverbExpr
// ---------------------------------------------------------------------------

#[test]
fn chained_adverbs_over_each_left() {
    // f/\:  →  AdverbExpr(AdverbExpr(IdentExpr(f), /), \:)
    assert_eq!(
        tree("f/\\:"),
        r#"Root
  ExprStmt
    AdverbExpr
      AdverbExpr
        IdentExpr
          Ident "f"
        Slash "/"
      EachLeft "\\:""#
    );
}

// ---------------------------------------------------------------------------
// Seeded over/scan with bracket form: f/[n;x]  →  IndexExpr(AdverbExpr(f,/), args)
// ---------------------------------------------------------------------------

#[test]
fn seeded_over_bracket() {
    // f/[10;x]  →  IndexExpr(AdverbExpr(IdentExpr(f), /), ArgList)
    assert_eq!(
        tree("f/[10;x]"),
        r#"Root
  ExprStmt
    IndexExpr
      AdverbExpr
        IdentExpr
          Ident "f"
        Slash "/"
      ArgList
        LBracket "["
        ExprStmt
          LiteralExpr
            Integer "10"
        Semi ";"
        ExprStmt
          IdentExpr
            Ident "x"
        RBracket "]""#
    );
}

#[test]
fn seeded_scan_bracket() {
    // f\[10;x]  →  IndexExpr(AdverbExpr(IdentExpr(f), \), ArgList)
    assert_eq!(
        tree("f\\[10;x]"),
        r#"Root
  ExprStmt
    IndexExpr
      AdverbExpr
        IdentExpr
          Ident "f"
        Backslash "\\"
      ArgList
        LBracket "["
        ExprStmt
          LiteralExpr
            Integer "10"
        Semi ";"
        ExprStmt
          IdentExpr
            Ident "x"
        RBracket "]""#
    );
}

// ---------------------------------------------------------------------------
// Keyword form: each / peach (these are builtin infix → BinExpr)
// ---------------------------------------------------------------------------

#[test]
fn each_keyword_form() {
    // count each x  →  BinExpr(IdentExpr(count), Ident(each), IdentExpr(x))
    assert_eq!(
        tree("count each x"),
        r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "count"
      Ident "each"
      IdentExpr
        Ident "x""#
    );
}

#[test]
fn peach_keyword_form() {
    // f peach x  →  BinExpr(IdentExpr(f), Ident(peach), IdentExpr(x))
    assert_eq!(
        tree("f peach x"),
        r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "f"
      Ident "peach"
      IdentExpr
        Ident "x""#
    );
}

// ---------------------------------------------------------------------------
// Adverb inside qSQL column expression — parser flags must not leak
// ---------------------------------------------------------------------------

#[test]
fn adverb_in_qsql_column() {
    // select sum/price from t  — adverb on sum in select column list
    let p = parse("select +/price from t");
    assert!(
        p.errors.is_empty(),
        "parse errors: {:?}",
        p.errors
    );
    let dump = format!("{:#?}", p.syntax());
    assert!(
        dump.contains("AdverbExpr"),
        "expected AdverbExpr in select column:\n{dump}"
    );
    assert!(
        dump.contains("SelectExpr"),
        "expected SelectExpr:\n{dump}"
    );
}

// ---------------------------------------------------------------------------
// Adverb on indexed call: f[x]'  →  AdverbExpr(IndexExpr(f, x), ')
// ---------------------------------------------------------------------------

#[test]
fn adverb_on_indexed_call() {
    // f[x]'  →  AdverbExpr(IndexExpr(IdentExpr(f), ArgList(x)), ')
    assert_eq!(
        tree("f[x]'"),
        r#"Root
  ExprStmt
    AdverbExpr
      IndexExpr
        IdentExpr
          Ident "f"
        ArgList
          LBracket "["
          ExprStmt
            IdentExpr
              Ident "x"
          RBracket "]"
      Each "'""#
    );
}
