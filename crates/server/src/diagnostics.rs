use tower_lsp::lsp_types::*;
use q_parser::SyntaxKind;
use crate::document::Document;

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
        let parent = match tok.parent() {
            Some(p) => p,
            None => continue,
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn warns_for(src: &str) -> Vec<String> {
        let doc = Document::new(src.to_string(), 0);
        unindented_close_warnings(&doc).into_iter().map(|d| d.message).collect()
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
}
