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

    #[test]
    fn bare_name_in_d_block_shows_qualified() {
        let src = "\\d .cache\ncache:1\n\\d .";
        // hover on `cache` (the reference side — but here it's the def)
        let cursor = src.find("cache:1").unwrap();
        // `cache` is in `\d .cache` context so qualified_for returns .cache.cache
        // Hover is on the LHS of assignment — but hover resolves it fine
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(text.contains(".cache.cache"), "got: {text}");
    }

    #[test]
    fn backtick_symbol_resolves_to_table() {
        let src = "\\d .cache\ncache:1\n\\d .\n`.cache.cache upsert 2";
        let cursor = src.find("`.cache.cache").unwrap() + 1; // on `.cache.cache`
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(text.contains(".cache.cache"), "got: {text}");
    }

    #[test]
    fn table_col_def_not_qualified() {
        // Column names inside table constructor are not globals.
        let src = "\\d .cache\ncache:([id:`u#`long$()] size:`long$())\n\\d .";
        let cursor = src.find("id:`u").unwrap(); // hover on `id` col def
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(
            !text.contains(".cache.id"),
            "table col falsely qualified; got: {text}"
        );
    }

    #[test]
    fn param_shadowing_global_not_qualified() {
        // `id` is both a global (.cache.id) and a lambda param.
        // Hover over the param should NOT show .cache.id.
        let src = "\\d .cache\nid:0j\nadd:{[id] id+1}\n\\d .";
        let cursor = src.find("id+1").unwrap(); // ref to the param inside lambda
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(
            !text.contains(".cache.id"),
            "param falsely qualified; got: {text}"
        );
    }

    #[test]
    fn no_operator_doc_inside_symbol() {
        // `:` of a file symbol is not the assign operator; bare `:` is.
        assert_eq!(hover_text("read0 `:f.txt", 7), None);
        assert!(hover_text("a : 1", 2).is_some_and(|t| t.contains("Assign")));
    }

    #[test]
    fn builtin_still_works() {
        let src = "count x";
        let cursor = src.find("count").unwrap();
        let text = hover_text(src, cursor).unwrap_or_default();
        assert!(text.to_lowercase().contains("count"), "got: {text}");
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
