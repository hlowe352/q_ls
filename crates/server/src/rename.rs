//! `textDocument/rename` and `textDocument/prepareRename`.
//!
//! Reuses [`crate::references::find_references`] to locate every occurrence
//! of the symbol under the cursor across the workspace, then emits a
//! `WorkspaceEdit` mapping each file URI to its `TextEdit`s.

use std::collections::HashMap;
use tower_lsp_server::ls_types::{
    Position, PrepareRenameResponse, Range, TextEdit, Uri, WorkspaceEdit,
};

use crate::builtins::is_builtin;
use crate::document::Document;
use crate::references::find_references;
use crate::workspace_index::WorkspaceIndex;

/// Validate that the cursor sits on a renameable (non-builtin) identifier
/// and return its current text + range.
pub fn prepare_rename(doc: &Document, pos: Position) -> Option<PrepareRenameResponse> {
    let (name, start, end) = doc.ident_at(doc.offset_of(pos))?;
    (!is_builtin(name)).then(|| PrepareRenameResponse::RangeWithPlaceholder {
        range: Range::new(doc.position_of(start), doc.position_of(end)),
        placeholder: name.to_string(),
    })
}

/// Build the `WorkspaceEdit` renaming the symbol at `pos` to `new_name`
/// across the workspace. `None` if the cursor isn't on a renameable
/// identifier or `new_name` isn't a valid q identifier.
pub fn rename(
    doc: &Document,
    pos: Position,
    new_name: &str,
    uri: &Uri,
    workspace: &WorkspaceIndex,
) -> Option<WorkspaceEdit> {
    let cursor = doc.offset_of(pos);
    let (old_name, _, _) = doc.ident_at(cursor)?;
    if is_builtin(old_name) || !is_valid_identifier(new_name) {
        return None;
    }

    // Qualified form (`.cache.cache` for `cache` inside `\d .cache`) and its
    // namespace prefix (`.cache.`; empty for bare names).
    let qualified_old = doc
        .sym_table()
        .qualified_for(cursor, old_name)
        .map_or_else(|| old_name.to_string(), |q| q.to_string());
    let ns_prefix = qualified_old
        .rsplit_once('.')
        .map_or(String::new(), |(prefix, _)| format!("{prefix}."));

    let locations = find_references(doc, pos, true, uri, workspace);
    if locations.is_empty() {
        return None;
    }

    // Shape each replacement after the original token text:
    // `cache` → `new`, `.cache.cache` → `.cache.new`, and a leading
    // backtick on symbols is preserved.
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        let source = if &loc.uri == uri {
            Some(doc)
        } else {
            workspace.get(&loc.uri)
        };
        let original = source.and_then(|d| {
            d.text()
                .get(d.offset_of(loc.range.start)..d.offset_of(loc.range.end))
        });
        let (tick, body) = original.map_or(("", ""), |o| {
            o.strip_prefix('`').map_or(("", o), |body| ("`", body))
        });
        let new_text = if body == qualified_old && body != old_name {
            format!("{tick}{ns_prefix}{new_name}")
        } else {
            format!("{tick}{new_name}")
        };
        changes.entry(loc.uri.clone()).or_default().push(TextEdit {
            range: loc.range,
            new_text,
        });
    }

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

/// Match q's identifier shape: starts with a letter (or `.` for namespaced
/// names), continues with alphanumerics/underscores/dots.
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '.')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::tests::{doc, index, uri};

    /// New text of each single-file edit, in source order, renaming the
    /// identifier at byte `cursor` to `new_name`.
    fn new_texts(src: &str, cursor: usize, new_name: &str) -> Option<Vec<String>> {
        let d = doc(src);
        let pos = d.position_of(cursor);
        let edit = rename(&d, pos, new_name, &uri("x.q"), &WorkspaceIndex::default())?;
        let mut edits = edit.changes?.remove(&uri("x.q"))?;
        edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
        Some(edits.into_iter().map(|e| e.new_text).collect())
    }

    /// Which sites are renamed is `find_references`' job (tested there);
    /// rename must rewrite each site in its own shape.
    #[test]
    fn rewrites_each_site_in_its_shape() {
        let cases: &[(&str, &[&str])] = &[
            ("foo:1; bar:foo+foo", &["tbl", "tbl", "tbl"]),
            // backtick symbol keeps its tick and namespace
            (
                "\\d .cache\ncache:1\nf:{`.cache.cache upsert (1;2)}",
                &["tbl", "`.cache.tbl"],
            ),
            // dotted reference keeps its namespace prefix
            (
                "\\d .cache\ncache:1\n\\d .\nshow .cache.cache",
                &["tbl", ".cache.tbl"],
            ),
        ];
        for (src, want) in cases {
            let cursor = src.find("foo:").or_else(|| src.find("cache:1")).unwrap();
            assert_eq!(
                new_texts(src, cursor, "tbl").unwrap(),
                *want,
                "source {src:?}"
            );
        }
    }

    #[test]
    fn refuses_builtins_and_invalid_names() {
        let builtin = doc("count x");
        assert!(prepare_rename(&builtin, Position::new(0, 0)).is_none());
        assert!(new_texts("count x", 0, "tally").is_none());
        for bad in ["1bad", "with space", ""] {
            assert!(new_texts("foo:1", 0, bad).is_none(), "{bad:?} accepted");
        }
        for good in [".ns.x", "a1_b"] {
            assert!(new_texts("foo:1", 0, good).is_some(), "{good:?} rejected");
        }
    }

    #[test]
    fn prepare_returns_identifier_range() {
        let d = doc("foo:1; foo+1");
        let resp = prepare_rename(&d, d.position_of(8)).expect("prepare ok");
        let PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } = resp else {
            panic!("expected RangeWithPlaceholder");
        };
        assert_eq!(placeholder, "foo");
        assert_eq!(range, Range::new(Position::new(0, 7), Position::new(0, 10)));
    }

    /// Renaming from the definition rewrites uses in other files. (From a
    /// use site is covered end-to-end in tests/lsp.rs.)
    #[test]
    fn rewrites_uses_in_other_files() {
        let (a, b) = (uri("a.q"), uri("b.q"));
        let idx = index(&[("b.q", "sharedFn 99")]);
        let edit = rename(
            &doc("sharedFn:{x+1}"),
            Position::new(0, 0),
            "newFn",
            &a,
            &idx,
        )
        .unwrap();
        let changes = edit.changes.unwrap();
        assert_eq!(changes[&a][0].new_text, "newFn");
        assert_eq!(changes[&b][0].new_text, "newFn");
    }
}
