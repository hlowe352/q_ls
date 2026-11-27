//! Clean-parse tests: every input below, every tree-sitter-q corpus example,
//! and every real-world q fixture must parse losslessly with no `ParseError`s
//! and no `SyntaxKind::Error` tokens.

use q_parser::{SyntaxKind, parse};

/// Problems with parsing `src`: parse errors, `Error` tokens, or text loss.
fn problems(src: &str) -> Vec<String> {
    let line_col = |off: usize| {
        let line = src[..off].matches('\n').count() + 1;
        let col = off - src[..off].rfind('\n').map_or(0, |i| i + 1);
        format!("{line}:{col}")
    };
    let p = parse(src);
    let root = p.syntax();
    // Avoid `{elem:?}` — rowan's Debug recurses and overflows on deep trees.
    let mut out: Vec<String> = root
        .descendants_with_tokens()
        .filter(|e| e.kind() == SyntaxKind::Error)
        .map(|e| {
            let text = e.as_token().map_or("", |t| t.text());
            format!("{} Error {text:?}", line_col(e.text_range().start().into()))
        })
        .collect();
    out.extend(
        p.errors
            .iter()
            .map(|e| format!("{} ParseError {:?}", line_col(e.offset), e.message)),
    );
    if root.text() != src {
        out.push("not lossless".into());
    }
    out
}

/// Run `f` on an 8 MiB stack: deeply nested real-world CSTs overflow the
/// default 2 MiB test thread on recursive `Drop`.
fn wide_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn run_corpus(dir_name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir_name);
    let mut total = 0;
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("q") {
            continue;
        }
        total += 1;
        let errs = problems(&std::fs::read_to_string(&path).unwrap());
        if !errs.is_empty() {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            failures.push(format!("{name}: {}", errs.join("; ")));
        }
    }
    assert!(
        failures.is_empty(),
        "{dir_name}: {}/{total} files failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn parses_tree_sitter_q_corpus_clean() {
    run_corpus("corpus");
}

#[test]
fn parses_real_q_fixtures_clean() {
    wide_stack(|| run_corpus("real_q"));
}

#[test]
fn parses_test_data_fixtures() {
    assert_eq!(
        problems(include_str!("../../../test_data/basic.q")),
        [""; 0]
    );
    assert_eq!(problems(include_str!("../../../test_data/qsql.q")), [""; 0]);
    // errors.q must report errors but stay lossless.
    let src = include_str!("../../../test_data/errors.q");
    let p = parse(src);
    assert!(!p.errors.is_empty());
    assert_eq!(p.syntax().text(), src);
}

#[test]
fn error_recovery_is_lossless() {
    let p = parse(")invalid");
    assert!(!p.errors.is_empty());
    assert_eq!(p.syntax().text(), ")invalid");
}

#[test]
fn parses_clean_cases() {
    let failures: Vec<String> = CLEAN
        .iter()
        .filter_map(|src| {
            let errs = problems(src);
            (!errs.is_empty()).then(|| format!("{src:?}: {}", errs.join("; ")))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[rustfmt::skip]
const CLEAN: &[&str] = &[
    "42",
    "42i",
    "42j",
    "42h",
    "3.14",
    "3.14e",
    "3.14f",
    "1e10",
    "1.5e-3",
    "0b",
    "1b",
    "01b",
    "010101b",
    "0x0F",
    "0xDEADBEEF",
    "\"hello\"",
    "\"\"",
    "\"a\"",
    "`sym",
    "`:file/path",
    "2000.01.01",
    "2024.06.04",
    "2000.01m",
    "12:00:00",
    "12:00:00.000",
    "12:00",
    "0Ng",
    "0Ni",
    "0Nj",
    "0Nh",
    "0Nn",
    "0Np",
    "0Nm",
    "0Nu",
    "0Nv",
    "0Nz",
    "0Wi",
    "0Wj",
    "-0Wi",
    "-0Wj",
    "0Wf",
    "0n",
    "0w",
    "0N",
    "0W",
    "`a`b`c",
    "``",
    "` `a",
    "`a`b`c!1 2 3",
    "1 2 3",
    "1 2 3h",
    "1 2 3i",
    "1 2 3j",
    "1 2 3f",
    "1 2 3e",
    "1.0 2.0 3.0",
    "1 2 3 4 5",
    "-1",
    "-1.0",
    "-1 2 3",
    "x-1",
    "x - 1",
    "x - -1",
    "-0W",
    "neg x",
    "1-2-3",
    "(-1)",
    "x-y-z",
    "1+2",
    "1+2+3",
    "2*3+4",
    "a%b%c",
    "x&y",
    "x|y",
    "x!y",
    "x#y",
    "x_y",
    "x~y",
    "x^y",
    "x,y",
    "x=y",
    "x<y",
    "x>y",
    "x<>y",
    "x<=y",
    "x>=y",
    "not x",
    "count x",
    "flip x",
    "type x",
    "enlist x",
    "string x",
    "raze x",
    "distinct x",
    "reverse x",
    "asc x",
    "desc x",
    "iasc x",
    "idesc x",
    "sum x",
    "avg x",
    "min x",
    "max x",
    "sdev x",
    "var x",
    "prd x",
    "first x",
    "last x",
    "abs x",
    "ceiling x",
    "floor x",
    "sqrt x",
    "exp x",
    "log x",
    "signum x",
    "reciprocal x",
    "hopen x",
    "hclose x",
    "read0 x",
    "read1 x",
    "get x",
    "key x",
    "value x",
    "parse x",
    "eval x",
    "load x",
    "tables x",
    "views x",
    "cols x",
    "meta x",
    "hdel x",
    "hsym x",
    "`int$x",
    "`float$x",
    "`sym$x",
    "`$x",
    "`long$1 2 3",
    "\"I\"$\"123\"",
    "\"J\"$x",
    "10h$x",
    "a:1",
    "a::1",
    ".ns.a:1",
    "a:b:c:1",
    "a+:1",
    "a-:1",
    "a*:1",
    "a%:1",
    "a,:1 2",
    "a|:x",
    "a&:x",
    "a[0]:1",
    "a[0;1]:1",
    "(a;b;c):1 2 3",
    "(a;b):x",
    "f x",
    "f[x]",
    "f[x;y]",
    "f[x;y;z]",
    "f[]",
    "f[;x]",
    "f[x;]",
    "f[;;]",
    "(f) x",
    "f . x",
    "f . (1;2;3)",
    "f @ x",
    "(+) . (1;2)",
    "{x+y}",
    "{[x] x+1}",
    "{[x;y] x+y}",
    "{[] 42}",
    "{x;y}",
    "{a:1; b:2; a+b}",
    "{[x] :x}",
    "{[x] {[y] x+y}}",
    "{x+y}[1;2]",
    "{x*x} each 1 2 3",
    "{x+y}/[0;1 2 3]",
    "{x+y}\\[0;1 2 3]",
    "f/",
    "f\\",
    "f'",
    "f':",
    "f/:",
    "f\\:",
    "1+/2",
    "1,/:2",
    "1-':2",
    "f/[10;x]",
    "f\\[10;x]",
    "f/\\:",
    "f[x]'",
    "(f')x",
    "(+/)x",
    "count each x",
    "f peach x",
    "{x+1} each 1 2 3",
    "(+')f",
    "f'[x;y]",
    "0+/x",
    "0 +/ x",
    "$[x;y;z]",
    "$[a;b;c;d;e]",
    "$[x;$[y;a;b];c]",
    "$[x>0;x;neg x]",
    "$[x;y]",
    "'x",
    "'\"`type\"",
    "'\"error message\"",
    ".[f;args;::]",
    ".[f;(x;y);{[e] e}]",
    "if[x>0; show x]",
    "if[x>0; show x; show y]",
    "if[x; :1b]",
    "do[5; x+:1]",
    "while[x<100; x*:2]",
    "if[x; '\"err\"]",
    "(1;2;3)",
    "()",
    "(1)",
    "(1 2 3)",
    "(1;2 3;4)",
    "(a;b;c)",
    "enlist 1",
    "1 2!`a`b",
    "(1;2)!(3;4)",
    "([]a:1 2;b:3 4)!x",
    "([] sym:`a`b`c; price:1 2 3)",
    "([sym:`a`b`c] price:1 2 3)",
    "([] a:1 2 3)",
    "([] )",
    "flip `a`b`c!(1 2;3 4;5 6)",
    "select from trade",
    "select sym from trade",
    "select sym,price from trade",
    "select sym:sym,p:price from trade",
    "select sum price from trade",
    "select sum price by sym from trade",
    "select avg price,sum size by sym from trade",
    "select from trade where sym=`AAPL",
    "select from trade where sym=`AAPL,price>100",
    "select from trade where sym in `AAPL`GOOG",
    "select from trade where not sym=`AAPL",
    "select from trade where price within 100 200",
    "select[5] from trade",
    "select[5;>price] from trade",
    "select[<price] from trade",
    "select distinct sym from trade",
    "exec sym from trade",
    "exec distinct sym from trade",
    "update price:price*1.1 from trade",
    "update price:price*1.1 from trade where sym=`AAPL",
    "delete from trade",
    "delete from trade where sym=`AAPL",
    "delete sym from trade",
    "select count i from trade",
    "select count i by sym from trade",
    "select from (select from trade)",
    "select from trade lj (select sym,name from ref)",
    "select {x*y}[price;size] from trade",
    "select avg price by date.month from trade",
    "t lj u",
    "t ij u",
    "t uj u",
    "t pj u",
    "t asof u",
    "t aj[`sym`time;u]",
    "t wj[w;`sym;u]",
    "(\"S\";enlist \",\") 0: `:data.csv",
    "`:output.txt 0: (\"hello\";\"world\")",
    "`:t/ set t",
    "get `:t",
    "@[tab;`col;:;newval]",
    "@[tab;`col;+;1]",
    "@[tab;`col;,;`new]",
    ".[t;(0;`price);:;99]",
    "![t;();0b;`col1`col2]",
    "?[t;enlist(>;`price;100);0b;()]",
    "@[t;`col;:;x]",
    ".[colNames;where colNames=old;:;new]",
    "@[tdir;`.d;,;cname]",
    ".ns.x",
    ".Q.dd[db;tname]",
    ".Q.en[db;data]",
    ".z.p",
    ".z.h",
    ".h.htc[`pre;x]",
    "\\l file.q",
    "\\p 5000",
    "\\d .ns",
    "\\t expr",
    "\\v",
    "\\a",
    "\\g 1",
    "\\b",
    "\\c 25 80",
    "x:1 / inline comment",
    "// double slash comment\nx:1",
    "/\nblock comment\n\\\nx:1",
    "a:1;\nb:2",
    "a:1; b:2; c:a+b",
    "a:1\nb:2\nc:3",
    "1+",
    "+[;1]",
    "$[cond;neg;]",
    "x where x>0",
    "t where t[`price]>100",
    "x@where x>0",
    "x bin y",
    "x binr y",
    "x xbar 5",
    "1_ x",
    "-1_ x",
    "2# x",
    "-2# x",
    "x inter y",
    "x except y",
    "x union y",
    "x cross y",
    "x rank y",
    "x differ y",
    "differ x",
    "x vs y",
    "x sv y",
    "x ss y",
    "x like \"pat*\"",
    "mcount[3;x]",
    "mavg[3;x]",
    "x mmu y",
    "x lsq y",
    "x wavg y",
    "x wsum y",
    "x cor y",
    "x cov y",
    "x fby (g;t)",
    "(sum;avg;min;max) @\\: t",
    "p:((+);(-);(*);(%))",
    "t:2000.01.01+til 10",
    "til 10",
    "0N?x",
    "10?x",
    "10?`4",
    ".Q.s x",
    ".Q.s1 x",
    "0 +/ 1 2 3",
    "f -1",
    "f-1",
    "(f)(-1)",
    "select x+y from t",
    "select x*y%z from t",
    "select 1+x from t",
    "select f[x] from t",
    "select {x+1}each x from t",
    "select x[`a] from t",
    "select (x;y) from t",
    "select from (select from t) where x>0",
    "t lj select sym,name from ref",
    "select {[x] x*2} each price from trade",
    "select price where {x>0} each price from trade",
    "f each p:buildPaths[db;tname]",
    "(db:getFSym;tname:`s):4#p",
    "a:b:1+2",
    "(+)",
    "(+;-)",
    "(+;-;*;%)",
    "(+)[1;2]",
    "(,/)",
    "(,'/)",
    "t[0][`price]",
    "t[0;`price]",
    "a[0][1][2]",
    "a . 0 1 2",
    "{if[x>0;:x]; neg x}",
    "{if[x<0;'\"neg\"]; x}",
    "{[x] a:x*2; :a}",
    "{x+y}peach 1 2 3",
    "f each/: t",
    "k)1+2",
    "p)1+2",
    "#!/usr/bin/env q\nx:42",
    "::",
    ":x",
    "f x\n  +y",
    "a:1\nb:2",
    r"checkTab:{[db;tname]
    if[tname in key db; :1b];
    paths: .Q.dd[db;tname];
    1b
  }",
    "f:{[x;y]\n    r: x+y;\n    if[r>100; '\"overflow\"];\n    r\n  }",
    "t:([] sym:`a`b`c; price:1.1 2.2 3.3; size:100 200 300)",
    "select avg price by sym from t where size>100",
    "update price:price*1.1 from `t where sym=`a",
    "a:+/[1 2 3 4 5]",
    "x:42",
    "x : 42 + 3",
    "{x*x}",
    "$[x>0;x;0]",
    "select price,size from trade",
    "select price,size from trade where sym=`AAPL",
    "select avg price by sym from trade",
    "delete from trade where price<0",
    "exec price from trade where sym=`GOOG",
    "read0 `:q1a.txt",
    "// this is a comment\nx:1",
    "loadf:loadf0[0b]   /load a file if it hasn't been loaded\nreloadf:loadf0[1b]",
    r#"q1a: {[]
    input: read0 `:q1a.txt;
    path: {(x+$["L"=first y;neg;] "J"$1_ y) mod 100}\[50;input];
    sum path=0
  }"#,
    "a:1;",
    "a:1;\nb:2;",
    "show q1a[];",
    "q1a: {[] 1};",
    r#"q1a: {[]
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
"#,
    "a<b<=c",
    "if[x>0; show x; show \"negative\"]",
    "if[tname in key db; :1b]",
    "if[not count[p] in 4 5; '\"must pass 4 or 5 params\"]",
    "do[5; x:x+1]",
    "while[x<100; x:x*2]",
    "{[x] if[x>0; :x; :neg x]}",
    "if:42",
    "$[99h ~ type x;enlist;] val",
    "'[{[p] p};enlist]",
    "get tdir,`.d",
    "'\".d mismatch at: \", 1_ string x",
    "f peach data",
    "x,/:y",
    "x,\\:y",
    "3+",
    "$[x;neg;]",
    "f[;db;data]",
    "x::42",
    ".[f;args; ::]",
    "{[db:getFSym;tname:`s] db}",
    r"checkTabExistence:{[db:getFSym;tname:`s]
    if[tname in key db; :1b];
    checkTablePathsExist buildTablePaths[db;tname];
    1b
 }",
    r#"checkDotDEquality:{[db:getFSym;tname:`s]
    if[not `partOrMissing ~ getTableType[db;tname]; 0b];
    d:differ get each .Q.dd[;`.d] each p:buildTablePaths[db;tname];
    if[1< sum d;
        '".d mismatch at: ", 1_string first 1_p where d];
    1b
 }"#,
    r#"addTab:('[{[p]
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
 };enlist])"#,
    "@[tdir;`.d;:;.[colNames;where colNames=old;:;new]]",
    ",[;tname]",
    ".[getValue;enlist path; ::]",
    r"delTab:{[db:getFSym;tname:`s]
    t:getTableType[db;tname];
    if[t ~ `flat; hdel .Q.dd[db;tname]; :()];
    del1Tab peach $[`splayed ~ t;
        enlist .Q.dd[db;tname];
        checkTablePathsExist buildTablePaths[db;tname]]
  }",
    "(rename .) each flip (oldPaths; newPaths)",
    "(db:getFSym;tname:`s;data;tabletype:`s):4#p",
    "f each p:buildTablePaths[db;tname]",
    "/\n  this is a block comment\n  spanning multiple lines\n\\\na:42",
    "f:{[x;y] if[x>0; :x+y]; neg x}",
    "a<=b>=c<>d",
    "x+:1",
    "x-:1",
    "x,:y",
    "@[tab;col;+:;val]",
    "opt,:p 4",
    "select distinct sym,price from trade",
    "x:1\n/\nblock\nstuff\n\\\ny:2\n",
    "(3#p),fn",
    "f x\n    +y",
    "f x\ng y",
    "a,:1 2 3",
    "select from t",
    "select a,b from t where c>0",
    "select sum a by b from t",
    "update a:1 from t",
    "delete from t where a>0",
    "exec a from t",
    "t:([]a:1 2 3;b:4 5 6)",
    "t:([k:1 2 3]v:4 5 6)",
    "1 2 3+4 5 6",
    "2#0N",
    "a[0;1]",
    "a where b",
    "$[a;b;c]",
    "@[f;x;g]",
    ".[f;x;g]",
    "0x0A",
    "1 2!3 4",
    "if[a;b]",
    "do[3;a+:1]",
    "while[a>0;a-:1]",
    "not a",
    "a lj b",
    "a asof b",
    "a bin b",
    "a cross b",
    "a ij b",
    "a uj b",
    "f/[x]",
    "f\\[x]",
    "([]a:1 2)",
    "flip `a`b!(1 2;3 4)",
    "a:enlist 1",
    "count each t",
    "raze (1 2;3 4)",
    "a+b*c-d%e",
    "f:{[x;y] x+y}",
    "f[1;2]",
    "a:b:1",
    "(+) . (1 2;3 4)",
    "a binr b",
    "a _ b",
    ":a",
    "a pj b",
    "x*:1",
    "x%:1",
    "x>:1",
    "x<:1",
    "x~:1",
    "x=:1",
    "x_:1",
    "x#:1",
    "x$:1",
    "x!:1",
    "x|:1",
    "x&:1",
    "x?:1",
    "x^:1",
    "x@:1",
    "0Wm",
    "0Wn",
    "0Wp",
    "0Wu",
    "0Wv",
    "0Wz",
];
