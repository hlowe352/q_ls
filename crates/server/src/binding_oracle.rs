//! Binding oracle: each case runs in a real q (see `q_test_support::q_bin`;
//! skipped without one) in several contexts — top level, a lambda, `\d .ns`,
//! a lambda defined in `.ns` (called there and from the root), and
//! `system "d .ns"`. The globals q creates or changes must be exactly the
//! names the binder records a global definition for inside the case.

use std::path::Path;

use crate::binder::Symbol;
use crate::document::Document;

/// `(prelude, code, names to watch)`. The prelude runs first at the root;
/// a name not starting with `.` is also watched as `.ns.name`.
#[rustfmt::skip]
const CASES: &[(&str, &str, &[&str])] = &[
    ("", "a:1", &["a"]),
    ("", "a::1", &["a"]),
    ("", ".x.y:1", &[".x.y"]),
    ("", "a:b:1", &["a", "b"]),
    ("", "(a;b):1 2", &["a", "b"]),
    ("", "f:{x}", &["f"]),
    ("", "if[1b;a:1]", &["a"]),
    ("", "$[1b;a:1;b:2]", &["a", "b"]),
    ("", "do[1;a:1]", &["a"]),
    ("", "x:1", &["x"]),
    ("a:1;.ns.a:1", "a+:1", &["a"]),
    ("a:1 2;.ns.a:1 2", "a,:3", &["a"]),
    ("a:1;.ns.a:1", "a+::1", &["a"]),
    ("a:(1#`k)!1#1;.ns.a:(1#`k)!1#1", "a[`k]:2", &["a"]),
    ("a:1;.ns.a:1", "a:2;b:a", &["a", "b"]),
    ("a:0;.ns.a:0", "{a::2}[]", &["a"]),
    ("f:10;.ns.f:10", "f::+", &["f"]),
    ("", "a:1;a::2", &["a"]),
    ("", "`a set 1", &["a"]),
    ("", "b,:1", &["b"]),
    ("a:0;.ns.a:0", "{[a] a::5}[1]", &["a"]),
    ("a:1 2;.ns.a:1 2", "a[0]:5", &["a"]),
    ("a:1 2;.ns.a:1 2", "a:3 4;a[0]:5", &["a"]),
    ("", "a.b:1", &["a.b"]),
    ("", ".ns.z:1", &[".ns.z"]),
    ("", "{a::1}[]", &["a"]),
    ("", "a:1;{a::2}[]", &["a"]),
];

/// Cases where our symbol table deliberately differs from q, with why.
#[rustfmt::skip]
const KNOWN: &[(&str, Ctx, &str)] = &[
    // Static analysis: either branch may run.
    ("$[1b;a:1;b:2]", Ctx::Top, "only the taken branch runs"),
    ("$[1b;a:1;b:2]", Ctx::Ns, "only the taken branch runs"),
    ("$[1b;a:1;b:2]", Ctx::SystemD, "only the taken branch runs"),
    // q's `parse` says composition; only the top-level evaluator assigns.
    ("f::+", Ctx::Top, "top-level evaluator quirk"),
    ("f::+", Ctx::Ns, "top-level evaluator quirk"),
    ("f::+", Ctx::SystemD, "top-level evaluator quirk"),
    // Within one top-level line under `\d`, q resolves a name to a single
    // target for the whole line.
    ("a:1;a::2", Ctx::Ns, "one target per name per top-level line"),
    ("a:1;a::2", Ctx::SystemD, "one target per name per top-level line"),
    // `set` is not modelled (its namespace is decided at run time).
    ("`a set 1", Ctx::Top, "set not modelled"),
    ("`a set 1", Ctx::Lambda, "set not modelled"),
    ("`a set 1", Ctx::Ns, "set not modelled"),
    ("`a set 1", Ctx::NsLambda, "set not modelled"),
    ("`a set 1", Ctx::NsLambdaFromRoot, "set not modelled"),
    ("`a set 1", Ctx::SystemD, "set not modelled"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctx {
    Top,
    Lambda,
    Ns,
    NsLambda,
    NsLambdaFromRoot,
    SystemD,
}

const CONTEXTS: [Ctx; 6] = [
    Ctx::Top,
    Ctx::Lambda,
    Ctx::Ns,
    Ctx::NsLambda,
    Ctx::NsLambdaFromRoot,
    Ctx::SystemD,
];

/// Source running `code` in `ctx` after `prelude`, and where `code` starts.
fn wrap(ctx: Ctx, prelude: &str, code: &str) -> (String, usize) {
    let (before, after) = match ctx {
        Ctx::Top => ("", ""),
        Ctx::Lambda => ("run:{", "}\nrun[]"),
        Ctx::Ns => ("\\d .ns\n", "\n\\d ."),
        Ctx::NsLambda => ("\\d .ns\nrun:{", "}\nrun[]\n\\d ."),
        Ctx::NsLambdaFromRoot => ("\\d .ns\nrun:{", "}\n\\d .\n.ns.run[]"),
        Ctx::SystemD => ("system \"d .ns\"\n", "\nsystem \"d .\""),
    };
    let head = format!("{prelude}\n{before}");
    (format!("{head}{code}{after}\n"), head.len())
}

/// Every global a watched name could mean.
fn candidates(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .flat_map(|n| {
            let ns = (!n.starts_with('.')).then(|| format!(".ns.{n}"));
            std::iter::once((*n).to_string()).chain(ns)
        })
        .collect()
}

/// Candidates our symbol table defines globally inside `code`.
fn ours(src: &str, code: std::ops::Range<usize>, cands: &[String]) -> Vec<String> {
    let doc = Document::new(src.to_string(), 0);
    cands
        .iter()
        .filter(|c| {
            doc.bindings()
                .defs(&Symbol::Global(c.as_str().into()))
                .iter()
                .any(|&o| code.contains(&(o as usize)))
        })
        .cloned()
        .collect()
}

/// Candidates q creates or changes running `src`.
fn theirs(q: &Path, src: &str, cands: &[String]) -> Result<Vec<String>, String> {
    let syms = format!("`{}", cands.join("`"));
    // The prelude is `src`'s first line; snapshot right after it.
    let (prelude, rest) = src.split_once('\n').unwrap();
    let script = format!(
        "{prelude}\n.oracle.get:{{@[get;x;{{`oracle_missing}}]}};\n\
         .oracle.c:(),{syms};\n.oracle.b:.oracle.get each .oracle.c;\n{rest}\\d .\n\
         -1 \"CHANGED \",/:string .oracle.c where not .oracle.b~'.oracle.get each .oracle.c;"
    );
    let stdout = q_test_support::run_q(q, &script)?;
    Ok(stdout
        .lines()
        .filter_map(|l| l.strip_prefix("CHANGED "))
        .map(String::from)
        .collect())
}

#[test]
fn bindings_match_q() {
    let Some(q) = q_test_support::q_bin() else {
        eprintln!("skipping: no q binary (set Q_BIN)");
        return;
    };
    let results: Vec<_> = std::thread::scope(|s| {
        let jobs: Vec<_> = CASES
            .iter()
            .flat_map(|case| CONTEXTS.iter().map(move |&ctx| (case, ctx)))
            .map(|(&(prelude, code, names), ctx)| {
                let q = &q;
                s.spawn(move || {
                    let (src, at) = wrap(ctx, prelude, code);
                    let cands = candidates(names);
                    let ours = ours(&src, at..at + code.len(), &cands);
                    (code, ctx, theirs(q, &src, &cands), ours)
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });

    let mut failures = Vec::new();
    let mut matched_known = Vec::new();
    for (code, ctx, theirs, ours) in results {
        let known = KNOWN.iter().any(|&(c, x, _)| c == code && x == ctx);
        match theirs {
            Err(e) => failures.push(format!("{code:?} in {ctx:?}: q failed:\n{e}")),
            Ok(theirs) if theirs != ours && !known => failures.push(format!(
                "{code:?} in {ctx:?}: q defines {theirs:?}, we define {ours:?}"
            )),
            Ok(theirs) if theirs == ours && known => {
                matched_known.push(format!("{code:?} in {ctx:?}"));
            }
            Ok(_) => {}
        }
    }
    assert!(
        failures.is_empty() && matched_known.is_empty(),
        "{}\n\nnow matching q, remove from KNOWN:\n{}",
        failures.join("\n"),
        matched_known.join("\n")
    );
}
