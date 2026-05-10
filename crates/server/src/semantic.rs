//! Semantic-token classifier.
//!
//! Walks every CST token in source order and classifies it against the
//! legend below, then emits the LSP delta-encoded
//! `[deltaLine, deltaStart, length, type, modifiers]` quintuples.

use tower_lsp_server::ls_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType,
};
use q_parser::{SyntaxKind, SyntaxToken};

use crate::builtins::is_builtin;
use crate::document::Document;

/// Token-type legend, in the order LSP indices reference. Keep this aligned
/// with `TYPE_*` consts below — `legend()` returns it for capability
/// registration.
pub const TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::FUNCTION,   // 0
    SemanticTokenType::VARIABLE,   // 1
    SemanticTokenType::PARAMETER,  // 2
    SemanticTokenType::NAMESPACE,  // 3
    SemanticTokenType::KEYWORD,    // 4
    SemanticTokenType::STRING,     // 5
    SemanticTokenType::NUMBER,     // 6
    SemanticTokenType::COMMENT,    // 7
    SemanticTokenType::OPERATOR,   // 8
];

const TYPE_FUNCTION: u32 = 0;
const TYPE_VARIABLE: u32 = 1;
const TYPE_PARAMETER: u32 = 2;
const TYPE_NAMESPACE: u32 = 3;
const TYPE_KEYWORD: u32 = 4;
const TYPE_STRING: u32 = 5;
const TYPE_NUMBER: u32 = 6;
const TYPE_COMMENT: u32 = 7;
const TYPE_OPERATOR: u32 = 8;

pub const MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,     // bit 0
    SemanticTokenModifier::DEFAULT_LIBRARY, // bit 1
];
const MOD_DECLARATION: u32 = 1 << 0;
const MOD_DEFAULT_LIBRARY: u32 = 1 << 1;

pub fn legend() -> (Vec<SemanticTokenType>, Vec<SemanticTokenModifier>) {
    (TYPES.to_vec(), MODIFIERS.to_vec())
}

pub fn semantic_tokens(doc: &Document) -> Vec<SemanticToken> {
    let root = doc.parse().syntax();

    let mut out: Vec<SemanticToken> = Vec::new();
    let mut prev_line: u32 = 0;
    let mut prev_start: u32 = 0;

    for tok in root
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
    {
        let Some((ty, modifiers)) = classify_token(&tok) else {
            continue;
        };

        // LSP forbids tokens spanning multiple lines. Split block comments,
        // multi-line strings, etc. into one token per line.
        let off: usize = tok.text_range().start().into();
        let text = tok.text();
        let mut line_off = off;
        for line in text.split_inclusive('\n') {
            // Trim the trailing `\n` (and `\r`) so we don't include them in
            // the token length — they belong between lines.
            let mut visible_len = line.len();
            if visible_len > 0 && line.as_bytes()[visible_len - 1] == b'\n' {
                visible_len -= 1;
            }
            if visible_len > 0 && line.as_bytes()[visible_len - 1] == b'\r' {
                visible_len -= 1;
            }
            let visible = &line[..visible_len];

            let length: u32 = visible.chars().map(|c| c.len_utf16() as u32).sum();
            if length > 0 {
                let pos = doc.position_of(line_off);
                let delta_line = pos.line - prev_line;
                let delta_start = if delta_line == 0 {
                    pos.character - prev_start
                } else {
                    pos.character
                };
                out.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length,
                    token_type: ty,
                    token_modifiers_bitset: modifiers,
                });
                prev_line = pos.line;
                prev_start = pos.character;
            }
            line_off += line.len();
        }
    }

    out
}



/// Classify a single token. Returns `(type_index, modifiers_bitset)` or
/// `None` if the token shouldn't be highlighted (whitespace, structural
/// punctuation we leave to the editor's grammar).
fn classify_token(tok: &SyntaxToken) -> Option<(u32, u32)> {
    use SyntaxKind::*;
    let kind = tok.kind();
    match kind {
        // Trivia / structure — let the editor's grammar handle these.
        Whitespace | Newline | LParen | RParen | LBracket | RBracket
        | LBrace | RBrace | Semi | Error => None,

        LineComment | CommentBlock | Shebang => Some((TYPE_COMMENT, 0)),

        String => Some((TYPE_STRING, 0)),
        Symbol | FileSymbol => Some((TYPE_STRING, 0)),

        Integer | Float | Timestamp | Date | Month | Guid | Timespan
        | Datetime | Minute | Second | Time | ByteList => Some((TYPE_NUMBER, 0)),

        DslLine | SystemCmd | Exit => Some((TYPE_KEYWORD, 0)),

        // Operator tokens.
        Colon | ColonColon | CompoundAssign | Plus | Minus | Star | Slash
        | Backslash | Percent | Bang | Amp | Pipe | Caret | Hash
        | Underscore | Tilde | Dollar | Query | At | Dot | Comma | Eq
        | Lt | Gt | NotEq | LtEq | GtEq | Each | EachPrior | EachLeft
        | EachRight => Some((TYPE_OPERATOR, 0)),

        Ident | DottedIdent => Some(classify_ident(tok)),

        // Anything else (a syntax-node kind not expected as a leaf) — skip.
        _ => None,
    }
}

fn classify_ident(tok: &SyntaxToken) -> (u32, u32) {
    let text = tok.text();

    // Built-ins (verbs, qSQL keywords, control words, namespaces).
    if is_builtin(text) {
        return (TYPE_FUNCTION, MOD_DEFAULT_LIBRARY);
    }

    let parent = tok.parent();
    let parent_kind = parent.as_ref().map(|p| p.kind());

    // Bare namespace prefix (e.g. `.app` in `.app.cfg` parsed as Namespace).
    if parent_kind == Some(SyntaxKind::Namespace) {
        return (TYPE_NAMESPACE, 0);
    }

    // Lambda parameter declaration (inside ParamList).
    if let Some(p) = parent.as_ref()
        && p.ancestors().any(|n| n.kind() == SyntaxKind::ParamList)
    {
        return (TYPE_PARAMETER, MOD_DECLARATION);
    }

    // Assignment LHS = declaration. The token's grandparent is a BinExpr
    // whose LHS is the IdentExpr containing this token.
    let is_decl = parent
        .as_ref()
        .and_then(|p| p.parent())
        .is_some_and(|gp| {
            gp.kind() == SyntaxKind::BinExpr
                && gp.first_child().as_ref() == parent.as_ref()
                && gp.children_with_tokens()
                    .filter_map(|el| el.into_token())
                    .any(|t| matches!(t.kind(), SyntaxKind::Colon | SyntaxKind::ColonColon))
        });

    let mods = if is_decl { MOD_DECLARATION } else { 0 };
    (TYPE_VARIABLE, mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_is_function() {
        let doc = Document::new("count x".to_string(), 0);
        let toks = semantic_tokens(&doc);
        // First semantic token is `count` (function/builtin).
        assert_eq!(toks[0].token_type, TYPE_FUNCTION);
        assert_eq!(toks[0].token_modifiers_bitset, MOD_DEFAULT_LIBRARY);
    }

    #[test]
    fn lambda_param_is_parameter() {
        let doc = Document::new("f:{[p] p+1}".to_string(), 0);
        let toks = semantic_tokens(&doc);
        // Find the token with PARAMETER type — the param `p`.
        assert!(toks.iter().any(|t| t.token_type == TYPE_PARAMETER));
    }

    #[test]
    fn assignment_lhs_has_declaration() {
        let doc = Document::new("foo:42".to_string(), 0);
        let toks = semantic_tokens(&doc);
        let first = &toks[0];
        assert_eq!(first.token_type, TYPE_VARIABLE);
        assert_eq!(first.token_modifiers_bitset & MOD_DECLARATION, MOD_DECLARATION);
    }

    #[test]
    fn multi_line_comment_block_split_per_line() {
        // q block comment: `/` opens, `\` closes, body lines in between.
        let src = "/\nblock\nstuff\n\\\na:1";
        let doc = Document::new(src.to_string(), 0);
        let toks = semantic_tokens(&doc);

        // Reconstruct absolute (line, start_col, length) for each comment
        // token from the delta encoding so we can spec-check each one.
        let mut line: u32 = 0;
        let mut col: u32 = 0;
        let mut comment_spans: Vec<(u32, u32, u32)> = Vec::new();
        for t in &toks {
            line += t.delta_line;
            col = if t.delta_line == 0 { col + t.delta_start } else { t.delta_start };
            if t.token_type == TYPE_COMMENT {
                comment_spans.push((line, col, t.length));
            }
        }
        // 4 visible lines: "/", "block", "stuff", "\\".
        assert_eq!(comment_spans.len(), 4, "got: {comment_spans:?}");
        // LSP forbids tokens that cross a newline — every token's end-col
        // (start + length, in UTF-16 units) must fit in its own source line.
        let lines: Vec<&str> = src.split('\n').collect();
        for &(ln, start, length) in &comment_spans {
            let line_text = lines[ln as usize];
            let line_utf16: u32 = line_text.chars().map(|c| c.len_utf16() as u32).sum();
            assert!(
                start + length <= line_utf16,
                "token at line {ln} col {start} len {length} runs past line end ({line_utf16})"
            );
        }
    }

    #[test]
    fn delta_encoding_resets_per_line() {
        let doc = Document::new("a:1\nb:2".to_string(), 0);
        let toks = semantic_tokens(&doc);
        // a → delta_line=0, delta_start=0
        // : on same line at col 1 (op token)
        // 1 (number) at col 2
        // b → delta_line=1, delta_start=0 (resets)
        let b_idx = toks
            .iter()
            .position(|t| t.delta_line == 1)
            .expect("found newline-separated token");
        assert_eq!(toks[b_idx].delta_start, 0);
    }

    #[test]
    fn legend_indices_match_consts() {
        // Sanity: the Function variant maps to index 0 etc. If anyone
        // reorders TYPES this test catches it.
        assert_eq!(TYPES[TYPE_FUNCTION as usize], SemanticTokenType::FUNCTION);
        assert_eq!(TYPES[TYPE_VARIABLE as usize], SemanticTokenType::VARIABLE);
        assert_eq!(TYPES[TYPE_PARAMETER as usize], SemanticTokenType::PARAMETER);
        assert_eq!(TYPES[TYPE_NAMESPACE as usize], SemanticTokenType::NAMESPACE);
        assert_eq!(TYPES[TYPE_KEYWORD as usize], SemanticTokenType::KEYWORD);
        assert_eq!(TYPES[TYPE_STRING as usize], SemanticTokenType::STRING);
        assert_eq!(TYPES[TYPE_NUMBER as usize], SemanticTokenType::NUMBER);
        assert_eq!(TYPES[TYPE_COMMENT as usize], SemanticTokenType::COMMENT);
        assert_eq!(TYPES[TYPE_OPERATOR as usize], SemanticTokenType::OPERATOR);
    }

}
