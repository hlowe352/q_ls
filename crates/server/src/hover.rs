use crate::builtins::{OPERATORS, lookup_doc};
use crate::document::Document;
use crate::syntax::has_ancestor;
use crate::workspace_index::WorkspaceIndex;
use q_parser::{SyntaxKind, TextSize};
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

pub fn hover(doc: &Document, pos: Position, workspace: &WorkspaceIndex) -> Option<Hover> {
    let offset = doc.offset_of(pos);
    let text = doc.text();
    if offset >= text.len() {
        return None;
    }
    let markdown = |value: String| {
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        })
    };

    // Text after a backtick is a symbol (`` `.cache.cache ``, `` `:file ``):
    // never a builtin or operator.
    let after_tick = |i: usize| i > 0 && text.as_bytes()[i - 1] == b'`';
    let Some((name, start, _)) = doc.ident_at(offset) else {
        // No identifier under the cursor: maybe a single-char operator.
        let op = text[offset..].chars().next()?.to_string();
        return (!after_tick(offset))
            .then(|| operator_doc(&op))
            .flatten()
            .and_then(markdown);
    };
    if !after_tick(start) {
        if let Some(detail) = lookup_doc(name) {
            return markdown(format!("**{name}** - {detail}"));
        }
        if let Some(doc_str) = operator_doc(name) {
            return markdown(doc_str);
        }
    }

    let table = doc.sym_table();
    if table.resolve(offset, name).is_some() {
        // Column definitions in a table constructor look like globals but
        // are column names; don't namespace-qualify them.
        let display = if cursor_in_table_expr(doc, offset) {
            None
        } else {
            table.qualified_for(offset, name)
        };
        return markdown(match display {
            Some(q) if q != name => format!("`{name}` → **`{q}`**"),
            _ => format!("**`{name}`**"),
        });
    }

    // Not resolvable in this file — try workspace globals, qualified with
    // the active namespace.
    let ns = table.active_ns_at(offset);
    let lookup = if !ns.is_empty() && !name.starts_with('.') {
        format!("{ns}.{name}")
    } else {
        name.to_string()
    };
    workspace
        .resolve_global(&lookup)
        .and_then(|_| markdown(format!("**`{lookup}`** *(workspace)*")))
}

#[allow(clippy::cast_possible_truncation)]
fn cursor_in_table_expr(doc: &Document, offset: usize) -> bool {
    let root = doc.parse().syntax();
    root.token_at_offset(TextSize::from(offset as u32))
        .left_biased()
        .and_then(|t| t.parent())
        .is_some_and(|n| has_ancestor(&n, SyntaxKind::TableExpr))
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
        ("\\d .cache\n«cache:1\n\\d .", Some("`cache` → **`.cache.cache`**")), // namespace-qualified
        ("\\d .cache\ncache:1\n\\d .\n`«.cache.cache upsert 2", Some("**`.cache.cache`**")),
        ("\\d .cache\nid:0j\ncache:([«id:`u#`long$()] size:`long$())\n\\d .", Some("**`id`**")), // column, not .cache.id
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
