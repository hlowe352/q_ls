//! `textDocument/rename` and `textDocument/prepareRename`.
//!
//! Reuses [`crate::references::find_references`] to locate every
//! occurrence of the symbol under the cursor, then emits a single
//! `WorkspaceEdit` mapping the document URI to one `TextEdit` per ref.

use std::collections::HashMap;
use tower_lsp_server::ls_types::{
    PrepareRenameResponse, Position, Range, TextEdit, Uri, WorkspaceEdit,
};

use crate::builtins::is_builtin;
use crate::document::Document;
use crate::references::find_references;

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
pub fn rename(
    doc: &Document,
    pos: Position,
    new_name: &str,
    uri: &Uri,
) -> Option<WorkspaceEdit> {
    let cursor = doc.offset_of(pos);
    let (old_name, _, _) = doc.ident_at(cursor)?;
    if is_builtin(old_name) {
        return None;
    }
    if !is_valid_identifier(new_name) {
        return None;
    }

    // include_declaration so the def site is rewritten too.
    let locations = find_references(doc, pos, true, uri);
    if locations.is_empty() {
        return None;
    }

    let edits: Vec<TextEdit> = locations
        .into_iter()
        .map(|loc| TextEdit { range: loc.range, new_text: new_name.to_string() })
        .collect();

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit { changes: Some(changes), document_changes: None, change_annotations: None })
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
    fn valid_identifier_shapes() {
        assert!(is_valid_identifier("foo"));
        assert!(is_valid_identifier(".ns.x"));
        assert!(is_valid_identifier("a1_b"));
        assert!(!is_valid_identifier("1foo"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("a b"));
    }
}
