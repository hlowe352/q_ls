//! Corpus-driven smoke test: every example from tree-sitter-q's corpus
//! must parse without producing `SyntaxKind::Error` tokens.

use q_parser::{parse, SyntaxKind, SyntaxNode};

fn first_error(node: &SyntaxNode) -> Option<String> {
    for elem in node.descendants_with_tokens() {
        if elem.kind() == SyntaxKind::Error {
            return Some(format!("{:?} at {:?}", elem, elem.text_range()));
        }
    }
    None
}

#[test]
fn parses_tree_sitter_q_corpus_clean() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/corpus");
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
        if let Some(msg) = first_error(&parsed.syntax()) {
            failures.push(format!("{}: {}", path.file_name().unwrap().to_string_lossy(), msg));
        }
    }
    assert!(failures.is_empty(),
        "{}/{} corpus files failed:\n{}",
        failures.len(), total, failures.join("\n"));
}
