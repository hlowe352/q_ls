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
//! - [`grammar`] — Pratt-style grammar rules (right-to-left, equal precedence)
//! - [`parser`] — event-driven parser driver
//! - [`sink`] — converts events into a rowan `GreenNode`
//! - [`syntax_kind`] — all token + node kind discriminants

pub mod event;
pub mod grammar;
pub mod parser;
pub mod syntax_kind;
pub mod sink;

pub use syntax_kind::{QLang, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxElement};
pub use parser::ParseError;
pub use rowan::{TextRange, TextSize};

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
    let (green, errors) = sink::Sink::new(events, errors).finish();
    Parse { green, errors }
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

    /// Underlying rowan [`GreenNode`] (useful for incremental re-parsing).
    #[must_use]
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

    #[test]
    fn parse_select_simple() {
        let parse = parse("select from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), "select from trade");
    }

    #[test]
    fn parse_select_columns() {
        let parse = parse("select price,size from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_select_where() {
        let parse = parse("select price,size from trade where sym=`AAPL");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_select_by() {
        let parse = parse("select avg price by sym from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_update_stmt() {
        let parse = parse("update price:price*1.1 from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_delete_stmt() {
        let parse = parse("delete from trade where price<0");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_exec_stmt() {
        let parse = parse("exec price from trade where sym=`GOOG");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_file_handle_symbol() {
        let parse = parse("read0 `:q1a.txt");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_juxtaposition() {
        // f x — function application by juxtaposition
        let parse = parse("count x");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_double_slash_comment() {
        let parse = parse("// this is a comment\nx:1");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_scan_with_seed() {
        let parse = parse("{x+y}\\[0;1 2 3]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_aoc_q1a() {
        let source = r#"q1a: {[]
    input: read0 `:q1a.txt;
    path: {(x+$["L"=first y;neg;] "J"$1_ y) mod 100}\[50;input];
    sum path=0
  }"#;
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_trailing_semi() {
        // Top-level statement ending with ;
        let parse = parse("a:1;");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_semi_separated_stmts() {
        let parse = parse("a:1;\nb:2;");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_show_call_semi() {
        let parse = parse("show q1a[];");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_assignment_with_semi() {
        let parse = parse("q1a: {[] 1};");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_full_aoc() {
        let source = r#"q1a: {[]
    input: read0 `:q1a.txt;
    path: {(x+$["L"=first y;neg;] "J"$1_ y) mod 100}\[50;input];
    sum path=0
  };

show q1a[];

// Not 6829, less
q1b: {[]
    input: read0 `:q1a.txt;
    path: {loc: x[0]+$[minus: "L"=first y;neg;] "J"$1_ y; (pos;$[(not minus) and 0=x 0;1+;] abs loc div 100;0=pos: loc mod 100)}\[(50;0;0);input];
    sum path[;1]
  };

show q1b[];
"#;
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Comparison operators
    // -----------------------------------------------------------------------

    #[test]
    fn parse_less_equal() {
        let parse = parse("x<=y");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_greater_equal() {
        let parse = parse("x>=y");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_not_equal() {
        let parse = parse("x<>y");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_chained_comparison() {
        // right-to-left: a<b<=c → a < (b <= c)
        let parse = parse("a<b<=c");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Control words
    // -----------------------------------------------------------------------

    #[test]
    fn parse_if_simple() {
        let parse = parse("if[x>0; show x]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_if_three_branch() {
        let parse = parse("if[x>0; show x; show \"negative\"]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_if_with_return() {
        // dbmaint pattern: if[cond; :value]
        let parse = parse("if[tname in key db; :1b]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_if_with_signal() {
        // dbmaint pattern: if[cond; '"error message"]
        let parse = parse("if[not count[p] in 4 5; '\"must pass 4 or 5 params\"]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_do_loop() {
        let parse = parse("do[5; x:x+1]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_while_loop() {
        let parse = parse("while[x<100; x:x*2]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_if_inside_lambda() {
        let parse = parse("{[x] if[x>0; :x; :neg x]}");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_if_not_control_without_bracket() {
        // `if` without `[` is just an identifier
        let parse = parse("if:42");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // File I/O operators
    // -----------------------------------------------------------------------

    #[test]
    fn parse_file_op_0_load_csv() {
        let parse = parse("(\"S\";enlist \",\") 0: `:data.csv");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_file_op_0_save() {
        let parse = parse("`:output.txt 0: (\"hello\";\"world\")");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_file_op_1_stdout() {
        let parse = parse("1: \"hello\"");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_file_op_2_stderr() {
        let parse = parse("2: \"error msg\"");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Functional forms (operator[args])
    // -----------------------------------------------------------------------

    #[test]
    fn parse_amend_at() {
        // @[table;col;:;newval]
        let parse = parse("@[tab;`col;:;newval]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_amend_dot() {
        // .[colNames;where colNames=old;:;new]
        let parse = parse(".[colNames;where colNames=old;:;new]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_amend_at_comma() {
        // dbmaint: @[tdir;`.d;,;cname]
        let parse = parse("@[tdir;`.d;,;cname]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dollar_bracket_conditional() {
        // $[99h ~ type x;enlist;] val
        let parse = parse("$[99h ~ type x;enlist;] val");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Compose with Each ('[ )
    // -----------------------------------------------------------------------

    #[test]
    fn parse_compose_each() {
        // dbmaint: '[ {[p] ...}; enlist ]
        let parse = parse("'[{[p] p};enlist]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Namespaced identifiers (.Q, .z, etc.)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_dotq_dd() {
        let parse = parse(".Q.dd[db;tname]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dotq_en() {
        let parse = parse(".Q.en[db;data]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dot_d_symbol() {
        let parse = parse("get tdir,`.d");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Signal / error
    // -----------------------------------------------------------------------

    #[test]
    fn parse_signal_string() {
        let parse = parse("'\"error message\"");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_signal_concat() {
        // dbmaint: '".d mismatch at: ", 1_string x
        let parse = parse("'\".d mismatch at: \", 1_ string x");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Iterators / adverbs
    // -----------------------------------------------------------------------

    #[test]
    fn parse_each_keyword() {
        let parse = parse("count each x");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_peach() {
        let parse = parse("f peach data");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_over_fold() {
        let parse = parse("+/1 2 3");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_scan_accumulate() {
        let parse = parse("+\\1 2 3");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_each_right() {
        let parse = parse("x,/:y");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_each_left() {
        let parse = parse("x,\\:y");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_each_prior() {
        let parse = parse("-':x");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_scan_converge_bracket() {
        // dbmaint: order verifyReorderCols/: get each paths,\: `.d
        let parse = parse("f/:\\:x");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Projections (partial application)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_projection_plus() {
        let parse = parse("3+");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_projection_in_list() {
        // $[cond;neg;] — projection of identity
        let parse = parse("$[x;neg;]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_projection_bracket() {
        // add1Tab[;db;domain;data;comp] — elided first arg
        let parse = parse("f[;db;data]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Tables
    // -----------------------------------------------------------------------

    #[test]
    fn parse_simple_table() {
        let parse = parse("([] sym:`a`b`c; price:1 2 3)");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_keyed_table() {
        let parse = parse("([sym:`a`b`c] price:1 2 3)");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Global assign (::)
    // -----------------------------------------------------------------------

    #[test]
    fn parse_global_assign() {
        let parse = parse("x::42");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_colon_colon_as_null() {
        // :: as generic null in trap: .[f;args; ::]
        let parse = parse(".[f;args; ::]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Typed params
    // -----------------------------------------------------------------------

    #[test]
    fn parse_typed_params() {
        let parse = parse("{[db:getFSym;tname:`s] db}");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Complex real-world patterns from dbmaint
    // -----------------------------------------------------------------------

    #[test]
    fn parse_dbmaint_check_existence() {
        let source = r"checkTabExistence:{[db:getFSym;tname:`s]
    if[tname in key db; :1b];
    checkTablePathsExist buildTablePaths[db;tname];
    1b
 }";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_dot_d_check() {
        let source = r#"checkDotDEquality:{[db:getFSym;tname:`s]
    if[not `partOrMissing ~ getTableType[db;tname]; 0b];
    d:differ get each .Q.dd[;`.d] each p:buildTablePaths[db;tname];
    if[1< sum d;
        '".d mismatch at: ", 1_string first 1_p where d];
    1b
 }"#;
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_add_tab() {
        // Complex pattern: compose with enlist, multiple if branches
        let source = r#"addTab:('[{[p]
    if[not count[p] in 4 5; '"four or five parameters must be passed to addTab"];
    (db:getFSym;tname:`s;data; tabletype:`s):4#p;
    opt: ([domain:`sym; compparam: 0 0 0i]);
    if[5=count p; opt,:p 4;];
    if[tabletype ~ `flat;
        (.Q.dd[db;tname], $[99h ~ type opt`compparam;enlist;] opt`compparam) set data;
        :()];
    add1Tab[;db;opt `domain;data;"i"$opt`compparam] each $[tabletype~`splayed;
        enlist .Q.dd[db;tname];
        checkTablePathsNotExist buildTablePaths[db;tname]];
 };enlist])"#;
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_amend_dot_d() {
        // @[tdir;`.d;:;.[colNames;where colNames=old;:;new]]
        let parse = parse("@[tdir;`.d;:;.[colNames;where colNames=old;:;new]]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_amend_comma() {
        // @[tdir;`.d;,;cname]
        let parse = parse("@[tdir;`.d;,;cname]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_comma_functional() {
        // ,[;tname]@) each files — comma as functional form
        let parse = parse(",[;tname]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_trap() {
        // .[f;args; ::] — trap with generic null handler
        let parse = parse(".[getValue;enlist path; ::]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_del_tab() {
        let source = r"delTab:{[db:getFSym;tname:`s]
    t:getTableType[db;tname];
    if[t ~ `flat; hdel .Q.dd[db;tname]; :()];
    del1Tab peach $[`splayed ~ t;
        enlist .Q.dd[db;tname];
        checkTablePathsExist buildTablePaths[db;tname]]
  }";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_dbmaint_rename_each() {
        // (rename .) each flip (old_paths; new_paths)
        let parse = parse("(rename .) each flip (oldPaths; newPaths)");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Inline assignment
    // -----------------------------------------------------------------------

    #[test]
    fn parse_inline_assign_in_list() {
        // (db:getFSym;tname:`s;data;tabletype:`s):4#p
        let parse = parse("(db:getFSym;tname:`s;data;tabletype:`s):4#p");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_inline_assign_each() {
        // each p:buildTablePaths[db;tname]
        let parse = parse("f each p:buildTablePaths[db;tname]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Block comments
    // -----------------------------------------------------------------------

    #[test]
    fn parse_block_comment() {
        let source = "/\n  this is a block comment\n  spanning multiple lines\n\\\na:42";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Lossless CST verification
    // -----------------------------------------------------------------------

    #[test]
    fn parse_lossless_complex() {
        let source = "f:{[x;y] if[x>0; :x+y]; neg x}";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn parse_lossless_comparisons() {
        let source = "a<=b>=c<>d";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn parse_lossless_file_ops() {
        let source = "1: \"hello\"";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    // -----------------------------------------------------------------------
    // Compound assignment operators
    // -----------------------------------------------------------------------

    #[test]
    fn parse_compound_assign_plus() {
        let parse = parse("x+:1");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_compound_assign_minus() {
        let parse = parse("x-:1");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_compound_assign_comma() {
        // append in place: x,:y
        let parse = parse("x,:y");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_compound_assign_in_amend() {
        // @[tab;col;+:;val]
        let parse = parse("@[tab;col;+:;val]");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_compound_assign_various() {
        for op in ["*:", "%:", ">:", "<:", "~:", "=:", "_:", "#:", "$:", "!:", "|:", "&:", "?:", "^:", "@:"] {
            let source = format!("x{op}1");
            let parse = parse(&source);
            assert!(parse.errors.is_empty(), "errors for {op}: {:?}", parse.errors);
        }
    }

    #[test]
    fn parse_dbmaint_opt_comma_assign() {
        // dbmaint: opt,:p 4
        let parse = parse("opt,:p 4");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Temporal null/inf types
    // -----------------------------------------------------------------------

    #[test]
    fn parse_month_null() {
        let parse = parse("0Nm");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_guid_null() {
        let parse = parse("0Ng");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_timespan_null() {
        let parse = parse("0Nn");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_minute_null() {
        let parse = parse("0Nu");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_second_null() {
        let parse = parse("0Nv");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_datetime_null() {
        let parse = parse("0Nz");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_timestamp_null() {
        let parse = parse("0Np");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_temporal_infs() {
        for suffix in ["g", "m", "n", "p", "u", "v", "z"] {
            let source = format!("0W{suffix}");
            let parse = parse(&source);
            assert!(parse.errors.is_empty(), "errors for 0W{suffix}: {:?}", parse.errors);
        }
    }

    // -----------------------------------------------------------------------
    // qSQL improvements
    // -----------------------------------------------------------------------

    #[test]
    fn parse_select_limit() {
        let parse = parse("select[5] from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_select_limit_sort() {
        let parse = parse("select[5;>price] from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_select_distinct() {
        let parse = parse("select distinct sym from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_select_distinct_columns() {
        let parse = parse("select distinct sym,price from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_exec_distinct() {
        let parse = parse("exec distinct sym from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    // -----------------------------------------------------------------------
    // Shebang
    // -----------------------------------------------------------------------

    #[test]
    fn parse_shebang() {
        let parse = parse("#!/usr/bin/env q\nx:42");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_shebang_lossless() {
        let source = "#!/usr/bin/env q\nx:42";
        let parse = parse(source);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    // -----------------------------------------------------------------------
    // Comment blocks
    // -----------------------------------------------------------------------

    #[test]
    fn parse_with_comment_block_between_stmts() {
        let src = "x:1\n/\nblock\nstuff\n\\\ny:2\n";
        let parse = parse(src);
        let dump = format!("{:#?}", parse.syntax());
        let count = dump.matches("ExprStmt").count();
        assert!(count >= 2, "expected ≥2 stmts, got:\n{dump}");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    /// Regression: `p)` mid-expression must NOT be lexed as a DSL escape line.
    /// The DSL prefix only applies when `k)` / `p)` begin a line.
    #[test]
    fn parse_p_paren_mid_expression_is_not_dsl() {
        // (3#p),fn  — `p)` here closes the `(3#p)` group, not a DSL escape
        let parse = parse("(3#p),fn");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        let dump = format!("{:#?}", parse.syntax());
        assert!(!dump.contains("DslLine"), "should not lex as DslLine:\n{dump}");
        assert!(!dump.contains("DslStmt"), "should not parse as DslStmt:\n{dump}");
    }

    /// Regression: `k)` and `p)` at line start ARE the DSL escape.
    #[test]
    fn parse_dsl_at_line_start_still_works() {
        let parse = parse("k)1+2");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        let dump = format!("{:#?}", parse.syntax());
        assert!(dump.contains("DslStmt"), "expected DslStmt:\n{dump}");
    }

    /// Regression: q allows line continuation when the next line starts with
    /// whitespace. The newline alone is not a statement boundary.
    #[test]
    fn parse_indented_continuation() {
        let src = "f x\n    +y";
        let parse = parse(src);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        let dump = format!("{:#?}", parse.syntax());
        // The whole thing should be a single statement (one ExprStmt).
        let count = dump.matches("ExprStmt").count();
        assert_eq!(count, 1, "expected 1 stmt (continuation), got {count}:\n{dump}");
    }

    /// Regression: a non-indented next line IS a new statement boundary.
    #[test]
    fn parse_unindented_is_new_stmt() {
        let src = "f x\ng y";
        let parse = parse(src);
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        let dump = format!("{:#?}", parse.syntax());
        let count = dump.matches("ExprStmt").count();
        assert_eq!(count, 2, "expected 2 stmts, got {count}:\n{dump}");
    }
}
