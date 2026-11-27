use crate::binder::Symbol;
use crate::builtins::{OPERATORS, lookup_doc};
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;
use q_parser::SyntaxKind;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

pub fn hover(doc: &Document, pos: Position, workspace: &WorkspaceIndex) -> Option<Hover> {
    let offset = doc.offset_of(pos);
    let markdown = |value: String| {
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        })
    };
    let Some(name) = doc.bindings().name_at(offset) else {
        // Not a name: a builtin's or operator's documentation.
        let token = doc.token_at(offset)?;
        let text = token.text();
        return match token.kind() {
            SyntaxKind::Ident | SyntaxKind::DottedIdent => {
                lookup_doc(text).and_then(|detail| markdown(format!("**{text}** - {detail}")))
            }
            _ => operator_doc(text).and_then(markdown),
        };
    };
    let text = &doc.text()[name.range];
    match &name.symbol {
        Symbol::Local { .. } => markdown(format!("**`{text}`**")),
        Symbol::Global(q) if !doc.bindings().defs(&name.symbol).is_empty() => {
            markdown(if q == text {
                format!("**`{text}`**")
            } else {
                format!("`{text}` → **`{q}`**")
            })
        }
        Symbol::Global(q) => workspace
            .resolve_global(q)
            .and_then(|_| markdown(format!("**`{q}`** *(workspace)*"))),
    }
}

fn operator_doc(op: &str) -> Option<String> {
    let (_, doc) = OPERATORS.iter().find(|(o, _)| *o == op)?;
    Some(format!("**`{op}`** - {doc}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn hover_text(src: &str, cursor: usize) -> Option<String> {
        let doc = Document::new(src.to_string(), 0);
        let pos = doc.position_of(cursor);
        hover(&doc, pos, &WorkspaceIndex::default()).map(|h| match h.contents {
            HoverContents::Markup(m) => m.value,
            _ => String::new(),
        })
    }

    /// `(case, exact hover text)`; `«` marks the cursor.
    #[rustfmt::skip]
    const CASES: &[(&str, Option<&str>)] = &[
        ("«count x", Some("**count** - Count elements")),             // builtin doc
        ("a «: 1", Some("**`:`** - Assign (dyadic) / Identity (monadic)")),
        ("read0 `«:f.txt", None),                                     // `:` of a symbol isn't an operator
        ("f:1; «f", Some("**`f`**")),
        ("«ghost", None),
        ("f:1 / «f", None),                                            // comments and strings
        ("f:1; \"«f\"", None),                                          // hold no names
        ("\\d .cache\n«cache:1\n\\d .", Some("`cache` → **`.cache.cache`**")), // namespace-qualified
        ("\\d .cache\ncache:1\n\\d .\n`«.cache.cache upsert 2", Some("**`.cache.cache`**")),
        ("\\d .cache\nid:0j\ncache:([«id:`u#`long$()] size:`long$())\n\\d .", None), // a column, not a name
        ("\\d .cache\nid:0j\nadd:{[id] «id+1}\n\\d .", Some("**`id`**")), // param shadows .cache.id
    ];

    #[test]
    fn hover_cases() {
        for (case, want) in CASES {
            let (src, cursor, _) = crate::workspace_index::tests::markers(case);
            assert_eq!(hover_text(&src, cursor).as_deref(), *want, "case {case:?}");
        }
    }

    #[test]
    fn workspace_cross_file_hover_shows_name() {
        use crate::workspace_index::WorkspaceIndex;
        // torq.q defines .proc.cp; cache.q references it
        let torq_src = "\\d .proc\n$[1b;[cp:{.z.p}];[cp:{.z.P}]];\n\\d .";
        let torq_uri: tower_lsp_server::ls_types::Uri = "file:///TorQ/torq.q".parse().unwrap();
        let mut idx = WorkspaceIndex::default();
        idx.insert(torq_uri, Document::new(torq_src.to_string(), 0));

        let cache_src = "\\d .cache\nadd:{[f] now:.proc.cp[];now}\n\\d .";
        let cache_doc = Document::new(cache_src.to_string(), 0);
        let offset = cache_src.find(".proc.cp").unwrap();
        let pos = cache_doc.position_of(offset);

        let result = hover(&cache_doc, pos, &idx);
        let text = result
            .map(|h| match h.contents {
                HoverContents::Markup(m) => m.value,
                _ => String::new(),
            })
            .unwrap_or_default();
        assert!(
            text.contains(".proc.cp"),
            "expected .proc.cp in hover; got: {text}"
        );
    }
}
