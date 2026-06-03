use q_parser::{SyntaxElement, SyntaxKind, SyntaxNode};

fn print_node(node: &SyntaxNode, indent: usize) {
    let pad = "  ".repeat(indent);
    let kind = node.kind();
    let text = node.text().to_string();
    // Leaf-equivalent: single token child with same span — print inline
    let tokens: Vec<_> = node
        .children_with_tokens()
        .filter(|e| !matches!(e.kind(), SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::CommentBlock))
        .collect();
    if tokens.len() == 1 {
        if let SyntaxElement::Token(t) = &tokens[0] {
            println!("{pad}{kind:?} {:?}", t.text());
            return;
        }
    }
    // Non-trivial text — print node kind then children
    let preview: String = text.chars().take(40).collect();
    println!("{pad}{kind:?} ({preview:?})");
    for child in node.children_with_tokens() {
        match child {
            SyntaxElement::Node(n) => print_node(&n, indent + 1),
            SyntaxElement::Token(t) => {
                if !matches!(t.kind(), SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::CommentBlock) {
                    println!("{pad}  {:?} {:?}", t.kind(), t.text());
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        for path in &args[1..] {
            let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("error reading {path:?}: {e}");
                std::process::exit(1);
            });
            println!("=== {path} ===");
            let parse = q_parser::parse(&source);
            if !parse.errors.is_empty() {
                for err in &parse.errors {
                    eprintln!("  ERROR: {}", err.message);
                }
            }
            print_node(&parse.syntax(), 0);
        }
    } else {
        // No args: read from stdin
        use std::io::Read;
        let mut source = String::new();
        std::io::stdin().read_to_string(&mut source).unwrap();
        let parse = q_parser::parse(&source);
        if !parse.errors.is_empty() {
            for err in &parse.errors {
                eprintln!("  ERROR: {}", err.message);
            }
        }
        print_node(&parse.syntax(), 0);
    }
}
