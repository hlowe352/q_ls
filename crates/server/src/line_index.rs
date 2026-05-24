//! Byte-offset ↔ LSP Position conversion.
//!
//! LSP Position uses UTF-16 code units for `character`. We keep the source as
//! UTF-8, so column conversion walks chars within the relevant line.
//!
//! Line endings: `\n` and `\r\n` both terminate a line. The `\r` in `\r\n`
//! belongs to the previous line for column counting; only the `\n` advances
//! the line index.

use tower_lsp_server::ls_types::Position;

/// Line-start byte offsets, plus the total length. `starts[i]` is the byte
/// offset of the first character on line `i`. `starts.last()` equals
/// `text.len()` (sentinel) so line slicing is uniform.
pub struct LineIndex {
    starts: Vec<u32>,
    len: u32,
}

impl LineIndex {
    #[allow(clippy::cast_possible_truncation)]
    pub fn new(text: &str) -> Self {
        let mut starts = Vec::with_capacity(text.len() / 40 + 1);
        starts.push(0);
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push((i + 1) as u32);
            }
        }
        Self { starts, len: text.len() as u32 }
    }

    /// Convert an LSP Position (UTF-16) to a UTF-8 byte offset, clamping to
    /// the end of the document if out of range.
    pub fn offset(&self, text: &str, pos: Position) -> usize {
        let line = pos.line as usize;
        if line >= self.starts.len() {
            return self.len as usize;
        }
        let line_start = self.starts[line] as usize;
        let line_end = self
            .starts
            .get(line + 1)
            .map_or(self.len as usize, |&n| n as usize);
        // Trim trailing newline (and CR) from the slice we walk.
        let mut slice_end = line_end;
        if slice_end > line_start && text.as_bytes()[slice_end - 1] == b'\n' {
            slice_end -= 1;
        }
        if slice_end > line_start && text.as_bytes()[slice_end - 1] == b'\r' {
            slice_end -= 1;
        }
        let line_text = &text[line_start..slice_end];

        let target = pos.character as usize;
        let mut utf16 = 0usize;
        for (byte_off, ch) in line_text.char_indices() {
            if utf16 >= target {
                return line_start + byte_off;
            }
            utf16 += ch.len_utf16();
        }
        // Past end of line — clamp to line end (excluding newline).
        line_start + line_text.len()
    }

    /// Convert a UTF-8 byte offset to an LSP Position (UTF-16).
    #[allow(clippy::cast_possible_truncation)]
    pub fn position(&self, text: &str, offset: usize) -> Position {
        let offset = offset.min(self.len as usize);
        // Binary search: find the largest line whose start ≤ offset.
        let line = match self.starts.binary_search(&(offset as u32)) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.starts[line] as usize;
        let line_text = &text[line_start..offset];
        let character: usize = line_text.chars().map(char::len_utf16).sum();
        Position::new(line as u32, character as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trip() {
        let text = "abc\ndef\nghi";
        let li = LineIndex::new(text);
        for off in 0..=text.len() {
            let pos = li.position(text, off);
            let back = li.offset(text, pos);
            assert_eq!(back, off, "offset {off} round-trip via {pos:?}");
        }
    }

    #[test]
    fn utf8_multibyte_uses_utf16_columns() {
        // "é" = 2 bytes UTF-8, 1 UTF-16 unit.
        let text = "café\nx";
        let li = LineIndex::new(text);
        // After "caf" + "é" -> column 4 (UTF-16), byte offset 5.
        let end_of_line0 = li.position(text, 5);
        assert_eq!(end_of_line0, Position::new(0, 4));
        let off = li.offset(text, Position::new(0, 4));
        assert_eq!(off, 5);
    }

    #[test]
    fn supplementary_plane_is_two_utf16_units() {
        // 🦀 = U+1F980, 4 bytes UTF-8, 2 UTF-16 units.
        let text = "🦀x";
        let li = LineIndex::new(text);
        // Position after the crab is column 2 in UTF-16, byte 4.
        assert_eq!(li.position(text, 4), Position::new(0, 2));
        assert_eq!(li.offset(text, Position::new(0, 2)), 4);
        // Position 1 (mid-surrogate) clamps inside the char — round-trip
        // via offset(1) lands at byte 0 (start of char) per char_indices.
        // Acceptable: editors don't emit mid-surrogate positions in practice.
    }

    #[test]
    fn crlf_line_endings() {
        let text = "abc\r\ndef";
        let li = LineIndex::new(text);
        // End of line 0 (before CR) is column 3.
        assert_eq!(li.position(text, 3), Position::new(0, 3));
        // Start of line 1 is byte 5 (after \r\n).
        assert_eq!(li.offset(text, Position::new(1, 0)), 5);
        assert_eq!(li.position(text, 5), Position::new(1, 0));
    }

    #[test]
    fn out_of_range_clamps() {
        let text = "ab\ncd";
        let li = LineIndex::new(text);
        // Line past end → clamps to text length.
        assert_eq!(li.offset(text, Position::new(99, 0)), text.len());
        // Column past end of line → clamps to end of that line.
        assert_eq!(li.offset(text, Position::new(0, 99)), 2);
    }

    #[test]
    fn empty_text() {
        let text = "";
        let li = LineIndex::new(text);
        assert_eq!(li.offset(text, Position::new(0, 0)), 0);
        assert_eq!(li.position(text, 0), Position::new(0, 0));
    }
}
