use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode, SyntaxToken, TextSize};
use crate::document::Document;

pub fn goto_definition(doc: &Document, pos: Position, uri: &Url) -> Option<GotoDefinitionResponse> {
    let offset = doc.offset_of(pos);
    let target_name = get_identifier_at(doc.text(), offset)?;
    let root = doc.parse().syntax();

    let def_offset = resolve_definition(&root, offset, &target_name)?;
    let def_pos = doc.position_of(def_offset);

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: Range::new(def_pos, def_pos),
    }))
}

/// Resolve `name` against the lexical scope of the cursor at `cursor_off`.
///
/// Order:
/// 1. Innermost-to-outermost enclosing `Lambda`:
///    a. Lambda's `ParamList` — direct match.
///    b. Last plain `name:` assignment inside the lambda body (skipping
///       nested lambdas), occurring strictly before `cursor_off`.
/// 2. Last *global* binding visible from anywhere in the file:
///    - any `name::` assignment,
///    - any dotted assignment whose LHS text equals `name` (e.g.
///      `.foo.bar: 1` matches lookup of `.foo.bar`),
///    - any plain `name:` assignment that is **not** nested inside a
///      `Lambda` (top-level).
///   Takes the last such hit before `cursor_off`.
fn resolve_definition(root: &SyntaxNode, cursor_off: usize, name: &str) -> Option<usize> {
    if let Some(token) = leaf_token_at(root, cursor_off) {
        for lambda in token
            .parent_ancestors()
            .filter(|n| n.kind() == SyntaxKind::Lambda)
        {
            if let Some(off) = param_offset(&lambda, name) {
                return Some(off);
            }
            if let Some(off) = last_local_assignment_before(&lambda, name, cursor_off) {
                return Some(off);
            }
        }
    }
    last_global_assignment_before(root, name, cursor_off)
}

fn leaf_token_at(root: &SyntaxNode, offset: usize) -> Option<SyntaxToken> {
    let off = TextSize::from(offset as u32);
    root.token_at_offset(off).right_biased()
        .or_else(|| root.token_at_offset(off).left_biased())
}

/// If `name` is in this lambda's `ParamList`, return its offset.
fn param_offset(lambda: &SyntaxNode, name: &str) -> Option<usize> {
    let plist = lambda.children().find(|c| c.kind() == SyntaxKind::ParamList)?;
    plist
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| t.kind() == SyntaxKind::Ident && t.text() == name)
        .map(|t| t.text_range().start().into())
}

/// Last plain `name:` assignment in `lambda`'s body, skipping nested
/// lambdas, occurring strictly before `cursor_off`.
///
/// "Plain" excludes `name::` (global) and dotted `.foo.bar:` (global) —
/// those bindings are visible to outer scopes too and are handled by the
/// global lookup pass.
fn last_local_assignment_before(
    lambda: &SyntaxNode,
    name: &str,
    cursor_off: usize,
) -> Option<usize> {
    fn visit(node: &SyntaxNode, name: &str, cursor_off: usize, best: &mut Option<usize>) {
        for child in node.children() {
            if child.kind() == SyntaxKind::Lambda {
                continue;
            }
            if child.kind() == SyntaxKind::BinExpr
                && let Some(info) = assignment_info(&child)
                && info.is_plain_local()
                && info.name == name
                && info.lhs_off < cursor_off
            {
                *best = Some(info.lhs_off);
            }
            visit(&child, name, cursor_off, best);
        }
    }
    let mut best = None;
    visit(lambda, name, cursor_off, &mut best);
    best
}

/// Last globally-visible binding before `cursor_off`. Globals are:
/// `::` assignments anywhere, dotted-name assignments anywhere, and plain
/// `name:` assignments that are not inside any `Lambda`.
fn last_global_assignment_before(
    root: &SyntaxNode,
    name: &str,
    cursor_off: usize,
) -> Option<usize> {
    let mut best = None;
    for node in root.descendants() {
        if node.kind() != SyntaxKind::BinExpr {
            continue;
        }
        let Some(info) = assignment_info(&node) else { continue };
        if info.name != name || info.lhs_off >= cursor_off {
            continue;
        }
        let is_global = info.is_double_colon
            || info.is_dotted
            || !is_inside_lambda(&node);
        if is_global {
            best = Some(info.lhs_off);
        }
    }
    best
}

fn is_inside_lambda(node: &SyntaxNode) -> bool {
    node.ancestors().skip(1).any(|n| n.kind() == SyntaxKind::Lambda)
}

struct AssignInfo {
    name: String,
    lhs_off: usize,
    is_double_colon: bool,
    is_dotted: bool,
}

impl AssignInfo {
    fn is_plain_local(&self) -> bool {
        !self.is_double_colon && !self.is_dotted
    }
}

/// Inspect a `BinExpr`. If it's an assignment (`:` / `::`) with a single
/// Ident or DottedIdent on the LHS, return its info.
fn assignment_info(bin: &SyntaxNode) -> Option<AssignInfo> {
    let op = bin
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| t.kind() == SyntaxKind::Colon || t.kind() == SyntaxKind::ColonColon)?;
    let is_double_colon = op.kind() == SyntaxKind::ColonColon;

    let lhs = bin.first_child()?;
    let token = lhs
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| !t.kind().is_trivia())?;

    let kind = token.kind();
    if kind != SyntaxKind::Ident && kind != SyntaxKind::DottedIdent {
        return None;
    }
    Some(AssignInfo {
        name: token.text().to_string(),
        lhs_off: token.text_range().start().into(),
        is_double_colon,
        is_dotted: kind == SyntaxKind::DottedIdent,
    })
}

fn get_identifier_at(text: &str, offset: usize) -> Option<String> {
    if offset >= text.len() { return None; }
    let bytes = text.as_bytes();
    let mut start = offset;
    let mut end = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) { start -= 1; }
    while end < bytes.len() && is_ident_char(bytes[end]) { end += 1; }
    if start == end { return None; }
    Some(text[start..end].to_string())
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def_offset(src: &str, cursor_byte: usize, name: &str) -> Option<usize> {
        let doc = Document::new(src.to_string(), 0);
        let root = doc.parse().syntax();
        resolve_definition(&root, cursor_byte, name)
    }

    #[test]
    fn lambda_param_wins_over_top_level() {
        let src = "fn:99;\nf:{[fn] fn+1}";
        let cursor = src.find("fn+1").unwrap();
        let off = def_offset(src, cursor, "fn").expect("found");
        let expected = src.find("[fn]").unwrap() + 1;
        assert_eq!(off, expected);
    }

    #[test]
    fn local_let_wins_over_top_level() {
        let src = "fn:99;\nf:{x:42; x+fn}";
        let cursor = src.find("x+fn").unwrap();
        let off = def_offset(src, cursor, "x").expect("found");
        let expected = src.find("x:42").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn top_level_resolves_when_outside_lambda() {
        let src = "fn:99;\nbar:fn";
        let cursor = src.rfind("fn").unwrap();
        let off = def_offset(src, cursor, "fn").expect("found");
        let expected = src.find("fn:99").unwrap();
        assert_eq!(off, expected);
    }

    #[test]
    fn nested_lambda_param_shadows_outer_param() {
        let src = "f:{[fn] g:{[fn] fn+1}; fn}";
        let cursor = src.find("fn+1").unwrap();
        let off = def_offset(src, cursor, "fn").expect("found");
        let inner = src.find("g:{[fn]").unwrap() + "g:{[".len();
        assert_eq!(off, inner);
    }

    /// A reference inside lambda A must NOT resolve to an unrelated local
    /// binding inside lambda B at the same level.
    #[test]
    fn local_in_other_function_is_not_visible() {
        let src = "f:{[x] fn:99; x};\ng:{[y] fn+y}";
        let cursor = src.find("fn+y").unwrap();
        let off = def_offset(src, cursor, "fn");
        assert!(off.is_none(), "must not see fn from f's body, got {off:?}");
    }

    /// Picks the most recent rebinding before the cursor.
    #[test]
    fn last_occurrence_wins() {
        let src = "a:1;\na:2;\na";
        let cursor = src.rfind('a').unwrap();
        let off = def_offset(src, cursor, "a").expect("found");
        let expected = src.find("a:2").unwrap();
        assert_eq!(off, expected);
    }

    /// Forward references don't resolve — definition must precede use.
    #[test]
    fn forward_reference_unresolved() {
        let src = "a;\na:1";
        let cursor = src.find('a').unwrap();
        let off = def_offset(src, cursor, "a");
        assert!(off.is_none(), "forward ref should be unresolved, got {off:?}");
    }

    /// `.ns.var:` inside a lambda is a global — visible from outside.
    #[test]
    fn dotted_assignment_inside_lambda_is_global() {
        let src = "init:{.app.cfg:1};\nuse:.app.cfg";
        let cursor = src.rfind(".app.cfg").unwrap();
        let off = def_offset(src, cursor, ".app.cfg").expect("found");
        let expected = src.find(".app.cfg:1").unwrap();
        assert_eq!(off, expected);
    }

    /// `name::` inside a lambda is a global — visible from outside.
    #[test]
    fn double_colon_inside_lambda_is_global() {
        let src = "init:{counter::5};\nuse:counter";
        let cursor = src.rfind("counter").unwrap();
        let off = def_offset(src, cursor, "counter").expect("found");
        let expected = src.find("counter::5").unwrap();
        assert_eq!(off, expected);
    }

    /// Regression: in dbmaint.q line 482, `fn` referenced inside `fn1Col`'s
    /// body must resolve to its lambda parameter, not to the unrelated
    /// `fn:` local in `castCol` further up the file.
    #[test]
    fn dbmaint_fn_resolves_to_lambda_param() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let src = std::fs::read_to_string(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../parser/tests/data/real_q/dbmaint.q",
                )).expect("dbmaint.q fixture");

                let body_marker = "newVal:fn ";
                let cursor = src.find(body_marker).unwrap() + "newVal:".len();
                let lambda_open = src.find("fn1Col:{[").unwrap();
                let param_off = src[lambda_open..].find("fn]").unwrap() + lambda_open;

                let off = def_offset(&src, cursor, "fn").expect("found");
                assert_eq!(off, param_off,
                    "expected goto-def to land on fn1Col's `fn` parameter \
                     at byte {param_off}, got {off}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
