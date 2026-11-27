use crate::builtins::BUILTINS;
use crate::document::Document;
use crate::workspace_index::WorkspaceIndex;
use std::collections::HashSet;
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

/// Builtins, then this document's identifiers, then workspace globals —
/// each label once, filtered by the identifier prefix before the cursor.
/// The word being typed is only offered if it names something.
pub fn complete(doc: &Document, pos: Position, workspace: &WorkspaceIndex) -> Vec<CompletionItem> {
    let offset = doc.offset_of(pos);
    let before = &doc.text()[..offset];
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
        .filter(|(name, ..)| {
            *name != prefix
                || doc.sym_table().resolve(offset, name).is_some()
                || workspace.resolve_global(name).is_some()
        })
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
    use crate::workspace_index::tests::doc;

    /// Builtins first, then document names; prefix-filtered, each label once.
    /// (Workspace names are covered end-to-end in tests/lsp.rs.)
    #[test]
    fn merges_sources_filtered_by_prefix() {
        let src = "counter:1; counter+1\ncou";
        let d = doc(src);
        let items = complete(&d, d.position_of(src.len()), &WorkspaceIndex::default());
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            ["count", "counter"],
            "the typed `cou` is not offered"
        );
        assert_eq!(items[0].kind, Some(CompletionItemKind::FUNCTION));
        assert_eq!(items[1].kind, Some(CompletionItemKind::VARIABLE));

        // ...but a defined name equal to the prefix still is.
        let src = "cou:1\ncou";
        let d = doc(src);
        let items = complete(&d, d.position_of(src.len()), &WorkspaceIndex::default());
        assert!(items.iter().any(|i| i.label == "cou"), "{items:?}");
    }
}
