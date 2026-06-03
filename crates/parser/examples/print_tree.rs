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
    let exprs: Vec<&str> = if args.len() > 1 {
        args[1..].iter().map(|s| s.as_str()).collect()
    } else {
        vec![
            "a+b*c",
            "a:b:1",
            "a+:1",
            "select sum a by b from t",
            "update a:1 from t",
            "delete from t where a>0",
            "f:{[x;y] x+y}",
            "{x+y}[1;2]",
            "(+')f",
            "f/[x]",
            "a where b",
            "$[a;b;c]",
            "do[3;a+:1]",
            "t:([]a:1 2 3;b:4 5 6)",
            "a:+/1 2 3",
            "1 2!3 4",
            "a lj b",
            ":a",
            "a _ b",
            "(+) . (1 2;3 4)",
        ]
    };

    for expr in exprs {
        println!("\n=== {expr:?} ===");
        let parse = q_parser::parse(expr);
        if !parse.errors.is_empty() {
            println!("  ERRORS: {:?}", parse.errors);
        }
        print_node(&parse.syntax(), 0);
    }
}
