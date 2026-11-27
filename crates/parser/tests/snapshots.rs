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

/// Number of top-level statements in `src`, which must parse cleanly.
fn count_stmts(src: &str) -> usize {
    tree(src);
    parse(src).syntax().children().count()
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
    // ... whatever the line starts with (checked against q 4.1).
    assert_eq!(count_stmts("b:til 3\n[1]"), 2);
    assert_eq!(count_stmts("c:{x}\n'[f;g]"), 2);
    assert_eq!(count_stmts("d:1\n,[2;3]"), 2);
    assert_eq!(count_stmts("e:1\n -1"), 1);
    assert!(count_stmts("x:1\n/\nblock\nstuff\n\\\ny:2\n") >= 2);
    assert!(
        tree("select a from (select b from t)")
            .matches("SelectExpr")
            .count()
            >= 2
    );
}

/// Non-trivia source text: what the parser treats as code.
fn code(src: &str) -> String {
    let p = parse(src);
    let root = p.syntax();
    let toks = root
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token);
    toks.filter(|t| !t.kind().is_trivia())
        .map(|t| t.text().to_string())
        .collect()
}

#[test]
fn comments_and_terminator() {
    // Each case checked against q 4.1: a column-0 `\` line outside a block
    // comment ends the script; `/`…`\` blocks, indented `\`, and `\ foo` don't.
    let cases: &[(&str, &[&str], &[&str])] = &[
        ("a:1\n\\\n/note\nb:2\n", &["a:1"], &["b:2", "note"]),
        (
            "a:1\n\\\nb:2\n/\nc:3\n\\\nd:4\n",
            &["a:1"],
            &["b:2", "c:3", "d:4"],
        ),
        ("a:1\n\\   \nb:2\n", &["a:1"], &["b:2"]),
        ("a:1\n/\nb:2\n\\\nc:3\n", &["a:1", "c:3"], &["b:2"]),
        ("a:1\n\t\\\nb:2\n", &["a:1", "b:2"], &[]),
        ("a:1\n\\ foo\nb:2\n", &["a:1", "b:2"], &[]),
        // `/` comments only at line start or after whitespace; a block opens
        // and closes only on column-0 `/` and `\\` lines.
        ("f:+/\ng:1", &["f:+/", "g:1"], &[]),
        ("x f/ y", &["xf/y"], &[]),
        ("e:1//c", &["//c"], &[]),
        ("d:\"a /b\"", &["/b"], &[]),
        ("b:1 2 3 / note", &["b:123"], &["note"]),
        ("/note\nx", &["x"], &["note"]),
        (" /\nx", &["x"], &[]),
        ("/\nnote\n \\\nmore\n\\\nx", &["x"], &["note", "more"]),
        ("/\t\nnote\n\\\t\nx", &["x"], &["note"]),
    ];
    for (src, live, dead) in cases {
        let code = code(src);
        for s in *live {
            assert!(
                code.contains(s),
                "{src:?}: {s} should be code, got {code:?}"
            );
        }
        for s in *dead {
            assert!(
                !code.contains(s),
                "{src:?}: {s} should be ignored, got {code:?}"
            );
        }
    }
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
    } // q has no prefix verbs (q 4.1 `parse` rejects each); the error spans
    // the verb term.
    for (src, verb) in PREFIX_VERBS {
        let p = parse(src);
        let spans: Vec<_> = p
            .errors
            .iter()
            .map(|e| &src[e.offset..e.offset + e.len])
            .collect();
        assert_eq!(spans, [*verb], "{src:?}: {:?}", p.errors);
    }
    for src in INVALID_LITERALS {
        let p = parse(src);
        let spans: Vec<_> = p
            .errors
            .iter()
            .map(|e| &src[e.offset..e.offset + e.len])
            .collect();
        assert_eq!(spans, [*src], "{src:?}: {:?}", p.errors);
    }
    // Bracketed, projected, applied or infix verbs are fine.
    for ok in [
        "(+/)x", "+/[x]", "x+/y", "x f/ y", "x f/ 1 2", "x y/ z", "f:+/", "x in y", "x each",
        "neg x", "x - -1", "'[f;g] x", "$[a;b;c]",
    ] {
        assert!(errors(ok).is_empty(), "{ok:?}: {:?}", errors(ok));
    }
}

/// `(source, verb term the prefix-verb error spans)`.
#[rustfmt::skip]
const PREFIX_VERBS: &[(&str, &str)] = &[
    ("-x", "-"), ("+ 1", "+"), ("#x", "#"), ("::x", "::"), ("x - - y", "-"),
    ("x*-y", "-"), ("+/x", "+/"), ("+/1 2 3", "+/"), ("+\\'x", "+\\'"),
    ("f/:\\:x", "f/:\\:"), ("-':x", "-':"), ("f'x", "f'"), ("y/z", "y/"),
    ("set x", "set"), ("setenv 1", "setenv"),
    ("raze select from each t", "each"), ("select +/price from trade", "+/"),
    ("1: \"hello\"", "1:"), ("{[a] +1}", "+"), ("+/ x", "+/"), ("+//x", "+//"),
    ("+/ -1 2 3", "+/"), ("{x}/ 1 2", "{x}/"),
];

/// Number-like words q rejects as literals: each is one error token.
#[rustfmt::skip]
const INVALID_LITERALS: &[&str] = &[
    "1.22b", "1.22h", "1.22i", "1.22j", "1e22i", "1e2h", "12e12.33", "1.1.2.3.4",
    "3b", "2032.12.511", "12:42:62346.123", "32D12:3", "0Wg", "1E5", "12:60",
];

/// `(source, node/token kinds that must appear, kinds that must not)`.
#[rustfmt::skip]
const SHAPES: &[(&str, &[&str], &[&str])] = &[
    ("1+", &["InfixProjection"], &[]),
    ("1+2", &["BinExpr"], &["InfixProjection"]),
    ("a +' b", &["BinExpr", "AdverbExpr"], &[]),
    ("a +/: b", &["BinExpr", "AdverbExpr"], &[]),
    ("x+/", &["InfixProjection", "AdverbExpr"], &["BinExpr"]),
    ("'[f;g]", &["IndexExpr", "VerbExpr"], &[]),
    ("x:'y", &["BinExpr", "AdverbExpr"], &["SignalExpr"]),
    ("'x+1", &["SignalExpr", "BinExpr"], &[]),
    // A verb-valued argument or right operand composes (checked against q).
    ("g f+", &["Composition", "InfixProjection"], &["ApplyExpr"]),
    ("x+y+", &["Composition", "InfixProjection"], &["BinExpr"]),
    ("f g h+", &["Composition"], &["ApplyExpr"]),
    ("x,:+", &["Composition"], &["BinExpr"]),
    ("f x:y+", &["Composition", "BinExpr"], &[]),
    ("f:+/", &["BinExpr"], &["Composition"]),
    ("f::+", &["Composition"], &["BinExpr"]),
    ("g (x+)", &["ApplyExpr"], &["Composition"]),
    ("f[g+]", &["IndexExpr"], &["Composition"]),
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
    ("2 cut x", &["BinExpr"], &["ApplyExpr"]),
    ("f over x", &["BinExpr"], &["ApplyExpr"]),
    ("`t set x", &["BinExpr"], &["ApplyExpr"]),
    ("mmu", &["IdentExpr"], &[]),
    ("select[5] col from t", &["LimitClause"], &[]),
    ("select[5;>price] col from t", &["LimitClause", "OrderClause"], &[]),
    ("select[>price] col from t", &["OrderClause"], &[]),
    ("select a,b from t where c>0", &["WhereClause"], &[]),
    ("select sum a by b from t", &["ByClause"], &[]),
    ("delete from t where a>0", &["DeleteExpr", "FromClause", "WhereClause"], &[]),
    ("update s:sum v by g from t", &["UpdateExpr", "ByClause"], &[]),
    ("t:select a from t", &["SelectExpr", "BinExpr"], &[]),
    ("0!select a from t", &["SelectExpr"], &[]),
    ("select a:x, b:(x,y) from t", &["SelectExpr"], &[]),
    ("select (+/)price from t", &["SelectExpr", "AdverbExpr"], &[]),
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
    // adverb_over_on_builtin: +/  →  AdverbExpr(VerbExpr(+), /)
    ("+/", r#"Root
  ExprStmt
    AdverbExpr
      VerbExpr
        Plus "+"
      Slash "/""#),
    // adverb_scan_on_builtin: +\  →  AdverbExpr(VerbExpr(+), \)
    ("+\\", r#"Root
  ExprStmt
    AdverbExpr
      VerbExpr
        Plus "+"
      Backslash "\\""#),
    // over_applied_to_args: +/[x]  →  IndexExpr(AdverbExpr(VerbExpr(+), /), [x])
    ("+/[x]", r#"Root
  ExprStmt
    IndexExpr
      AdverbExpr
        VerbExpr
          Plus "+"
        Slash "/"
      ArgList
        LBracket "["
        ExprStmt
          IdentExpr
            Ident "x"
        RBracket "]""#),
    // derived_infix: x f'y  →  BinExpr(x, AdverbExpr(f, '), y)
    ("x f'y", r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "x"
      AdverbExpr
        IdentExpr
          Ident "f"
        Each "'"
      IdentExpr
        Ident "y""#),
    // infix_over: 1+/2  →  BinExpr(1, AdverbExpr(VerbExpr(+), /), 2)
    ("1+/2", r#"Root
  ExprStmt
    BinExpr
      LiteralExpr
        Integer "1"
      AdverbExpr
        VerbExpr
          Plus "+"
        Slash "/"
      LiteralExpr
        Integer "2""#),
    // infix_each_right: x,/:y  →  BinExpr(x, AdverbExpr(VerbExpr(,), /:), y)
    ("x,/:y", r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "x"
      AdverbExpr
        VerbExpr
          Comma ","
        EachRight "/:"
      IdentExpr
        Ident "y""#),
    // infix_each_left: x,\:y  →  BinExpr(x, AdverbExpr(VerbExpr(,), \:), y)
    ("x,\\:y", r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "x"
      AdverbExpr
        VerbExpr
          Comma ","
        EachLeft "\\:"
      IdentExpr
        Ident "y""#),
    // infix_each_prior: x-':y  →  BinExpr(x, AdverbExpr(VerbExpr(-), ':), y)
    ("x-':y", r#"Root
  ExprStmt
    BinExpr
      IdentExpr
        Ident "x"
      AdverbExpr
        VerbExpr
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
    // symbol_list: adjacent backtick symbols are one literal
    ("`a`b`c", r#"Root
  ExprStmt
    LiteralExpr
      Symbol "`a`b`c""#),
    // symbols_with_spaces: application, not a list (q: `` `a `b `` is 'type)
    ("`a `b", r#"Root
  ExprStmt
    ApplyExpr
      LiteralExpr
        Symbol "`a"
      LiteralExpr
        Symbol "`b""#),
    // booleans_juxtapose: 0b 1b 0b  →  application, as in q (vectors are `010b`)
    ("0b 1b 0b", r#"Root
  ExprStmt
    ApplyExpr
      LiteralExpr
        Boolean "0b"
      ApplyExpr
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
