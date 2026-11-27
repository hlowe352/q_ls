//! Differential test against q itself: every statement in the fixtures is
//! converted from our CST into the parse tree q's `parse` would build
//! (`x+y` → `(+;`x;`y)`), and a real q compares the two with `~`.
//!
//! Needs a q binary (`$Q_BIN`, else `~/.kx/bin/q`) and is skipped without
//! one. Mismatches q shares with us by design, or that await a fix, are
//! listed in `tests/data/q_oracle_known.txt`; a listed statement that now
//! matches fails the test too, so the list only shrinks.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use q_parser::{SyntaxElement, SyntaxKind as K, SyntaxNode, SyntaxToken};

/// q helpers. `G` turns a glyph or name into what `parse` puts in its tree
/// (verbs and keywords as values, other names as symbols); `N` renders a
/// tree so q's symbol-vector collapsing can't hide or invent differences.
const DRIVER: &str = r#"
G:{x:(),x;$[x~(),":";first parse"a:1";any x~/:(),/:("/";"\\";"'";"':";"/:";"\\:");first parse"f",x;parse x]};
N:{$[0h=type x;"(",(";"sv N each x),")";11h=type x;$[1=count x;",",.Q.s1 x;"(",(";"sv .Q.s1 each x),")"];.Q.s1 x]};
P:{@[parse;(),x;{`QERR}]};
T:{[i;s;o] p:P s; $[`QERR~p; -1"QONLY\t",string i; not(N p)~N v:@[value;o;{`OERR}]; -1"DIFF\t",string[i],"\t",.Q.s1[p],"\t",.Q.s1 v; ::]};
E:{[i;s] if[not`QERR~P s; -1"OURSONLY\t",string i]};
"#;

fn q_bin() -> Option<PathBuf> {
    std::env::var_os("Q_BIN")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".kx/bin/q")))
        .filter(|p| p.is_file())
}

/// A q string literal.
fn qstr(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' | '\\' => out.extend(['\\', c]),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// q code building the value `parse` gives for the glyph or name `s`.
fn g(s: &str) -> String {
    format!("(G{})", qstr(s))
}

/// Source text with each run of trivia collapsed to one space.
fn flat(node: &SyntaxNode) -> String {
    let mut out = String::new();
    for t in node
        .descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
    {
        if !t.kind().is_trivia() {
            out.push_str(t.text());
        } else if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

fn tokens(node: &SyntaxNode) -> Vec<SyntaxToken> {
    node.children_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|t| !t.kind().is_trivia())
        .collect()
}

/// Entries of a `(…;…)` / `[…;…]` node; an empty entry is `::`.
fn entries(node: &SyntaxNode) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = None;
    for el in node.children_with_tokens() {
        match el {
            SyntaxElement::Node(n) => cur = Some(to_q(&n)?),
            SyntaxElement::Token(t) if matches!(t.kind(), K::Semi | K::RBracket | K::RParen) => {
                out.push(cur.take().unwrap_or_else(|| "::".into()));
            }
            SyntaxElement::Token(_) => {}
        }
    }
    // `f[]` / `[]`: one empty entry, not a list of one.
    if out == ["::"] && node.children().next().is_none() {
        out.clear();
    }
    Some(out)
}

/// q code that evaluates to the parse tree of `n`, or `None` for forms the
/// converter doesn't model (tables, qSQL).
fn to_q(n: &SyntaxNode) -> Option<String> {
    let kids: Vec<SyntaxNode> = n.children().collect();
    let toks = tokens(n);
    let first = || toks.first().map(SyntaxToken::text);
    Some(match n.kind() {
        K::ExprStmt => kids.first().map_or(Some("::".into()), to_q)?,
        K::ParenExpr => to_q(kids.first()?)?,
        K::ListExpr if kids.is_empty() && toks.len() == 2 => "()".into(),
        K::ListExpr => format!("({};{})", g("enlist"), entries(n)?.join(";")),
        K::VectorExpr => format!("({})", flat(n)),
        K::LiteralExpr | K::FileSymbolExpr => match toks[0].kind() {
            K::Symbol | K::FileSymbol => format!("(enlist {})", toks[0].text()),
            _ => format!("({})", toks[0].text()),
        },
        K::IdentExpr | K::Namespace | K::VerbExpr => g(first()?),
        // The verb is a direct token for a primitive, else the middle node.
        K::BinExpr | K::InfixProjection => {
            let primitive = kids.len() == if n.kind() == K::BinExpr { 2 } else { 1 };
            let (verb, rhs) = if primitive {
                (g(first()?), kids.get(1))
            } else {
                (to_q(&kids[1])?, kids.get(2))
            };
            let lhs = to_q(&kids[0])?;
            match rhs {
                Some(r) => format!("({verb};{lhs};{})", to_q(r)?),
                None => format!("({verb};{lhs})"),
            }
        }
        K::ApplyExpr => format!("({};{})", to_q(&kids[0])?, to_q(&kids[1])?),
        K::AdverbExpr => format!("({};{})", g(first()?), to_q(&kids[0])?),
        K::IndexExpr => {
            let mut args = entries(&kids[1])?;
            if args.is_empty() {
                args.push("::".into());
            }
            format!("({};{})", to_q(&kids[0])?, args.join(";"))
        }
        K::SignalExpr => format!("(\"'\";{})", to_q(&kids[0])?),
        K::ReturnExpr => format!("(\":\";{})", to_q(&kids[0])?),
        K::PrognExpr => match entries(n)?.as_slice() {
            [] => "::".into(),
            [one] => one.clone(),
            many => format!("(\";\";{})", many.join(";")),
        },
        // Compared as text: `parse` keeps lambdas as values.
        K::Lambda => flat(n),
        _ => return None,
    })
}

/// Statements to check: top-level ones and every lambda-body statement.
fn statements(src: &str, out: &mut Vec<(String, Option<String>)>) {
    let parse = q_parser::parse(src);
    for stmt in parse.syntax().descendants() {
        let parent = stmt.parent().map(|p| p.kind());
        if stmt.kind() != K::ExprStmt || !matches!(parent, Some(K::Root | K::Lambda)) {
            continue;
        }
        let text = flat(&stmt);
        let range = stmt.text_range();
        let has_error = stmt.descendants_with_tokens().any(|e| e.kind() == K::Error)
            || parse
                .errors
                .iter()
                .any(|e| range.contains(u32::try_from(e.offset).unwrap().into()));
        if text.is_empty() {
            continue;
        }
        if has_error {
            out.push((text, None));
        } else if let Some(form) = to_q(&stmt) {
            out.push((text, Some(form)));
        }
    }
}

fn fixture_sources() -> Vec<String> {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    // One expression per line; `/` lines are comments.
    let mut srcs: Vec<String> = std::fs::read_to_string(data.join("q_oracle.q"))
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('/'))
        .map(String::from)
        .collect();
    for dir in ["corpus", "real_q"] {
        let mut paths: Vec<_> = std::fs::read_dir(data.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "q"))
            .collect();
        paths.sort();
        srcs.extend(paths.iter().map(|p| std::fs::read_to_string(p).unwrap()));
    }
    srcs
}

#[test]
fn parse_trees_match_q() {
    let Some(q) = q_bin() else {
        eprintln!("skipping: no q binary (set Q_BIN)");
        return;
    };
    // Deep real-world CSTs need more than the default test stack.
    let cases = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| {
            let mut cases = Vec::new();
            for src in fixture_sources() {
                statements(&src, &mut cases);
            }
            cases
        })
        .unwrap()
        .join()
        .unwrap();

    let mut script = DRIVER.to_string();
    for (i, (text, form)) in cases.iter().enumerate() {
        match form {
            Some(form) => writeln!(script, "T[{i};{};{}];", qstr(text), qstr(form)),
            None => writeln!(script, "E[{i};{}];", qstr(text)),
        }
        .unwrap();
    }
    script.push_str("-1\"DONE\";\nexit 0\n");
    let path = std::env::temp_dir().join(format!("q_oracle_{}.q", std::process::id()));
    std::fs::write(&path, script).unwrap();
    let out = Command::new(&q)
        .arg(&path)
        .arg("-q")
        .stdin(Stdio::null())
        .output()
        .expect("run q");
    std::fs::remove_file(&path).unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().any(|l| l == "DONE"),
        "q did not finish:\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let known_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/q_oracle_known.txt");
    let known: BTreeSet<String> = std::fs::read_to_string(&known_path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect();
    let mut seen = BTreeSet::new();
    let mut unexpected = Vec::new();
    for line in stdout.lines().filter(|l| *l != "DONE") {
        let mut parts = line.splitn(4, '\t');
        let (Some(what), Some(i)) = (parts.next(), parts.next()) else {
            continue;
        };
        let text = &cases[i.parse::<usize>().unwrap()].0;
        seen.insert(text.clone());
        if !known.contains(text) {
            let detail: Vec<_> = parts.collect();
            unexpected.push(format!("{what}: {text}\n    {}", detail.join("\n    ")));
        }
    }
    let stale: Vec<_> = known.difference(&seen).collect();
    assert!(
        unexpected.is_empty() && stale.is_empty(),
        "{} of {} statements differ from q (QONLY: q rejects, we accept; \
         OURSONLY: we reject, q accepts; DIFF: q tree, then ours):\n{}\n\n\
         now matching q, remove from q_oracle_known.txt:\n{}",
        unexpected.len(),
        cases.len(),
        unexpected.join("\n"),
        stale
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
