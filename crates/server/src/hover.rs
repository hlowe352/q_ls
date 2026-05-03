use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn hover(_doc: &Document, _pos: Position) -> Option<Hover> {
    None
}
