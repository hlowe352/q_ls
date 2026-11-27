use q_parser::{Parse, SyntaxToken, TextSize};
#[allow(clippy::wildcard_imports)]
use tower_lsp_server::ls_types::*;

use crate::binder::Bindings;
use crate::line_index::LineIndex;

pub struct Document {
    text: String,
    version: i32,
    parse: Parse,
    line_index: LineIndex,
    bindings: Bindings,
}

impl Document {
    pub fn new(text: String, version: i32) -> Self {
        let parse = q_parser::parse(&text);
        let line_index = LineIndex::new(&text);
        let bindings = Bindings::build(&parse.syntax());
        Self {
            text,
            version,
            parse,
            line_index,
            bindings,
        }
    }

    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    pub fn version(&self) -> i32 {
        self.version
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn parse(&self) -> &Parse {
        &self.parse
    }

    pub fn apply_changes(&mut self, changes: Vec<TextDocumentContentChangeEvent>, version: i32) {
        // Per LSP spec, edits in a single notification apply in order against
        // the original document. Walk the list once: for ranged edits, resolve
        // against the *current* text & line index, then mutate (refreshing
        // the line index after each edit so subsequent ranges stay correct).
        // A full-document replace (no `range`) discards prior text.
        for change in changes {
            if let Some(range) = change.range {
                let s = self.line_index.offset(&self.text, range.start);
                let e = self.line_index.offset(&self.text, range.end);
                self.text.replace_range(s..e, &change.text);
                self.line_index = LineIndex::new(&self.text);
            } else {
                self.text = change.text;
                self.line_index = LineIndex::new(&self.text);
            }
        }

        self.version = version;
        self.parse = q_parser::parse(&self.text);
        self.bindings = Bindings::build(&self.parse.syntax());
    }

    pub fn offset_of(&self, pos: Position) -> usize {
        self.line_index.offset(&self.text, pos)
    }

    pub fn position_of(&self, offset: usize) -> Position {
        self.line_index.position(&self.text, offset)
    }

    /// The non-trivia token at byte `offset`, or ending there (a cursor
    /// just after a name).
    #[allow(clippy::cast_possible_truncation)]
    pub fn token_at(&self, offset: usize) -> Option<SyntaxToken> {
        let at = self
            .parse
            .syntax()
            .token_at_offset(TextSize::from(offset as u32));
        at.clone()
            .right_biased()
            .filter(|t| !t.kind().is_trivia())
            .or_else(|| at.left_biased().filter(|t| !t.kind().is_trivia()))
    }
}
