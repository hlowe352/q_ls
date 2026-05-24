#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;
use crate::builtins::BUILTINS;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;

fn get_prefix(text: &str, offset: usize) -> String {
    let before = &text[..offset.min(text.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map_or(0, |i| i + 1);
    before[start..].to_string()
}

pub fn complete_with_workspace(
    doc: &Document,
    pos: Position,
    workspace: &WorkspaceIndex,
) -> Vec<CompletionItem> {
    let offset = doc.offset_of(pos);
    let prefix = get_prefix(doc.text(), offset);

    let mut items: Vec<CompletionItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for &(name, detail) in BUILTINS {
        if name.starts_with(prefix.as_str()) {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail.to_string()),
                ..Default::default()
            });
            seen.insert(name.to_string());
        }
    }

    // Document identifiers — pulled from the cached symbol table instead
    // of re-walking the tree on every keystroke.
    for ident in doc.sym_table().idents() {
        if ident.starts_with(prefix.as_str()) && !seen.contains(ident) {
            seen.insert(ident.to_string());
            items.push(CompletionItem {
                label: ident.to_string(),
                kind: Some(CompletionItemKind::VARIABLE),
                ..Default::default()
            });
        }
    }

    // Workspace-wide identifiers from other files
    for ident in workspace.all_idents() {
        if ident.starts_with(prefix.as_str()) && !seen.contains(ident) {
            seen.insert(ident.to_string());
            items.push(CompletionItem {
                label: ident.to_string(),
                kind: Some(CompletionItemKind::VARIABLE),
                ..Default::default()
            });
        }
    }

    items
}

#[allow(dead_code)]
pub fn complete(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    complete_with_workspace(doc, pos, &WorkspaceIndex::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_index::WorkspaceIndex;
    use crate::document::Document;

    #[test]
    fn workspace_globals_appear_in_completion() {
        let mut idx = WorkspaceIndex::default();
        idx.index_file(
            "file:///other.q".parse().unwrap(),
            Document::new("myHelper:{x+1}".to_string(), 0),
        );

        let doc = Document::new("myH".to_string(), 0);
        let pos = doc.position_of(3);
        let items = complete_with_workspace(&doc, pos, &idx);
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
        let items = complete_with_workspace(&doc, pos, &idx);
        assert!(
            items.iter().any(|i| i.label == "count"),
            "builtin count not found: {items:?}"
        );
    }
}
