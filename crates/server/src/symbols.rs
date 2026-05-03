use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn document_symbols(_doc: &Document) -> Vec<DocumentSymbol> {
    Vec::new()
}
