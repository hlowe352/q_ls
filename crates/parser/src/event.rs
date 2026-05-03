use crate::syntax_kind::SyntaxKind;

#[derive(Debug, Clone)]
pub enum Event {
    Start { kind: SyntaxKind, forward_parent: Option<usize> },
    Token { kind: SyntaxKind, text: String },
    Finish,
    Placeholder,
}
