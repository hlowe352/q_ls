use tower_lsp_server::ls_types::*;
use q_parser::{SyntaxKind, SyntaxNode};
use crate::document::Document;
use crate::builtins::is_builtin;

pub fn compute_diagnostics(doc: &Document) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = doc.parse().errors.iter().map(|err| {
        let start = doc.position_of(err.offset);
        let end = doc.position_of(err.offset + err.len);
        Diagnostic {
            range: Range::new(start, end),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("q-ls".into()),
            message: err.message.clone(),
            ..Default::default()
        }
    }).collect();

    out.extend(unindented_close_warnings(doc));
    out.extend(unresolved_reference_warnings(doc));
    out
}

/// Warn on closing brackets at column 0 inside a multi-line construct.
///
/// q's line-continuation rule makes a non-indented line a new statement,
/// so a `}`, `]`, or `)` flush left silently terminates whatever scope it
/// was meant to close. The parser may still recover, but the resulting
/// AST diverges from intent and real `q` will reject the file.
fn unindented_close_warnings(doc: &Document) -> Vec<Diagnostic> {
    let src = doc.text();
    let root = doc.parse().syntax();
    let mut diagnostics = Vec::new();

    for tok in root.descendants_with_tokens().filter_map(|e| e.into_token()) {
        let kind = tok.kind();
        if !matches!(kind, SyntaxKind::RBrace | SyntaxKind::RBracket | SyntaxKind::RParen) {
            continue;
        }
        let off: usize = tok.text_range().start().into();

        // Column 0?  The byte before this token must be a newline (or BOF).
        let at_col_zero = off == 0 || src.as_bytes()[off - 1] == b'\n';
        if !at_col_zero {
            continue;
        }

        // Single-line scope: skip — only flag closes that span multiple lines.
        let Some(parent) = tok.parent() else { continue };
        let open_off: usize = parent.text_range().start().into();
        if !src[open_off..off].contains('\n') {
            continue;
        }

        let pos = doc.position_of(off);
        let end = doc.position_of(off + tok.text().len());
        diagnostics.push(Diagnostic {
            range: Range::new(pos, end),
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("q-ls".into()),
            message: format!(
                "closing `{}` at column 0 ends the surrounding scope as a new statement; \
                 q expects multi-line closes to be indented",
                tok.text()
            ),
            ..Default::default()
        });
    }

    diagnostics
}

/// Warn on identifier references that don't resolve to any visible
/// definition.
///
/// Walks every `IdentExpr` / `Namespace` node, skips:
/// - assignment LHS (the definition site itself),
/// - parameter list entries (declarations, not refs),
/// - tokens inside qSQL clauses (`SelectExpr`/`UpdateExpr`/`ExecExpr`/
///   `DeleteExpr`) — column names there come from row context and aren't
///   bound by visible code,
/// - q built-ins (see [`crate::builtins`]).
///
/// For everything else, calls `SymTable::resolve` at the token's
/// position. If it returns `None`, emit a warning.
fn unresolved_reference_warnings(doc: &Document) -> Vec<Diagnostic> {
    let root = doc.parse().syntax();
    let table = doc.sym_table();
    let mut diagnostics = Vec::new();

    for node in root.descendants() {
        if !matches!(node.kind(), SyntaxKind::IdentExpr | SyntaxKind::Namespace) {
            continue;
        }
        if is_in_qsql(&node) || is_assignment_lhs(&node) || is_in_param_list(&node) {
            continue;
        }
        let Some(token) = node
            .descendants_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| !t.kind().is_trivia())
        else {
            continue;
        };
        let name = token.text();
        if is_builtin(name) {
            continue;
        }
        let off: usize = token.text_range().start().into();
        if table.resolve(off, name).is_some() {
            continue;
        }

        let pos = doc.position_of(off);
        let end = doc.position_of(off + name.len());
        diagnostics.push(Diagnostic {
            range: Range::new(pos, end),
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("q-ls".into()),
            message: format!("unresolved reference `{name}`"),
            ..Default::default()
        });
    }

    diagnostics
}

fn is_in_qsql(node: &SyntaxNode) -> bool {
    node.ancestors().any(|n| matches!(
        n.kind(),
        SyntaxKind::SelectExpr
            | SyntaxKind::UpdateExpr
            | SyntaxKind::ExecExpr
            | SyntaxKind::DeleteExpr
    ))
}

fn is_in_param_list(node: &SyntaxNode) -> bool {
    node.ancestors().any(|n| n.kind() == SyntaxKind::ParamList)
}

/// True if this `IdentExpr` is the LHS of a `BinExpr` whose operator is
/// `:` or `::` — i.e. this node *is* a definition, not a reference.
fn is_assignment_lhs(node: &SyntaxNode) -> bool {
    let Some(parent) = node.parent() else { return false; };
    if parent.kind() != SyntaxKind::BinExpr {
        return false;
    }
    if parent.first_child().as_ref() != Some(node) {
        return false;
    }
    parent
        .children_with_tokens()
        .filter_map(|el| el.into_token())
        .any(|t| t.kind() == SyntaxKind::Colon || t.kind() == SyntaxKind::ColonColon)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warns_for(src: &str) -> Vec<String> {
        let doc = Document::new(src.to_string(), 0);
        unindented_close_warnings(&doc).into_iter().map(|d| d.message).collect()
    }

    fn unresolved_for(src: &str) -> Vec<String> {
        let doc = Document::new(src.to_string(), 0);
        unresolved_reference_warnings(&doc).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn warns_on_unindented_brace_close() {
        let src = "f:{[x]\n    x+1\n}\n";
        let warnings = warns_for(src);
        assert_eq!(warnings.len(), 1, "expected 1 warning, got: {warnings:?}");
        assert!(warnings[0].contains("closing `}`"));
    }

    #[test]
    fn warns_on_unindented_bracket_close() {
        let src = "f[1;\n2;\n3\n]\n";
        let warnings = warns_for(src);
        assert!(warnings.iter().any(|w| w.contains("closing `]`")), "got: {warnings:?}");
    }

    #[test]
    fn warns_on_unindented_paren_close() {
        let src = "(1;\n2;\n3\n)\n";
        let warnings = warns_for(src);
        assert!(warnings.iter().any(|w| w.contains("closing `)`")), "got: {warnings:?}");
    }

    #[test]
    fn no_warning_when_indented() {
        let src = "f:{[x]\n    x+1\n  }\n";
        let warnings = warns_for(src);
        assert!(warnings.is_empty(), "expected no warnings, got: {warnings:?}");
    }

    #[test]
    fn no_warning_for_single_line_scope() {
        let src = "{x+1}\n";
        let warnings = warns_for(src);
        assert!(warnings.is_empty(), "expected no warnings, got: {warnings:?}");
    }

    #[test]
    fn unresolved_flags_truly_undefined_name() {
        let src = "f:{[x] x+y}";
        let warnings = unresolved_for(src);
        assert!(warnings.iter().any(|w| w.contains("`y`")), "got: {warnings:?}");
    }

    #[test]
    fn unresolved_skips_builtins() {
        let src = "f:{[x] count x}";
        let warnings = unresolved_for(src);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn unresolved_skips_param() {
        let src = "f:{[x] x+1}";
        let warnings = unresolved_for(src);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn unresolved_skips_top_level_def() {
        let src = "f:1; f";
        let warnings = unresolved_for(src);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn unresolved_skips_qsql_columns() {
        let src = "select sym, px from t";
        let warnings = unresolved_for(src);
        // `t` would normally be flagged as undefined, but it's the table arg
        // to `from` which is part of qSQL — currently we suppress all qSQL
        // refs. Adjust if/when qSQL gets tighter handling.
        assert!(warnings.iter().all(|w| !w.contains("`sym`") && !w.contains("`px`")),
            "qSQL columns must not be flagged: {warnings:?}");
    }

    #[test]
    fn unresolved_skips_assignment_lhs() {
        let src = "newName: 42";
        let warnings = unresolved_for(src);
        assert!(warnings.is_empty(), "assignment LHS is a def, got: {warnings:?}");
    }

    #[test]
    fn unresolved_skips_q_namespaces() {
        let src = ".q.id .Q.dd .z.s";
        let warnings = unresolved_for(src);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn unresolved_flags_user_namespace_member_when_undefined() {
        let src = "use:.app.cfg";
        let warnings = unresolved_for(src);
        assert!(warnings.iter().any(|w| w.contains("`.app.cfg`")), "got: {warnings:?}");
    }

    #[test]
    fn unresolved_resolves_user_namespace_when_defined() {
        let src = ".app.cfg:1; use:.app.cfg";
        let warnings = unresolved_for(src);
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    /// Sanity: dbmaint.q is real q. Surface any unresolved-name false
    /// positives so we can tune the builtin allow-list.
    ///
    /// Runs on a wide-stack thread because rowan's `GreenNode` drops
    /// recursively, and dbmaint.q nests deep enough to overflow the
    /// default 2 MB test thread stack on teardown — not a logic issue.
    #[test]
    fn unresolved_dbmaint_noise_floor() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let src = std::fs::read_to_string(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../parser/tests/data/real_q/dbmaint.q",
                )).expect("dbmaint.q fixture");
                let warnings = unresolved_for(&src);
                if !warnings.is_empty() {
                    eprintln!("dbmaint.q produced {} unresolved-ref warnings:", warnings.len());
                    for w in &warnings {
                        eprintln!("  {w}");
                    }
                }
                assert!(warnings.is_empty(),
                    "regression: dbmaint.q now reports unresolved refs: {warnings:#?}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
