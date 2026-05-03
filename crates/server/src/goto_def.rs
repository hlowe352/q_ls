use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn goto_definition(_doc: &Document, _pos: Position, _uri: &Url) -> Option<GotoDefinitionResponse> {
    None
}
