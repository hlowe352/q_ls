use crate::binder::Symbol;
use crate::config::Config;
use crate::document::Document;
use crate::syntax::tokens;
use crate::workspace_index::WorkspaceIndex;
use q_parser::SyntaxKind;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

pub fn compute_diagnostics(
    doc: &Document,
    workspace: &WorkspaceIndex,
    config: &Config,
) -> Vec<Diagnostic> {
    let errors = doc.parse().errors.iter().map(|err| {
        diagnostic(
            doc,
            err.offset,
            err.len,
            DiagnosticSeverity::ERROR,
            err.message.clone(),
        )
    });
    errors
        .chain(unindented_close_warnings(doc))
        .chain(unresolved_reference_warnings(doc, workspace, config))
        .collect()
}

fn diagnostic(
    doc: &Document,
    offset: usize,
    len: usize,
    severity: DiagnosticSeverity,
    message: String,
) -> Diagnostic {
    Diagnostic {
        range: Range::new(doc.position_of(offset), doc.position_of(offset + len)),
        severity: Some(severity),
        source: Some("q-ls".into()),
        message,
        ..Default::default()
    }
}

/// Warn on closing brackets at column 0 inside a multi-line construct.
///
/// q's line-continuation rule makes a non-indented line a new statement,
/// so a `}`, `]`, or `)` flush left silently terminates whatever scope it
/// was meant to close. The parser may still recover, but the resulting
/// AST diverges from intent and real `q` will reject the file.
fn unindented_close_warnings(doc: &Document) -> impl Iterator<Item = Diagnostic> + '_ {
    let src = doc.text();
    tokens(&doc.parse().syntax()).filter_map(move |tok| {
        if !matches!(
            tok.kind(),
            SyntaxKind::RBrace | SyntaxKind::RBracket | SyntaxKind::RParen
        ) {
            return None;
        }
        let off: usize = tok.text_range().start().into();
        let at_col_zero = off == 0 || src.as_bytes()[off - 1] == b'\n';
        // Only closes of scopes that span multiple lines.
        let open: usize = tok.parent()?.text_range().start().into();
        if !at_col_zero || !src[open..off].contains('\n') {
            return None;
        }
        Some(diagnostic(
            doc,
            off,
            tok.text().len(),
            DiagnosticSeverity::WARNING,
            format!(
                "closing `{}` at column 0 ends the surrounding scope as a new statement; \
                 q expects multi-line closes to be indented",
                tok.text()
            ),
        ))
    })
}

/// Warn on uses of a global defined neither in this file nor in the
/// workspace, unless the config's `suppress_unresolved` names it. A
/// namespace (`.u`) is defined once anything under it is (`.u.x`).
fn unresolved_reference_warnings<'a>(
    doc: &'a Document,
    workspace: &'a WorkspaceIndex,
    config: &'a Config,
) -> impl Iterator<Item = Diagnostic> + 'a {
    let bindings = doc.bindings();
    bindings.names().iter().filter_map(move |name| {
        let Symbol::Global(qualified) = &name.symbol else {
            return None;
        };
        let text = &doc.text()[name.range];
        let resolved = name.is_def()
            || !bindings.defs(&name.symbol).is_empty()
            || workspace.resolve_global(qualified).is_some()
            || config.suppress_unresolved.contains(text)
            || (qualified.starts_with('.') && {
                let member = format!("{qualified}.");
                let mut defined = bindings
                    .globals()
                    .map(|(n, _)| n)
                    .chain(workspace.all_idents());
                defined.any(|n| n.starts_with(&member))
            });
        (!resolved).then(|| {
            diagnostic(
                doc,
                name.range.start().into(),
                text.len(),
                DiagnosticSeverity::WARNING,
                format!("unresolved reference `{text}`"),
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names flagged as unresolved in `src`, in source order.
    fn unresolved(src: &str) -> Vec<String> {
        let doc = Document::new(src.to_string(), 0);
        let diags = compute_diagnostics(&doc, &WorkspaceIndex::default(), &Config::default());
        diags
            .into_iter()
            .filter_map(|d| {
                let name = d.message.strip_prefix("unresolved reference `")?;
                Some(name.trim_end_matches('`').to_string())
            })
            .collect()
    }

    /// `(source, exact unresolved names)`. Cross-file resolution and the
    /// `suppress_unresolved` config are covered end-to-end in tests/lsp.rs.
    #[rustfmt::skip]
    const UNRESOLVED: &[(&str, &[&str])] = &[
        ("f:{[x] x+y}", &["y"]),
        ("f:{[x] count x}", &[]),                           // builtin
        ("f:{[x] x+1}", &[]),                               // param
        ("f:1; f", &[]),                                    // top-level def
        ("newName: 42", &[]),                               // assignment LHS is a def
        ("f:{t:1; g:{t+1}; g[]}", &["t"]),                  // no closures
        ("select sym, px from t", &["t"]),                  // columns aren't names; `t` is
        ("t:([]sym:`a;px:1); select sym, px from t", &[]),
        ("evict:{[r] select lastaccess,id,size from cache}", &["cache"]),
        (".q.id .Q.dd .z.s .q .z", &[]),                    // builtin namespaces
        (".u.x:1; show .u", &[]),                           // a namespace with members
        ("show .u", &[".u"]),
        ("use:.app.cfg", &[".app.cfg"]),
        (".app.cfg:1; use:.app.cfg", &[]),
        ("system \"d .test\"\nval:42\nshow .test.val", &[]),
        ("\\d .cache\ncache:1\n\\d .\nshow .cache.cache", &[]),
        ("\\d .foo\nval:42\n\\d .\nshow val", &["val"]),    // back at root: `val` is `.foo.val`
        ("\\d .cache\ncache:([id:`u#`long$()] )\nfuncs:(`u#`long$())!()\n\\d .\nuse:.cache.cache", &[]),
        ("loadf:loadf0[0b]   /load a file if it hasn't been loaded", &["loadf0"]), // not comment words
    ];

    #[test]
    fn unresolved_references() {
        for (src, want) in UNRESOLVED {
            assert_eq!(unresolved(src), *want, "source {src:?}");
        }
    }

    /// `(source, closer expected to be flagged at column 0)`.
    const UNINDENTED_CLOSE: &[(&str, Option<&str>)] = &[
        ("f:{[x]\n    x+1\n}\n", Some("}")),
        ("f[1;\n2;\n3\n]\n", Some("]")),
        ("(1;\n2;\n3\n)\n", Some(")")),
        ("f:{[x]\n    x+1\n  }\n", None), // indented
        ("{x+1}\n", None),                // single line
    ];

    #[test]
    fn unindented_close_warnings() {
        for (src, want) in UNINDENTED_CLOSE {
            let doc = Document::new((*src).to_string(), 0);
            let got: Vec<String> = super::unindented_close_warnings(&doc)
                .map(|d| d.message)
                .collect();
            match want {
                Some(close) => assert!(
                    got.len() == 1 && got[0].contains(&format!("closing `{close}`")),
                    "source {src:?}: {got:?}"
                ),
                None => assert!(got.is_empty(), "source {src:?}: {got:?}"),
            }
        }
    }

    /// dbmaint.q is real q: it must produce no unresolved-name false
    /// positives. Wide stack: rowan's recursive `Drop` overflows the default
    /// 2 MB test thread on this file.
    #[test]
    fn unresolved_dbmaint_noise_floor() {
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let src = include_str!("../../parser/tests/data/real_q/dbmaint.q");
                assert_eq!(unresolved(src), Vec::<String>::new());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
