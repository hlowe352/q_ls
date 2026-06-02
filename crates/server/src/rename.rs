//! `textDocument/rename` and `textDocument/prepareRename`.
//!
//! Reuses [`crate::references::find_references_with_workspace`] to locate every
//! occurrence of the symbol under the cursor across all open files, then emits
//! a `WorkspaceEdit` mapping each file URI to its `TextEdit`s.

use std::collections::HashMap;
use tower_lsp_server::ls_types::{
    PrepareRenameResponse, Position, Range, TextEdit, Uri, WorkspaceEdit,
};

use crate::builtins::is_builtin;
use crate::document::Document;
use crate::references::find_references_with_workspace;
use crate::workspace_index::WorkspaceIndex;

/// Validate that the cursor sits on a renameable identifier and return its
/// current text + range. Refuses built-ins and tokens that don't look like
/// identifiers.
pub fn prepare_rename(doc: &Document, pos: Position) -> Option<PrepareRenameResponse> {
    let cursor = doc.offset_of(pos);
    let (name, start, end) = doc.ident_at(cursor)?;
    if is_builtin(name) {
        return None;
    }
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: Range::new(doc.position_of(start), doc.position_of(end)),
        placeholder: name.to_string(),
    })
}

/// Build the `WorkspaceEdit` for renaming the symbol at `pos` to `new_name`.
/// Returns `None` if the cursor isn't on a renameable identifier or the
/// new name fails the same identifier-shape check.
///
/// This is a single-file convenience wrapper around [`rename_with_workspace`].
#[allow(dead_code)]
pub fn rename(
    doc: &Document,
    pos: Position,
    new_name: &str,
    uri: &Uri,
) -> Option<WorkspaceEdit> {
    rename_with_workspace(doc, pos, new_name, uri, &HashMap::new(), &WorkspaceIndex::default())
}

/// Build the `WorkspaceEdit` for renaming the symbol at `pos` to `new_name`,
/// searching across all open documents and the workspace index for references.
///
/// Returns `None` if the cursor isn't on a renameable identifier or the
/// new name fails the identifier-shape check.
pub fn rename_with_workspace(
    doc: &Document,
    pos: Position,
    new_name: &str,
    uri: &Uri,
    open_docs: &HashMap<Uri, Document>,
    workspace: &WorkspaceIndex,
) -> Option<WorkspaceEdit> {
    let cursor = doc.offset_of(pos);
    let (old_name, _, _) = doc.ident_at(cursor)?;
    if is_builtin(old_name) {
        return None;
    }
    if !is_valid_identifier(new_name) {
        return None;
    }

    // Qualified form (e.g. `.cache.cache` when cursor is on `cache` inside `\d .cache`).
    let qualified_old = doc.sym_table()
        .qualified_for(cursor, old_name)
        .map_or_else(|| old_name.to_string(), |q| q.to_string());
    // Namespace prefix: `.cache.` for `.cache.cache`, empty for bare names.
    let ns_prefix: String = qualified_old
        .rsplit_once('.')
        .map(|(prefix, _)| format!("{prefix}."))
        .unwrap_or_default();

    let locations = find_references_with_workspace(doc, pos, true, uri, open_docs, workspace);
    if locations.is_empty() {
        return None;
    }

    // Build a lookup for original document text: current doc + open docs + workspace.
    let get_doc = |u: &Uri| -> Option<&Document> {
        if u == uri { return Some(doc); }
        if let Some(d) = open_docs.get(u) { return Some(d); }
        workspace.files().get(u)
    };

    // Group locations by file URI, computing the right new_text per token shape.
    let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        let new_text = get_doc(&loc.uri).and_then(|d| {
            let start = d.offset_of(loc.range.start);
            let end   = d.offset_of(loc.range.end);
            d.text().get(start..end)
        }).map_or_else(
            || new_name.to_string(),
            |original| rewrite_token(original, new_name, old_name, &qualified_old, &ns_prefix),
        );
        changes.entry(loc.uri).or_default()
            .push(TextEdit { range: loc.range, new_text });
    }

    Some(WorkspaceEdit { changes: Some(changes), document_changes: None, change_annotations: None })
}

/// Given the original token text and the new bare name, produce the
/// correctly shaped replacement:
///
/// | Original             | Result                |
/// |----------------------|-----------------------|
/// | `cache`              | `newName`             |
/// | `.cache.cache`       | `.cache.newName`      |
/// | `` `.cache.cache ``  | `` `.cache.newName `` |
/// | `` `cache ``         | `` `newName ``        |
fn rewrite_token(original: &str, new_name: &str, old_name: &str, qualified_old: &str, ns_prefix: &str) -> String {
    if let Some(sym_body) = original.strip_prefix('`') {
        // Symbol token — preserve the leading backtick.
        let new_body = rewrite_ident(sym_body, new_name, old_name, qualified_old, ns_prefix);
        format!("`{new_body}")
    } else {
        rewrite_ident(original, new_name, old_name, qualified_old, ns_prefix)
    }
}

fn rewrite_ident(original: &str, new_name: &str, old_name: &str, qualified_old: &str, ns_prefix: &str) -> String {
    if original == old_name {
        new_name.to_string()
    } else if original == qualified_old {
        // Qualified form: keep namespace prefix, swap bare name.
        format!("{ns_prefix}{new_name}")
    } else {
        // Fallback (shouldn't happen for valid locations).
        new_name.to_string()
    }
}

/// Match q's identifier shape: starts with a letter (or `.` for namespaced
/// names), continues with alphanumerics/underscores/dots. Rejects empty
/// names, names starting with a digit, and names containing whitespace.
fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    let first_ok = first.is_ascii_alphabetic() || first == '.';
    if !first_ok {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri() -> Uri {
        "file:///x.q".parse().unwrap()
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
        let edit = rename(&doc, pos, "baz", &uri()).expect("rename ok");
        let edits = edit
            .changes
            .as_ref()
            .and_then(|c| c.get(&uri()))
            .expect("has edits");
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
        let edit = rename(&doc, pos, "y", &uri()).expect("rename ok");
        let edits = edit
            .changes
            .as_ref()
            .and_then(|c| c.get(&uri()))
            .expect("has edits");
        // Just the outer x:99 def — neither the param nor `x+1` ref.
        assert_eq!(edits.len(), 1);
    }

    #[test]
    fn rename_rewrites_local_rebindings() {
        // q rebinds the same lambda-local — rename should touch all sites.
        let doc = Document::new("f:{a:1; a:2; a}".to_string(), 0);
        let src = doc.text().to_string();
        let pos = doc.position_of(src.find("a:2").unwrap());
        let edit = rename(&doc, pos, "b", &uri()).expect("rename ok");
        let edits = edit
            .changes
            .as_ref()
            .and_then(|c| c.get(&uri()))
            .expect("has edits");
        assert_eq!(edits.len(), 3, "expected all 3 a sites, got {edits:#?}");
    }

    #[test]
    fn rename_rejects_invalid_new_name() {
        let doc = Document::new("foo:1".to_string(), 0);
        let pos = doc.position_of(0);
        assert!(rename(&doc, pos, "1bad", &uri()).is_none());
        assert!(rename(&doc, pos, "with space", &uri()).is_none());
        assert!(rename(&doc, pos, "", &uri()).is_none());
    }

    #[test]
    fn rename_preserves_backtick_on_symbol_refs() {
        // `.cache.cache upsert row` — the symbol token must keep its backtick.
        let src = "\\d .cache\ncache:1\nf:{`.cache.cache upsert (1;2)}";
        let doc = Document::new(src.to_string(), 0);
        let pos = doc.position_of(src.find("cache:1").unwrap());
        let edit = rename(&doc, pos, "tbl", &uri()).expect("rename ok");
        let edits = edit.changes.as_ref().and_then(|c| c.get(&uri())).expect("has edits");
        // The symbol edit must produce `` `.cache.tbl ``, not just `tbl`.
        let sym_edit = edits.iter().find(|e| e.new_text.starts_with('`'))
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
        let edit = rename(&doc, pos, "tbl", &uri()).expect("rename ok");
        let edits = edit.changes.as_ref().and_then(|c| c.get(&uri())).expect("has edits");
        let dotted_edit = edits.iter().find(|e| e.new_text.starts_with('.'))
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
    fn rename_with_workspace_rewrites_across_files() {
        use crate::document::Document;
        use crate::workspace_index::WorkspaceIndex;
        use std::collections::HashMap;

        let uri_a: Uri = "file:///a.q".parse().unwrap();
        let uri_b: Uri = "file:///b.q".parse().unwrap();

        let doc_a = Document::new("sharedFn:{x+1}".to_string(), 0);

        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri_a.clone(), Document::new("sharedFn:{x+1}".to_string(), 0));

        let mut open_docs = HashMap::new();
        open_docs.insert(uri_b.clone(), Document::new("sharedFn 99".to_string(), 0));

        let pos = doc_a.position_of(0); // cursor on `sharedFn` def in a.q
        let edit = rename_with_workspace(&doc_a, pos, "newFn", &uri_a, &open_docs, &idx)
            .expect("rename_with_workspace must return Some");

        let changes = edit.changes.as_ref().expect("must have changes");
        assert!(changes.contains_key(&uri_a), "must rewrite a.q");
        assert!(changes.contains_key(&uri_b), "must rewrite b.q");
        assert_eq!(changes[&uri_a][0].new_text, "newFn");
        assert_eq!(changes[&uri_b][0].new_text, "newFn");
    }

    #[test]
    fn rename_with_workspace_from_use_site_rewrites_across_files() {
        use crate::document::Document;
        use crate::workspace_index::WorkspaceIndex;
        use std::collections::HashMap;

        let uri_a: Uri = "file:///a.q".parse().unwrap();
        let uri_b: Uri = "file:///b.q".parse().unwrap();

        let doc_b = Document::new("sharedFn 99".to_string(), 0);

        let mut idx = WorkspaceIndex::default();
        idx.index_file(uri_a.clone(), Document::new("sharedFn:{x+1}".to_string(), 0));

        let pos = doc_b.position_of(0); // cursor on `sharedFn` use in b.q
        let edit = rename_with_workspace(&doc_b, pos, "newFn", &uri_b, &HashMap::new(), &idx)
            .expect("rename from use site must return Some");

        let changes = edit.changes.as_ref().expect("must have changes");
        assert!(changes.contains_key(&uri_a), "must rewrite def in a.q");
        assert!(changes.contains_key(&uri_b), "must rewrite use in b.q");
    }
}
