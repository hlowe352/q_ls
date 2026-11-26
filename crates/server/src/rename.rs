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
    use crate::workspace_index::tests::{doc, index};

    fn uri() -> Uri {
        crate::workspace_index::tests::uri("x.q")
    }

    /// Single-file rename edits for the doc at `pos`.
    fn edits(d: &Document, pos: Position, new_name: &str) -> Option<Vec<TextEdit>> {
        let edit = rename(d, pos, new_name, &uri(), &WorkspaceIndex::default())?;
        edit.changes?.remove(&uri())
    }

    #[test]
    fn prepare_returns_range_for_user_ident() {
        let doc = Document::new("foo:1; foo+1".to_string(), 0);
        let pos = doc.position_of(7); // on the second `foo`
        let r = prepare_rename(&doc, pos).expect("prepare ok");
        match r {
            PrepareRenameResponse::RangeWithPlaceholder { placeholder, .. } => {
                assert_eq!(placeholder, "foo");
            }
            _ => panic!("expected RangeWithPlaceholder"),
        }
    }

    #[test]
    fn prepare_refuses_builtins() {
        let doc = Document::new("count x".to_string(), 0);
        let pos = doc.position_of(0); // on `count`
        assert!(prepare_rename(&doc, pos).is_none());
    }

    #[test]
    fn rename_rewrites_all_uses() {
        let doc = Document::new("foo:1; bar:foo+foo".to_string(), 0);
        let pos = doc.position_of(0); // on `foo:` def
        let edits = edits(&doc, pos, "baz").expect("rename ok");
        assert_eq!(edits.len(), 3);
        for e in edits {
            assert_eq!(e.new_text, "baz");
        }
    }

    #[test]
    fn rename_excludes_unrelated_names() {
        // `x` in lambda param scope and outer `x:99` are different bindings.
        let doc = Document::new("x:99; f:{[x] x+1}".to_string(), 0);
        let pos = doc.position_of(0); // on outer `x`
        let edits = edits(&doc, pos, "y").expect("rename ok");
        // Just the outer x:99 def — neither the param nor `x+1` ref.
        assert_eq!(edits.len(), 1);
    }

    #[test]
    fn rename_rewrites_local_rebindings() {
        // q rebinds the same lambda-local — rename should touch all sites.
        let doc = Document::new("f:{a:1; a:2; a}".to_string(), 0);
        let src = doc.text().to_string();
        let pos = doc.position_of(src.find("a:2").unwrap());
        let edits = edits(&doc, pos, "b").expect("rename ok");
        assert_eq!(edits.len(), 3, "expected all 3 a sites, got {edits:#?}");
    }

    #[test]
    fn rename_rejects_invalid_new_name() {
        let doc = Document::new("foo:1".to_string(), 0);
        let pos = doc.position_of(0);
        for bad in ["1bad", "with space", ""] {
            assert!(edits(&doc, pos, bad).is_none(), "{bad:?} accepted");
        }
    }

    #[test]
    fn rename_preserves_backtick_on_symbol_refs() {
        // `.cache.cache upsert row` — the symbol token must keep its backtick.
        let src = "\\d .cache\ncache:1\nf:{`.cache.cache upsert (1;2)}";
        let doc = Document::new(src.to_string(), 0);
        let pos = doc.position_of(src.find("cache:1").unwrap());
        let edits = edits(&doc, pos, "tbl").expect("rename ok");
        // The symbol edit must produce `` `.cache.tbl ``, not just `tbl`.
        let sym_edit = edits
            .iter()
            .find(|e| e.new_text.starts_with('`'))
            .expect("must have a symbol edit");
        assert_eq!(sym_edit.new_text, "`.cache.tbl");
    }

    #[test]
    fn rename_preserves_dotted_prefix_on_qualified_refs() {
        // `.cache.cache` dotted-ident refs outside the namespace must
        // become `.cache.newName`, not just `newName`.
        let src = "\\d .cache\ncache:1\n\\d .\nshow .cache.cache";
        let doc = Document::new(src.to_string(), 0);
        let pos = doc.position_of(src.find("cache:1").unwrap());
        let edits = edits(&doc, pos, "tbl").expect("rename ok");
        let dotted_edit = edits
            .iter()
            .find(|e| e.new_text.starts_with('.'))
            .expect("must have a dotted edit");
        assert_eq!(dotted_edit.new_text, ".cache.tbl");
    }

    #[test]
    fn valid_identifier_shapes() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier(".ns.x"));
        assert!(is_valid_identifier("a1_b"));
        assert!(!is_valid_identifier("1foo"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("a b"));
    }

    #[test]
    fn rename_rewrites_across_files_from_def_and_use() {
        let a = crate::workspace_index::tests::uri("a.q");
        let b = crate::workspace_index::tests::uri("b.q");
        // From the def in a.q, with b.q using it.
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
        // From the use in b.q, with the def in a.q.
        let idx = index(&[("a.q", "sharedFn:{x+1}")]);
        let edit = rename(&doc("sharedFn 99"), Position::new(0, 0), "newFn", &b, &idx).unwrap();
        let changes = edit.changes.unwrap();
        assert!(
            changes.contains_key(&a) && changes.contains_key(&b),
            "{changes:?}"
        );
    }
}
