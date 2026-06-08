//! Broad parser exploration: run many q expressions, report unexpected errors.
//! Usage: cargo run -p q-parser --example explore_broad

use q_parser::{SyntaxKind, SyntaxNode, parse};

fn has_error_node(node: &SyntaxNode) -> bool {
    node.descendants_with_tokens().any(|e| e.kind() == SyntaxKind::Error)
}

fn check(label: &str, src: &str, expect_errors: bool) -> bool {
    let p = parse(src);
    let has_perr = !p.errors.is_empty();
    let has_enode = has_error_node(&p.syntax());
    let bad = if expect_errors {
        !has_perr && !has_enode // expected errors but got none — might be ok, just note
    } else {
        has_perr || has_enode
    };
    if bad || (expect_errors && !has_perr && !has_enode) {
        if !expect_errors && (has_perr || has_enode) {
            println!("FAIL [{label}] {src:?}");
            for e in &p.errors {
                println!("  ParseError: {:?}", e.message);
            }
            if has_enode {
                println!("  (has Error node in tree)");
            }
        } else if expect_errors && !has_perr && !has_enode {
            println!("NOTE [{label}] expected errors but parsed clean: {src:?}");
        }
        return false;
    }
    true
}

macro_rules! ok {
    ($cat:literal, $src:literal) => {
        check($cat, $src, false)
    };
}

fn main() {
    let mut pass = 0usize;
    let mut fail = 0usize;

    macro_rules! run {
        (ok $cat:literal $src:literal) => {{
            if ok!($cat, $src) { pass += 1; } else { fail += 1; }
        }};
    }

    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------
    run!(ok "literal" "42");
    run!(ok "literal" "42i");
    run!(ok "literal" "42j");
    run!(ok "literal" "42h");
    run!(ok "literal" "3.14");
    run!(ok "literal" "3.14e");
    run!(ok "literal" "3.14f");
    run!(ok "literal" "1e10");
    run!(ok "literal" "1.5e-3");
    run!(ok "literal" "0b");
    run!(ok "literal" "1b");
    run!(ok "literal" "01b");
    run!(ok "literal" "010101b");
    run!(ok "literal" "0x0F");
    run!(ok "literal" "0xDEADBEEF");
    run!(ok "literal" "\"hello\"");
    run!(ok "literal" "\"\"");
    run!(ok "literal" "\"a\"");
    run!(ok "literal" "`sym");
    run!(ok "literal" "`:file/path");
    run!(ok "literal" "2000.01.01");
    run!(ok "literal" "2024.06.04");
    run!(ok "literal" "2000.01m");
    run!(ok "literal" "12:00:00");
    run!(ok "literal" "12:00:00.000");
    run!(ok "literal" "12:00");
    run!(ok "literal" "0Ng");
    run!(ok "literal" "0Ni");
    run!(ok "literal" "0Nj");
    run!(ok "literal" "0Nh");
    run!(ok "literal" "0Nn");
    run!(ok "literal" "0Np");
    run!(ok "literal" "0Nm");
    run!(ok "literal" "0Nu");
    run!(ok "literal" "0Nv");
    run!(ok "literal" "0Nz");
    run!(ok "literal" "0Wi");
    run!(ok "literal" "0Wj");
    run!(ok "literal" "-0Wi");
    run!(ok "literal" "-0Wj");
    run!(ok "literal" "0Wf");
    run!(ok "literal" "0n");
    run!(ok "literal" "0w");
    run!(ok "literal" "0N");
    run!(ok "literal" "0W");

    // -----------------------------------------------------------------------
    // Symbol / symbol list
    // -----------------------------------------------------------------------
    run!(ok "symbol" "`a`b`c");
    run!(ok "symbol" "``");
    run!(ok "symbol" "` `a");
    run!(ok "symbol" "`a`b`c!1 2 3");

    // -----------------------------------------------------------------------
    // Typed lists
    // -----------------------------------------------------------------------
    run!(ok "typed-list" "1 2 3");
    run!(ok "typed-list" "1 2 3h");
    run!(ok "typed-list" "1 2 3i");
    run!(ok "typed-list" "1 2 3j");
    run!(ok "typed-list" "1 2 3f");
    run!(ok "typed-list" "1 2 3e");
    run!(ok "typed-list" "1.0 2.0 3.0");
    run!(ok "typed-list" "1 2 3 4 5");

    // -----------------------------------------------------------------------
    // Negative numbers / minus disambiguation
    // -----------------------------------------------------------------------
    run!(ok "minus" "-1");
    run!(ok "minus" "-1.0");
    run!(ok "minus" "-1 2 3");
    run!(ok "minus" "x-1");
    run!(ok "minus" "x - 1");
    run!(ok "minus" "x - -1");
    run!(ok "minus" "-0W");
    run!(ok "minus" "neg x");
    run!(ok "minus" "-x");
    run!(ok "minus" "1-2-3");
    run!(ok "minus" "(-1)");
    run!(ok "minus" "x-y-z");

    // -----------------------------------------------------------------------
    // Arithmetic / operators
    // -----------------------------------------------------------------------
    run!(ok "arith" "1+2");
    run!(ok "arith" "1+2+3");
    run!(ok "arith" "2*3+4");
    run!(ok "arith" "a%b%c");
    run!(ok "arith" "x&y");
    run!(ok "arith" "x|y");
    run!(ok "arith" "x!y");
    run!(ok "arith" "x#y");
    run!(ok "arith" "x_y");
    run!(ok "arith" "x~y");
    run!(ok "arith" "x^y");
    run!(ok "arith" "x,y");
    run!(ok "arith" "x=y");
    run!(ok "arith" "x<y");
    run!(ok "arith" "x>y");
    run!(ok "arith" "x<>y");
    run!(ok "arith" "x<=y");
    run!(ok "arith" "x>=y");

    // -----------------------------------------------------------------------
    // Unary operators
    // -----------------------------------------------------------------------
    run!(ok "unary" "not x");
    run!(ok "unary" "neg x");
    run!(ok "unary" "count x");
    run!(ok "unary" "flip x");
    run!(ok "unary" "type x");
    run!(ok "unary" "enlist x");
    run!(ok "unary" "string x");
    run!(ok "unary" "raze x");
    run!(ok "unary" "distinct x");
    run!(ok "unary" "reverse x");
    run!(ok "unary" "asc x");
    run!(ok "unary" "desc x");
    run!(ok "unary" "iasc x");
    run!(ok "unary" "idesc x");
    run!(ok "unary" "sum x");
    run!(ok "unary" "avg x");
    run!(ok "unary" "min x");
    run!(ok "unary" "max x");
    run!(ok "unary" "sdev x");
    run!(ok "unary" "var x");
    run!(ok "unary" "prd x");
    run!(ok "unary" "first x");
    run!(ok "unary" "last x");
    run!(ok "unary" "abs x");
    run!(ok "unary" "ceiling x");
    run!(ok "unary" "floor x");
    run!(ok "unary" "sqrt x");
    run!(ok "unary" "exp x");
    run!(ok "unary" "log x");
    run!(ok "unary" "signum x");
    run!(ok "unary" "reciprocal x");
    run!(ok "unary" "hopen x");
    run!(ok "unary" "hclose x");
    run!(ok "unary" "read0 x");
    run!(ok "unary" "read1 x");
    run!(ok "unary" "get x");
    run!(ok "unary" "set x");
    run!(ok "unary" "key x");
    run!(ok "unary" "value x");
    run!(ok "unary" "parse x");
    run!(ok "unary" "eval x");
    run!(ok "unary" "load x");
    run!(ok "unary" "tables x");
    run!(ok "unary" "views x");
    run!(ok "unary" "cols x");
    run!(ok "unary" "meta x");
    run!(ok "unary" "hdel x");
    run!(ok "unary" "hsym x");

    // -----------------------------------------------------------------------
    // Cast / parse
    // -----------------------------------------------------------------------
    run!(ok "cast" "`int$x");
    run!(ok "cast" "`float$x");
    run!(ok "cast" "`sym$x");
    run!(ok "cast" "`$x");
    run!(ok "cast" "`long$1 2 3");
    run!(ok "cast" "\"I\"$\"123\"");
    run!(ok "cast" "\"J\"$x");
    run!(ok "cast" "10h$x");

    // -----------------------------------------------------------------------
    // Assignment
    // -----------------------------------------------------------------------
    run!(ok "assign" "a:1");
    run!(ok "assign" "a::1");
    run!(ok "assign" ".ns.a:1");
    run!(ok "assign" "a:b:c:1");
    run!(ok "assign" "a+:1");
    run!(ok "assign" "a-:1");
    run!(ok "assign" "a*:1");
    run!(ok "assign" "a%:1");
    run!(ok "assign" "a,:1 2");
    run!(ok "assign" "a|:x");
    run!(ok "assign" "a&:x");
    run!(ok "assign" "a[0]:1");
    run!(ok "assign" "a[0;1]:1");
    run!(ok "assign" "(a;b;c):1 2 3");
    run!(ok "assign" "(a;b):x");

    // -----------------------------------------------------------------------
    // Function application
    // -----------------------------------------------------------------------
    run!(ok "apply" "f x");
    run!(ok "apply" "f[x]");
    run!(ok "apply" "f[x;y]");
    run!(ok "apply" "f[x;y;z]");
    run!(ok "apply" "f[]");
    run!(ok "apply" "f[;x]");
    run!(ok "apply" "f[x;]");
    run!(ok "apply" "f[;;]");
    run!(ok "apply" "(f) x");
    run!(ok "apply" "f . x");
    run!(ok "apply" "f . (1;2;3)");
    run!(ok "apply" "f @ x");
    run!(ok "apply" "(+) . (1;2)");

    // -----------------------------------------------------------------------
    // Lambda
    // -----------------------------------------------------------------------
    run!(ok "lambda" "{x}");
    run!(ok "lambda" "{x+y}");
    run!(ok "lambda" "{[x] x+1}");
    run!(ok "lambda" "{[x;y] x+y}");
    run!(ok "lambda" "{[] 42}");
    run!(ok "lambda" "{x;y}");
    run!(ok "lambda" "{a:1; b:2; a+b}");
    run!(ok "lambda" "{:x}");
    run!(ok "lambda" "{[x] :x}");
    run!(ok "lambda" "{{x}x}");
    run!(ok "lambda" "{[x] {[y] x+y}}");
    run!(ok "lambda" "{x+y}[1;2]");
    run!(ok "lambda" "{x*x} each 1 2 3");
    run!(ok "lambda" "{x+y}/[0;1 2 3]");
    run!(ok "lambda" "{x+y}\\[0;1 2 3]");

    // -----------------------------------------------------------------------
    // Adverbs / iterators
    // -----------------------------------------------------------------------
    run!(ok "adverb" "f/");
    run!(ok "adverb" "f\\");
    run!(ok "adverb" "f'");
    run!(ok "adverb" "f':");
    run!(ok "adverb" "f/:");
    run!(ok "adverb" "f\\:");
    run!(ok "adverb" "+/x");
    run!(ok "adverb" "+\\x");
    run!(ok "adverb" "f/x");
    run!(ok "adverb" "f\\x");
    run!(ok "adverb" "f'x");
    run!(ok "adverb" "1+/2");
    run!(ok "adverb" "1,/:2");
    run!(ok "adverb" "1-':2");
    run!(ok "adverb" "+/1 2 3");
    run!(ok "adverb" "+\\1 2 3");
    run!(ok "adverb" "f/[10;x]");
    run!(ok "adverb" "f\\[10;x]");
    run!(ok "adverb" "f/\\:");
    run!(ok "adverb" "f[x]'");
    run!(ok "adverb" "(f')x");
    run!(ok "adverb" "(+/)x");
    run!(ok "adverb" "count each x");
    run!(ok "adverb" "f peach x");
    run!(ok "adverb" "{x+1} each 1 2 3");
    run!(ok "adverb" "f/:\\:x");
    run!(ok "adverb" "(+')f");
    run!(ok "adverb" "+//x");        // over-over: double scan converge
    run!(ok "adverb" "+\\'x");       // scan-each
    run!(ok "adverb" "-':x");        // each-prior
    run!(ok "adverb" "f'[x;y]");     // each-bracket form
    run!(ok "adverb" "0+/x");        // seeded over without bracket
    run!(ok "adverb" "0 +/ x");      // seeded over with spaces
    run!(ok "adverb" "+/ x");        // over with space

    // -----------------------------------------------------------------------
    // Conditional / cond
    // -----------------------------------------------------------------------
    run!(ok "cond" "$[x;y;z]");
    run!(ok "cond" "$[a;b;c;d;e]");
    run!(ok "cond" "$[x;$[y;a;b];c]");
    run!(ok "cond" "$[x>0;x;neg x]");
    run!(ok "cond" "$[x;y]");          // 2-arg: error or works?

    // -----------------------------------------------------------------------
    // Signal / error / trap
    // -----------------------------------------------------------------------
    run!(ok "signal" "'x");
    run!(ok "signal" "'\"`type\"");
    run!(ok "signal" "'\"error message\"");
    run!(ok "signal" ".[f;args;::]");
    run!(ok "signal" "@[f;x;{x}]");
    run!(ok "signal" ".[f;(x;y);{[e] e}]");

    // -----------------------------------------------------------------------
    // Control words
    // -----------------------------------------------------------------------
    run!(ok "ctrl" "if[x>0; show x]");
    run!(ok "ctrl" "if[x>0; show x; show y]");
    run!(ok "ctrl" "if[x; :1b]");
    run!(ok "ctrl" "do[5; x+:1]");
    run!(ok "ctrl" "while[x<100; x*:2]");
    run!(ok "ctrl" "if[x; '\"err\"]");

    // -----------------------------------------------------------------------
    // List / paren forms
    // -----------------------------------------------------------------------
    run!(ok "list" "(1;2;3)");
    run!(ok "list" "()");
    run!(ok "list" "(1)");
    run!(ok "list" "(1 2 3)");
    run!(ok "list" "(1;2 3;4)");
    run!(ok "list" "(a;b;c)");
    run!(ok "list" "enlist 1");
    run!(ok "list" "enlist x");

    // -----------------------------------------------------------------------
    // Dictionary
    // -----------------------------------------------------------------------
    run!(ok "dict" "`a`b`c!1 2 3");
    run!(ok "dict" "1 2!`a`b");
    run!(ok "dict" "(1;2)!(3;4)");
    run!(ok "dict" "([]a:1 2;b:3 4)!x");

    // -----------------------------------------------------------------------
    // Table
    // -----------------------------------------------------------------------
    run!(ok "table" "([] sym:`a`b`c; price:1 2 3)");
    run!(ok "table" "([sym:`a`b`c] price:1 2 3)");
    run!(ok "table" "([] a:1 2 3)");
    run!(ok "table" "([] )");            // empty table — might fail?
    run!(ok "table" "flip `a`b`c!(1 2;3 4;5 6)");

    // -----------------------------------------------------------------------
    // qSQL
    // -----------------------------------------------------------------------
    run!(ok "qsql" "select from trade");
    run!(ok "qsql" "select sym from trade");
    run!(ok "qsql" "select sym,price from trade");
    run!(ok "qsql" "select sym:sym,p:price from trade");
    run!(ok "qsql" "select sum price from trade");
    run!(ok "qsql" "select sum price by sym from trade");
    run!(ok "qsql" "select avg price,sum size by sym from trade");
    run!(ok "qsql" "select from trade where sym=`AAPL");
    run!(ok "qsql" "select from trade where sym=`AAPL,price>100");
    run!(ok "qsql" "select from trade where sym in `AAPL`GOOG");
    run!(ok "qsql" "select from trade where not sym=`AAPL");
    run!(ok "qsql" "select from trade where price within 100 200");
    run!(ok "qsql" "select[5] from trade");
    run!(ok "qsql" "select[5;>price] from trade");
    run!(ok "qsql" "select[<price] from trade");
    run!(ok "qsql" "select distinct sym from trade");
    run!(ok "qsql" "exec sym from trade");
    run!(ok "qsql" "exec distinct sym from trade");
    run!(ok "qsql" "update price:price*1.1 from trade");
    run!(ok "qsql" "update price:price*1.1 from trade where sym=`AAPL");
    run!(ok "qsql" "delete from trade");
    run!(ok "qsql" "delete from trade where sym=`AAPL");
    run!(ok "qsql" "delete sym from trade");
    run!(ok "qsql" "select count i from trade");
    run!(ok "qsql" "select count i by sym from trade");
    run!(ok "qsql" "select from (select from trade)");
    run!(ok "qsql" "select from trade lj (select sym,name from ref)");
    run!(ok "qsql" "select +/price from trade");         // adverb in column
    run!(ok "qsql" "select {x*y}[price;size] from trade"); // lambda in column
    run!(ok "qsql" "select avg price by date.month from trade"); // date part

    // -----------------------------------------------------------------------
    // Joins
    // -----------------------------------------------------------------------
    run!(ok "join" "t lj u");
    run!(ok "join" "t ij u");
    run!(ok "join" "t uj u");
    run!(ok "join" "t pj u");
    run!(ok "join" "t asof u");
    run!(ok "join" "t aj[`sym`time;u]");  // as-of join
    run!(ok "join" "t wj[w;`sym;u]");     // window join

    // -----------------------------------------------------------------------
    // File I/O operators
    // -----------------------------------------------------------------------
    run!(ok "fileio" "1: \"hello\"");
    run!(ok "fileio" "2: \"error\"");
    run!(ok "fileio" "(\"S\";enlist \",\") 0: `:data.csv");
    run!(ok "fileio" "`:output.txt 0: (\"hello\";\"world\")");
    run!(ok "fileio" "`:t/ set t");
    run!(ok "fileio" "get `:t");

    // -----------------------------------------------------------------------
    // Functional forms
    // -----------------------------------------------------------------------
    run!(ok "func" "@[tab;`col;:;newval]");
    run!(ok "func" "@[tab;`col;+;1]");
    run!(ok "func" "@[tab;`col;,;`new]");
    run!(ok "func" ".[t;(0;`price);:;99]");
    run!(ok "func" "![t;();0b;`col1`col2]");  // functional delete
    run!(ok "func" "?[t;enlist(>;`price;100);0b;()]");  // functional select

    // -----------------------------------------------------------------------
    // Amend / indexed assign
    // -----------------------------------------------------------------------
    run!(ok "amend" "@[t;`col;:;x]");
    run!(ok "amend" ".[colNames;where colNames=old;:;new]");
    run!(ok "amend" "@[tdir;`.d;,;cname]");

    // -----------------------------------------------------------------------
    // Namespace
    // -----------------------------------------------------------------------
    run!(ok "ns" ".ns.x");
    run!(ok "ns" ".Q.dd[db;tname]");
    run!(ok "ns" ".Q.en[db;data]");
    run!(ok "ns" ".z.p");
    run!(ok "ns" ".z.h");
    run!(ok "ns" ".h.htc[`pre;x]");

    // -----------------------------------------------------------------------
    // System commands
    // -----------------------------------------------------------------------
    run!(ok "syscmd" "\\l file.q");
    run!(ok "syscmd" "\\p 5000");
    run!(ok "syscmd" "\\d .ns");
    run!(ok "syscmd" "\\t expr");
    run!(ok "syscmd" "\\v");
    run!(ok "syscmd" "\\a");
    run!(ok "syscmd" "\\g 1");
    run!(ok "syscmd" "\\b");
    run!(ok "syscmd" "\\c 25 80");

    // -----------------------------------------------------------------------
    // Comments
    // -----------------------------------------------------------------------
    run!(ok "comment" "x:1 / inline comment");
    run!(ok "comment" "// double slash comment\nx:1");
    run!(ok "comment" "/\nblock comment\n\\\nx:1");

    // -----------------------------------------------------------------------
    // Multi-statement
    // -----------------------------------------------------------------------
    run!(ok "multi" "a:1;\nb:2");
    run!(ok "multi" "a:1; b:2; c:a+b");
    run!(ok "multi" "a:1\nb:2\nc:3");

    // -----------------------------------------------------------------------
    // Projection
    // -----------------------------------------------------------------------
    run!(ok "proj" "1+");
    run!(ok "proj" "f[;x]");
    run!(ok "proj" "f[x;]");
    run!(ok "proj" "+[;1]");
    run!(ok "proj" "$[cond;neg;]");

    // -----------------------------------------------------------------------
    // Complex / real-world patterns
    // -----------------------------------------------------------------------
    run!(ok "real" "x where x>0");
    run!(ok "real" "t where t[`price]>100");
    run!(ok "real" "x@where x>0");           // @ as index
    run!(ok "real" "x bin y");
    run!(ok "real" "x binr y");
    run!(ok "real" "x xbar 5");
    run!(ok "real" "1_ x");
    run!(ok "real" "-1_ x");
    run!(ok "real" "2# x");
    run!(ok "real" "-2# x");
    run!(ok "real" "x inter y");
    run!(ok "real" "x except y");
    run!(ok "real" "x union y");
    run!(ok "real" "x cross y");
    run!(ok "real" "x rank y");
    run!(ok "real" "x differ y");
    run!(ok "real" "differ x");
    run!(ok "real" "x vs y");
    run!(ok "real" "x sv y");
    run!(ok "real" "x ss y");
    run!(ok "real" "x like \"pat*\"");
    run!(ok "real" "mcount[3;x]");
    run!(ok "real" "mavg[3;x]");
    run!(ok "real" "x mmu y");
    run!(ok "real" "x lsq y");
    run!(ok "real" "x wavg y");
    run!(ok "real" "x wsum y");
    run!(ok "real" "x cor y");
    run!(ok "real" "x cov y");
    run!(ok "real" "x fby (g;t)");     // fby aggregation
    run!(ok "real" "(sum;avg;min;max) @\\: t");  // apply each-left
    run!(ok "real" "p:((+);(-);(*);(%))");       // list of operators
    run!(ok "real" "t:2000.01.01+til 10");       // date arithmetic
    run!(ok "real" "til 10");
    run!(ok "real" "0N?x");                      // random draw
    run!(ok "real" "10?x");
    run!(ok "real" "10?`4");                     // random syms
    run!(ok "real" ".Q.s x");
    run!(ok "real" ".Q.s1 x");

    // -----------------------------------------------------------------------
    // Tricky: adverb + negative number
    // -----------------------------------------------------------------------
    run!(ok "tricky" "+/-1");          // over applied to -1
    run!(ok "tricky" "+/ -1 2 3");    // over applied to negative list start
    run!(ok "tricky" "0 +/ 1 2 3");   // seeded over with space
    run!(ok "tricky" "f -1");          // apply f to -1: juxtapose or binary?
    run!(ok "tricky" "f-1");           // f minus 1: binary
    run!(ok "tricky" "{x}-1");         // lambda minus 1?
    run!(ok "tricky" "(f)(-1)");       // f applied to (-1) in parens

    // -----------------------------------------------------------------------
    // Tricky: qSQL with complex column expressions
    // -----------------------------------------------------------------------
    run!(ok "tricky" "select x+y from t");
    run!(ok "tricky" "select x*y%z from t");
    run!(ok "tricky" "select 1+x from t");
    run!(ok "tricky" "select f[x] from t");
    run!(ok "tricky" "select {x+1}each x from t");
    run!(ok "tricky" "select x[`a] from t");
    run!(ok "tricky" "select (x;y) from t");

    // -----------------------------------------------------------------------
    // Tricky: nested qSQL
    // -----------------------------------------------------------------------
    run!(ok "tricky" "select from (select from t) where x>0");
    run!(ok "tricky" "t lj select sym,name from ref");
    run!(ok "tricky" "raze select from each t");

    // -----------------------------------------------------------------------
    // Tricky: lambdas in qSQL
    // -----------------------------------------------------------------------
    run!(ok "tricky" "select {[x] x*2} each price from trade");
    run!(ok "tricky" "select price where {x>0} each price from trade");

    // -----------------------------------------------------------------------
    // Tricky: assignment inside expressions
    // -----------------------------------------------------------------------
    run!(ok "tricky" "f each p:buildPaths[db;tname]");
    run!(ok "tricky" "(db:getFSym;tname:`s):4#p");
    run!(ok "tricky" "a:b:1+2");

    // -----------------------------------------------------------------------
    // Tricky: operator-as-value forms
    // -----------------------------------------------------------------------
    run!(ok "tricky" "(+)");
    run!(ok "tricky" "(+;-)");
    run!(ok "tricky" "(+;-;*;%)");
    run!(ok "tricky" "(+) . (1;2)");
    run!(ok "tricky" "(+)[1;2]");
    run!(ok "tricky" "(,/)");              // over as value
    run!(ok "tricky" "(,'/)");             // each-over? No: compose then over? Hmm

    // -----------------------------------------------------------------------
    // Tricky: chained index
    // -----------------------------------------------------------------------
    run!(ok "tricky" "t[0][`price]");
    run!(ok "tricky" "t[0;`price]");
    run!(ok "tricky" "a[0][1][2]");
    run!(ok "tricky" "a . 0 1 2");

    // -----------------------------------------------------------------------
    // Tricky: return / signal as non-first statement
    // -----------------------------------------------------------------------
    run!(ok "tricky" "{if[x>0;:x]; neg x}");
    run!(ok "tricky" "{if[x<0;'\"neg\"]; x}");
    run!(ok "tricky" "{[x] a:x*2; :a}");

    // -----------------------------------------------------------------------
    // Tricky: each applied to function producing function
    // -----------------------------------------------------------------------
    run!(ok "tricky" "{x+y}peach 1 2 3");
    run!(ok "tricky" "f each/: t");   // each-right inside... does this work?

    // -----------------------------------------------------------------------
    // DSL / k escape
    // -----------------------------------------------------------------------
    run!(ok "dsl" "k)1+2");
    run!(ok "dsl" "p)1+2");

    // -----------------------------------------------------------------------
    // Shebang
    // -----------------------------------------------------------------------
    run!(ok "shebang" "#!/usr/bin/env q\nx:42");

    // -----------------------------------------------------------------------
    // Global null / return
    // -----------------------------------------------------------------------
    run!(ok "misc" "::");                 // generic null
    run!(ok "misc" ":x");                 // return x
    run!(ok "misc" "::x");                // global assign to x? or ::(x)?

    // -----------------------------------------------------------------------
    // Multiline with indented continuation
    // -----------------------------------------------------------------------
    run!(ok "multiline" "f x\n  +y");    // continuation (indented)
    run!(ok "multiline" "a:1\nb:2");     // two statements

    // -----------------------------------------------------------------------
    // Complex real-world function
    // -----------------------------------------------------------------------
    run!(ok "realworld" r#"checkTab:{[db;tname]
    if[tname in key db; :1b];
    paths: .Q.dd[db;tname];
    1b
  }"#);
    run!(ok "realworld" "f:{[x;y]\n    r: x+y;\n    if[r>100; '\"overflow\"];\n    r\n  }");
    run!(ok "realworld" "t:([] sym:`a`b`c; price:1.1 2.2 3.3; size:100 200 300)");
    run!(ok "realworld" "select avg price by sym from t where size>100");
    run!(ok "realworld" "update price:price*1.1 from `t where sym=`a");
    run!(ok "realworld" "a:+/[1 2 3 4 5]");

    // -----------------------------------------------------------------------
    // Summary
    // -----------------------------------------------------------------------
    println!("\n========================================");
    println!("PASS: {pass}  FAIL: {fail}");
    println!("========================================");

    if fail > 0 {
        std::process::exit(1);
    }
}
