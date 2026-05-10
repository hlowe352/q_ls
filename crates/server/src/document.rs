use q_parser::Parse;
use tower_lsp_server::ls_types::*;

use crate::line_index::LineIndex;
use crate::sym_table::SymTable;

pub struct Document {
    text: String,
    version: i32,
    parse: Parse,
    line_index: LineIndex,
    sym_table: SymTable,
}

impl Document {
    pub fn new(text: String, version: i32) -> Self {
        let parse = q_parser::parse(&text);
        let line_index = LineIndex::new(&text);
        let sym_table = SymTable::build(&parse.syntax());
        Self { text, version, parse, line_index, sym_table }
    }

    pub fn sym_table(&self) -> &SymTable {
        &self.sym_table
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
            match change.range {
                Some(range) => {
                    let s = self.line_index.offset(&self.text, range.start);
                    let e = self.line_index.offset(&self.text, range.end);
                    self.text.replace_range(s..e, &change.text);
                    self.line_index = LineIndex::new(&self.text);
                }
                None => {
                    self.text = change.text;
                    self.line_index = LineIndex::new(&self.text);
                }
            }
        }

        self.version = version;
        self.parse = q_parser::parse(&self.text);
        self.sym_table = SymTable::build(&self.parse.syntax());
    }

    pub fn offset_of(&self, pos: Position) -> usize {
        self.line_index.offset(&self.text, pos)
    }

    pub fn position_of(&self, offset: usize) -> Position {
        self.line_index.position(&self.text, offset)
    }

    /// Identifier text spanning byte `offset`, plus its `[start, end)` byte
    /// range. q identifiers are runs of `[A-Za-z0-9_.]`. Returns `None` if
    /// `offset` falls outside any such run.
    pub fn ident_at(&self, offset: usize) -> Option<(&str, usize, usize)> {
        if offset > self.text.len() {
            return None;
        }
        let bytes = self.text.as_bytes();
        let mut start = offset;
        let mut end = offset;
        while start > 0 && is_ident_byte(bytes[start - 1]) {
            start -= 1;
        }
        while end < bytes.len() && is_ident_byte(bytes[end]) {
            end += 1;
        }
        if start == end {
            return None;
        }
        Some((&self.text[start..end], start, end))
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}
