use tower_lsp_server::ls_types::*;
use crate::builtins::BUILTINS;
use crate::document::Document;

fn get_prefix(text: &str, offset: usize) -> String {
    let before = &text[..offset.min(text.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map_or(0, |i| i + 1);
    before[start..].to_string()
}

pub fn complete(doc: &Document, pos: Position) -> Vec<CompletionItem> {
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

    items
}
