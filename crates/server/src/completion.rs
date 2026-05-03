use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn complete(_doc: &Document, _pos: Position) -> Vec<CompletionItem> {
    Vec::new()
}
