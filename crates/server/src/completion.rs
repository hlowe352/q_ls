use crate::builtins::BUILTINS;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;
use std::collections::HashSet;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

/// Builtins, then this document's identifiers, then workspace globals —
/// each label once, filtered by the identifier prefix before the cursor.
pub fn complete(doc: &Document, pos: Position, workspace: &WorkspaceIndex) -> Vec<CompletionItem> {
    let before = &doc.text()[..doc.offset_of(pos)];
    let prefix = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map_or(before, |i| &before[i + 1..]);

    let builtins = BUILTINS
        .iter()
        .map(|&(name, detail)| (name, Some(detail), CompletionItemKind::FUNCTION));
    let idents = doc
        .sym_table()
        .idents()
        .chain(workspace.all_idents())
        .map(|name| (name, None, CompletionItemKind::VARIABLE));
    let mut seen = HashSet::new();
    builtins
        .chain(idents)
        .filter(|(name, ..)| name.starts_with(prefix) && seen.insert(*name))
        .map(|(name, detail, kind)| CompletionItem {
            label: name.to_string(),
            kind: Some(kind),
            detail: detail.map(str::to_string),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::workspace_index::WorkspaceIndex;

    #[test]
    fn workspace_globals_appear_in_completion() {
        let mut idx = WorkspaceIndex::default();
        idx.insert(
            "file:///other.q".parse().unwrap(),
            Document::new("myHelper:{x+1}".to_string(), 0),
        );

        let doc = Document::new("myH".to_string(), 0);
        let pos = doc.position_of(3);
        let items = complete(&doc, pos, &idx);
        assert!(
            items.iter().any(|i| i.label == "myHelper"),
            "workspace global must appear in completion: {items:?}"
        );
    }

    #[test]
    fn builtin_still_appears() {
        let idx = WorkspaceIndex::default();
        let doc = Document::new("cou".to_string(), 0);
        let pos = doc.position_of(3);
        let items = complete(&doc, pos, &idx);
        assert!(
            items.iter().any(|i| i.label == "count"),
            "builtin count not found: {items:?}"
        );
    }
}
