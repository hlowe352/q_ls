use q_parser::Parse;
use tower_lsp::lsp_types::*;

pub struct Document {
    text: String,
    version: i32,
    parse: Parse,
}

impl Document {
    pub fn new(text: String, version: i32) -> Self {
        let parse = q_parser::parse(&text);
        Self { text, version, parse }
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
        for change in changes {
            if let Some(range) = change.range {
                let start = self.offset_of(range.start);
                let end = self.offset_of(range.end);
                self.text.replace_range(start..end, &change.text);
            } else {
                self.text = change.text;
            }
        }
        self.version = version;
        self.parse = q_parser::parse(&self.text);
    }

    pub fn offset_of(&self, pos: Position) -> usize {
        let mut offset = 0;
        for (i, line) in self.text.split('\n').enumerate() {
            if i == pos.line as usize {
                return offset + pos.character as usize;
            }
            offset += line.len() + 1;
        }
        self.text.len()
    }

    pub fn position_of(&self, offset: usize) -> Position {
        let mut line = 0u32;
        let mut col = 0u32;
        for (i, ch) in self.text.char_indices() {
            if i == offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        Position::new(line, col)
    }
}
