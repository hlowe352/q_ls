//! CST shape tests. `SNAPSHOTS` pins the *exact* tree (non-trivia tokens
//! only) so grammar changes cannot silently alter adverb/vector structure;
//! `SHAPES` checks for the presence/absence of node kinds.

use q_parser::{SyntaxNode, parse};
use rowan::NodeOrToken;
use std::fmt::Write as _;

fn dump(node: &SyntaxNode) -> String {
    fn go(out: &mut String, node: &SyntaxNode, depth: usize) {
        writeln!(out, "{}{:?}", "  ".repeat(depth), node.kind()).unwrap();
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => go(out, &n, depth + 1),
                NodeOrToken::Token(t) if !t.kind().is_trivia() => {
                    let indent = "  ".repeat(depth + 1);
                    writeln!(out, "{indent}{:?} {:?}", t.kind(), t.text()).unwrap();
                }
                NodeOrToken::Token(_) => {}
            }
        }
    }
    let mut out = String::new();
    go(&mut out, node, 0);
    out.trim().to_string()
}

/// Parse `src`, asserting no parse errors, and return its tree dump.
fn tree(src: &str) -> String {
    let p = parse(src);
    assert!(p.errors.is_empty(), "errors for {src:?}: {:?}", p.errors);
    dump(&p.syntax())
}

fn count_stmts(src: &str) -> usize {
    tree(src).matches("ExprStmt").count()
}

#[test]
fn snapshots() {
    for (src, expected) in SNAPSHOTS {
        assert_eq!(tree(src), *expected, "snapshot mismatch for {src:?}");
    }
}

#[test]
fn shapes() {
    for (src, present, absent) in SHAPES {
        let dump = tree(src);
        for kind in *present {
            assert!(dump.contains(kind), "{src:?}: expected {kind}:\n{dump}");
        }
        for kind in *absent {
            assert!(!dump.contains(kind), "{src:?}: unexpected {kind}:\n{dump}");
        }
    }
}

#[test]
fn statement_boundaries() {
    // Indented next line continues the statement; column 0 starts a new one.
    assert_eq!(count_stmts("f x\n    +y"), 1);
    assert_eq!(count_stmts("f x\ng y"), 2);
    assert!(count_stmts("x:1\n/\nblock\nstuff\n\\\ny:2\n") >= 2);
    assert!(
        tree("select a from (select b from t)")
            .matches("SelectExpr")
            .count()
            >= 2
    );
}

#[test]
fn qsql_clause_lists() {
    use q_parser::SyntaxKind;
    let find = |src: &str, kind| {
        let p = parse(src);
        assert!(p.errors.is_empty(), "errors for {src:?}: {:?}", p.errors);
        p.syntax().descendants().find(|n| n.kind() == kind).unwrap()
    };
    // `from` must end the column list, not be juxtaposed into it.
    for src in ["select a from t", "update a:1 from t"] {
        let cols = find(src, SyntaxKind::ColumnList).text().to_string();
        assert!(
            !cols.contains("from"),
            "{src:?}: `from` leaked into {cols:?}"
        );
    }
    // Commas separate columns / conditions rather than acting as join.
    let cols = find("select a:b+c,d:e*f from t", SyntaxKind::ColumnList);
    assert_eq!(cols.children().count(), 2, "{cols:#?}");
    let conds = find("select a from t where d>0,e<1", SyntaxKind::WhereClause);
    assert_eq!(conds.children().count(), 2, "{conds:#?}");
}

#[test]
fn diagnostics() {
    let errors =
        |src: &str| -> Vec<String> { parse(src).errors.into_iter().map(|e| e.message).collect() };
    // `"a" 32` must not loop forever trying to build a VectorExpr; errors fine.
    let _ = parse(r#"f["a" 32 "b"]"#);
    // System commands are only valid at line start.
    let msgs = errors("x \\l file.q");
    assert!(
        msgs.iter()
            .any(|m| m.contains("scan adverb") || m.contains("start of a line")),
        "expected misplaced \\l diagnostic, got: {msgs:?}"
    );
    for ok in ["f\\", "\\l file.q", "x:1\n\\l file.q"] {
        assert!(errors(ok).is_empty(), "{ok:?}: {:?}", errors(ok));
    }
}

/// `(source, node/token kinds that must appear, kinds that must not)`.
#[rustfmt::skip]
const SHAPES: &[(&str, &[&str], &[&str])] = &[
    ("1+", &["InfixProjection"], &[]),
    ("1+2", &["BinExpr"], &["InfixProjection"]),
    ("a +' b", &["InfixModExpr"], &[]),
    ("a +/: b", &["InfixModExpr"], &[]),
    ("'[f;g]", &["Composition"], &[]),
    ("'`err", &["SignalExpr"], &[]),
    ("0Nm", &["Month"], &[]),
    ("0Ng", &["Guid"], &[]),
    ("0Nn", &["Timespan"], &[]),
    ("12:30", &["Minute"], &[]),
    ("12:30:45", &["Second"], &[]),
    ("0Nz", &["Datetime"], &[]),
    ("0xABCD", &["ByteList"], &[]),
    ("`:foo.csv", &["FileSymbolExpr"], &["LiteralExpr"]),
    ("([k:1 2] v:3 4)", &["TableExpr", "TableKeys"], &[]),
    ("([] v:3 4)", &["TableExpr"], &["TableKeys"]),
    (".q", &["Namespace"], &[]),
    (".q.func", &[], &["Namespace"]),
    ("{:42}", &["ReturnExpr"], &[]),
    ("x mmu y", &["BinExpr"], &[]),
    ("a in b", &["BinExpr"], &[]),
    ("1 within 2", &["BinExpr"], &[]),
    ("t lj u", &["BinExpr"], &[]),
    ("mmu", &["IdentExpr"], &[]),
    ("select[5] col from t", &["LimitClause"], &[]),
    ("select[5;>price] col from t", &["LimitClause", "OrderClause"], &[]),
    ("select[>price] col from t", &["OrderClause"], &[]),
    ("select a,b from t where c>0", &["WhereClause"], &[]),
    ("select sum a by b from t", &["ByClause"], &[]),
    ("delete from t where a>0", &["DeleteExpr", "WhereClause"], &[]),
    ("t:select a from t", &["SelectExpr", "BinExpr"], &[]),
    ("0!select a from t", &["SelectExpr"], &[]),
    ("select a:x, b:(x,y) from t", &["SelectExpr"], &[]),
    ("select +/price from t", &["SelectExpr", "AdverbExpr"], &[]),
    ("k)1+2", &["DslStmt"], &[]),
    ("p)select * from t", &["DslStmt"], &[]),
    // `p)` mid-expression closes a paren; it is not a DSL escape.
    ("(3#p),fn", &[], &["DslLine", "DslStmt"]),
    // Space on both sides, or none before: subtraction, not a negative literal.
    ("1 2 3 - 23", &["BinExpr"], &["Integer \"-"]),
    ("3-1", &["BinExpr"], &[]),
];

#[rustfmt::skip]
const SNAPSHOTS: &[(&str, &str)] = &[
    // adverb_over_on_ident: f/  →  AdverbExpr(IdentExpr(f), /)
    ("f/", r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      Slash "/""#),
    // adverb_scan_on_ident: f\  →  AdverbExpr(IdentExpr(f), \)
    ("f\\", r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      Backslash "\\""#),
    // adverb_each_on_ident: f'  →  AdverbExpr(IdentExpr(f), ')
    ("f'", r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      Each "'""#),
    // adverb_each_prior_on_ident: f':  →  AdverbExpr(IdentExpr(f), ':)
    ("f':", r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      EachPrior "':""#),
    // adverb_each_right_on_ident: f/:  →  AdverbExpr(IdentExpr(f), /:)
    ("f/:", r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      EachRight "/:""#),
    // adverb_each_left_on_ident: f\:  →  AdverbExpr(IdentExpr(f), \:)
    ("f\\:", r#"Root
  ExprStmt
    AdverbExpr
      IdentExpr
        Ident "f"
      EachLeft "\\:""#),
    // adverb_over_on_builtin: +/  →  AdverbExpr(UnaryExpr(+), /)
    ("+/", r#"Root
  ExprStmt
    AdverbExpr
      UnaryExpr
        Plus "+"
      Slash "/""#),
    // adverb_scan_on_builtin: +\  →  AdverbExpr(UnaryExpr(+), \)
    ("+\\", r#"Root
  ExprStmt
    AdverbExpr
      UnaryExpr
        Plus "+"
      Backslash "\\""#),
    // over_applied_to_list: +/x  →  ApplyExpr(AdverbExpr(UnaryExpr(+), /), IdentExpr(x))
    ("+/x", r#"Root
  ExprStmt
    ApplyExpr
      AdverbExpr
        UnaryExpr
          Plus "+"
        Slash "/"
      IdentExpr
        Ident "x""#),
    // each_applied_to_function_and_arg: count'x  →  ApplyExpr(AdverbExpr(IdentExpr(count), '), IdentExpr(x))
    ("count'x", r#"Root
  ExprStmt
    ApplyExpr
      AdverbExpr
        IdentExpr
          Ident "count"
        Each "'"
      IdentExpr
        Ident "x""#),
    // infix_mod_over: 1+/2  →  InfixModExpr(1, +, /, 2)
    ("1+/2", r#"Root
  ExprStmt
    InfixModExpr
      LiteralExpr
        Integer "1"
      Plus "+"
      Slash "/"
      LiteralExpr
        Integer "2""#),
    // infix_mod_each_right: x,/:y  →  InfixModExpr(x, ,, /:, y)
    ("x,/:y", r#"Root
  ExprStmt
    InfixModExpr
      IdentExpr
        Ident "x"
      Comma ","
      EachRight "/:"
      IdentExpr
        Ident "y""#),
    // infix_mod_each_left: x,\:y  →  InfixModExpr(x, ,, \:, y)
    ("x,\\:y", r#"Root
  ExprStmt
    InfixModExpr
      IdentExpr
        Ident "x"
      Comma ","
      EachLeft "\\:"
      IdentExpr
        Ident "y""#),
    // infix_mod_each_prior: x-':y  →  InfixModExpr(x, -, ':, y)
    ("x-':y", r#"Root
  ExprStmt
    InfixModExpr
      IdentExpr
        Ident "x"
      Minus "-"
      EachPrior "':"
      IdentExpr
        Ident "y""#),
    // chained_adverbs_over_each_left: f/\:  →  AdverbExpr(AdverbExpr(IdentExpr(f), /), \:)
    ("f/\\:", r#"Root
  ExprStmt
    AdverbExpr
      AdverbExpr
        IdentExpr
          Ident "f"
        Slash "/"
      EachLeft "\\:""#),
    // seeded_over_bracket: f/[10;x]  →  IndexExpr(AdverbExpr(IdentExpr(f), /), ArgList)
    ("f/[10;x]", r#"Root
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
        RBracket "]""#),
    // seeded_scan_bracket: f\[10;x]  →  IndexExpr(AdverbExpr(IdentExpr(f), \), ArgList)
    ("f\\[10;x]", r#"Root
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
        RBracket "]""#),
    // each_keyword_form: count each x  →  BinExpr(IdentExpr(count), Ident(each), IdentExpr(x))
    ("count each x", r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "count"
      Ident "each"
      IdentExpr
        Ident "x""#),
    // peach_keyword_form: f peach x  →  BinExpr(IdentExpr(f), Ident(peach), IdentExpr(x))
    ("f peach x", r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "f"
      Ident "peach"
      IdentExpr
        Ident "x""#),
    // adverb_on_indexed_call: f[x]'  →  AdverbExpr(IndexExpr(IdentExpr(f), ArgList(x)), ')
    ("f[x]'", r#"Root
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
      Each "'""#),
    // integer_vector
    ("1 2 3", r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Integer "1"
      LiteralExpr
        Integer "2"
      LiteralExpr
        Integer "3""#),
    // float_vector
    ("1.0 2.0 3.0", r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Float "1.0"
      LiteralExpr
        Float "2.0"
      LiteralExpr
        Float "3.0""#),
    // symbol_vector
    ("`a `b `c", r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Symbol "`a"
      LiteralExpr
        Symbol "`b"
      LiteralExpr
        Symbol "`c""#),
    // boolean_vector
    ("0b 1b 0b", r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Boolean "0b"
      LiteralExpr
        Boolean "1b"
      LiteralExpr
        Boolean "0b""#),
    // vector_in_assignment: t: 1 2 3 — the user's reported case
    ("t: 1 2 3", r#"Root
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
          Integer "3""#),
    // func_applied_to_vector: f 1 2 3 — f applied to the vector 1 2 3
    ("f 1 2 3", r#"Root
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
          Integer "3""#),
    // vector_then_binop: 1 2 3 + 1 — vector addition
    ("1 2 3 + 1", r#"Root
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
        Integer "1""#),
    // vector_indexed: 1 2 3 4 f — vector indexed/applied with f
    ("1 2 3 4 f", r#"Root
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
        Ident "f""#),
    // vector_with_negative_elements: 1 2 3 -23 — space before minus, no space after → -23 is a negative literal
    ("1 2 3 -23", r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Integer "1"
      LiteralExpr
        Integer "2"
      LiteralExpr
        Integer "3"
      LiteralExpr
        Integer "-23""#),
    // vector_negative_floats
    ("1.0 -2.5 3.0", r#"Root
  ExprStmt
    VectorExpr
      LiteralExpr
        Float "1.0"
      LiteralExpr
        Float "-2.5"
      LiteralExpr
        Float "3.0""#),
    // negative_literal_after_paren: (-23) — after LParen → merge into single Integer "-23" token
    ("(-23)", r#"Root
  ExprStmt
    ParenExpr
      LParen "("
      ExprStmt
        LiteralExpr
          Integer "-23"
      RParen ")""#),
    // func_applied_to_single_arg_not_vector: f x — still ApplyExpr, not VectorExpr
    ("f x", r#"Root
  ExprStmt
    ApplyExpr
      IdentExpr
        Ident "f"
      IdentExpr
        Ident "x""#),
];
