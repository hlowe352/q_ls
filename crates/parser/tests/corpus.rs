//! Corpus-driven smoke tests: every example from tree-sitter-q's corpus
//! AND every real-world q fixture must parse without producing
//! `SyntaxKind::Error` tokens.

use q_parser::{parse, SyntaxKind, SyntaxNode};

fn collect_errors(src: &str, node: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    for elem in node.descendants_with_tokens() {
        if elem.kind() == SyntaxKind::Error {
            let r = elem.text_range();
            let off: usize = r.start().into();
            let line = src[..off].matches('\n').count() + 1;
            let col = off - src[..off].rfind('\n').map_or(0, |i| i + 1);
            // Avoid `{elem:?}` — the rowan Debug impl recurses into children
            // and overflows the default test-thread stack on deeply nested trees.
            let snippet: String = elem.as_token().map_or_else(
                String::new,
                |t| t.text().to_string(),
            );
            out.push(format!("line {line}:{col} Error {snippet:?}"));
        }
    }
    out
}

fn run_corpus(dir_name: &str, label: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(dir_name);
    let mut failures = Vec::new();
    let mut total = 0;
    for entry in std::fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("q") {
            continue;
        }
        total += 1;
        let src = std::fs::read_to_string(&path).unwrap();
        let parsed = parse(&src);
        let mut errs = collect_errors(&src, &parsed.syntax());
        for e in &parsed.errors {
            let off = e.offset;
            let line = src[..off].matches('\n').count() + 1;
            let col = off - src[..off].rfind('\n').map_or(0, |i| i + 1);
            errs.push(format!("line {line}:{col} ParseError {:?}", e.message));
        }
        if !errs.is_empty() {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            failures.push(format!("{name}: {}", errs.join("; ")));
        }
    }
    assert!(failures.is_empty(),
        "{label}: {}/{} files failed:\n{}",
        failures.len(), total, failures.join("\n"));
}

#[test]
fn parses_tree_sitter_q_corpus_clean() {
    run_corpus("corpus", "tree-sitter-q corpus");
}

#[test]
fn parses_real_q_fixtures_clean() {
    // Real-world q files (e.g. dbmaint.q) produce deeply-nested CSTs whose
    // recursive `Drop` exceeds the default 2 MiB test-thread stack on
    // some platforms. Run on an 8 MiB stack to avoid the overflow.
    let handle = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(|| run_corpus("real_q", "real-world q fixtures"))
        .unwrap();
    handle.join().unwrap();
}
