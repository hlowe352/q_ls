use logos::Logos;

use crate::event::Event;
use crate::syntax_kind::SyntaxKind;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
    pub len: usize,
}

// ---------------------------------------------------------------------------
// Lexed token (post-lex, whitespace gaps already inserted)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct LexedToken {
    pub kind: SyntaxKind,
    pub text: String,
}

// ---------------------------------------------------------------------------
// Marker
// ---------------------------------------------------------------------------

/// An open marker returned by [`Parser::start`].
///
/// Must be closed with [`Marker::complete`] or discarded with
/// [`Marker::abandon`].
pub struct Marker {
    pub(crate) pos: usize,
    /// Set to `true` once the marker has been consumed so that the `Drop`
    /// impl can assert nothing is accidentally abandoned silently.
    completed: bool,
}

impl Marker {
    fn new(pos: usize) -> Self {
        Self { pos, completed: false }
    }

    /// Close the marker, turning the range of events since `start()` into a
    /// node of the given `kind`.  Returns a [`CompletedMarker`] which can be
    /// used to wrap the node inside a parent via [`CompletedMarker::precede`].
    pub fn complete(mut self, p: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
        self.completed = true;
        // Replace the Placeholder that was pushed at `start()` with a real
        // Start event.
        match &mut p.events[self.pos] {
            Event::Placeholder => {
                p.events[self.pos] = Event::Start { kind, forward_parent: None };
            }
            _ => unreachable!("marker position must point to a Placeholder"),
        }
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }

    /// Discard this marker (remove its Placeholder).  The events that were
    /// pushed between `start()` and `abandon()` are kept as-is — only the
    /// opening slot is cleared.
    pub fn abandon(mut self, p: &mut Parser) {
        self.completed = true;
        // Leave the Placeholder in place; the Sink will skip it.
        let _ = p; // nothing else to do
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        if !self.completed {
            // In debug builds, panic so the programmer notices.
            #[cfg(debug_assertions)]
            panic!("Marker dropped without being completed or abandoned");
        }
    }
}

// ---------------------------------------------------------------------------
// CompletedMarker
// ---------------------------------------------------------------------------

/// A closed marker.  Can be re-opened as a parent node via
/// [`CompletedMarker::precede`].
pub struct CompletedMarker {
    pub(crate) pos: usize,
}

impl CompletedMarker {
    /// Wrap this completed node inside a new parent.  Returns a [`Marker`]
    /// for the parent; call `.complete(p, kind)` on it when done.
    ///
    /// This works by inserting a new `Start` event *before* the current one
    /// using the `forward_parent` chain so that the Sink can re-order them.
    pub fn precede(self, p: &mut Parser) -> Marker {
        let new_pos = p.events.len();
        // The new Start will be placed at `new_pos`; it will have
        // `forward_parent` pointing to *this* node's Start event so the Sink
        // knows to emit `new_pos` first.
        p.events.push(Event::Placeholder);
        // Patch the existing Start to point forward to its new parent.
        match &mut p.events[self.pos] {
            Event::Start { forward_parent, .. } => {
                *forward_parent = Some(new_pos - self.pos);
            }
            _ => unreachable!("CompletedMarker must point to a Start event"),
        }
        Marker::new(new_pos)
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Collapse multi-line comment blocks into single LineComment tokens.
/// In q, `/` alone at the start of a line opens a block comment,
/// and `\` alone at the start of a line closes it.
fn collapse_block_comments(tokens: &mut Vec<LexedToken>) {
    let mut i = 0;
    while i < tokens.len() {
        // Block comment opener: a LineComment whose text is just `/\n` or `/\r\n`
        // that appears at the start of a line.
        let is_opener = tokens[i].kind == SyntaxKind::LineComment
            && tokens[i].text.trim_end_matches(['\r', '\n']) == "/"
            && (i == 0
                || tokens[i - 1].kind == SyntaxKind::Newline
                || tokens[i - 1].text.ends_with('\n'));

        if is_opener {
            // Find the matching closer: `\` at start of a line
            let start = i;
            let mut j = i + 1;
            let mut found = false;
            while j < tokens.len() {
                if tokens[j].kind == SyntaxKind::Backslash
                    && j > 0
                    && (tokens[j - 1].kind == SyntaxKind::Newline
                        || tokens[j - 1].text.ends_with('\n'))
                {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                // Merge tokens[start..=j] into one LineComment
                let text: String =
                    tokens[start..=j].iter().map(|t| t.text.as_str()).collect();
                tokens.splice(
                    start..=j,
                    std::iter::once(LexedToken {
                        kind: SyntaxKind::LineComment,
                        text,
                    }),
                );
                i = start + 1;
                continue;
            }
        }
        i += 1;
    }
}

pub struct Parser {
    tokens: Vec<LexedToken>,
    pos: usize,
    pub(crate) events: Vec<Event>,
    errors: Vec<ParseError>,
}

impl Parser {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Lex `source` and build the flat token list, inserting synthetic
    /// [`SyntaxKind::Whitespace`] tokens for any gaps the lexer skipped.
    pub fn new(source: &str) -> Self {
        let mut tokens: Vec<LexedToken> = Vec::new();
        let mut lexer = q_lexer::Token::lexer(source);
        let mut last_end: usize = 0;

        while let Some(result) = lexer.next() {
            let span = lexer.span();

            // If there's a gap between the last token's end and this token's
            // start, the lexer skipped whitespace — re-inject it.
            if span.start > last_end {
                let ws_text = source[last_end..span.start].to_string();
                tokens.push(LexedToken { kind: SyntaxKind::Whitespace, text: ws_text });
            }

            let kind = match result {
                Ok(tok) => SyntaxKind::from_token(tok),
                Err(_) => SyntaxKind::Error,
            };
            let text = source[span.clone()].to_string();
            tokens.push(LexedToken { kind, text });
            last_end = span.end;
        }

        // Trailing whitespace after the last token.
        if last_end < source.len() {
            let ws_text = source[last_end..].to_string();
            tokens.push(LexedToken { kind: SyntaxKind::Whitespace, text: ws_text });
        }

        // Post-process: collapse multi-line comment blocks.
        // In q, `/` alone at start of a line opens a block comment,
        // `\` alone at start of a line closes it.
        collapse_block_comments(&mut tokens);

        Self {
            tokens,
            pos: 0,
            events: Vec::new(),
            errors: Vec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Marker / event management
    // -----------------------------------------------------------------------

    /// Open a new marker at the current position.  The caller must close it
    /// with [`Marker::complete`] or [`Marker::abandon`].
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Placeholder);
        Marker::new(pos)
    }

    // -----------------------------------------------------------------------
    // Token inspection helpers
    // -----------------------------------------------------------------------

    /// The index of the `n`-th non-trivia token from the current position,
    /// or `None` if there are fewer than `n+1` such tokens remaining.
    fn non_trivia_idx(&self, n: usize) -> Option<usize> {
        let mut count = 0;
        let mut idx = self.pos;
        loop {
            if idx >= self.tokens.len() {
                return None;
            }
            if !self.tokens[idx].kind.is_trivia() {
                if count == n {
                    return Some(idx);
                }
                count += 1;
            }
            idx += 1;
        }
    }

    /// Peek at the current (non-trivia) token kind.
    pub fn current(&self) -> Option<SyntaxKind> {
        self.nth(0)
    }

    /// Lookahead: peek at the `n`-th non-trivia token kind from now.
    pub fn nth(&self, n: usize) -> Option<SyntaxKind> {
        self.non_trivia_idx(n).map(|i| self.tokens[i].kind)
    }

    /// Text of the current (non-trivia) token.
    pub fn current_text(&self) -> Option<String> {
        self.nth_text(0)
    }

    /// Text of the `n`-th non-trivia token.
    pub fn nth_text(&self, n: usize) -> Option<String> {
        self.non_trivia_idx(n).map(|i| self.tokens[i].text.clone())
    }

    /// Returns `true` if the current non-trivia token is `kind`.
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == Some(kind)
    }

    /// Returns `true` when there are no more non-trivia tokens.
    pub fn at_end(&self) -> bool {
        self.current().is_none()
    }

    // -----------------------------------------------------------------------
    // Consumption helpers
    // -----------------------------------------------------------------------

    /// Emit all leading trivia tokens as [`Event::Token`] events, advancing
    /// `self.pos` past them.
    pub fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token { kind: tok.kind, text: tok.text.clone() });
            self.pos += 1;
        }
    }

    /// Emit leading trivia then consume one non-trivia token.
    pub fn bump(&mut self) {
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token { kind: tok.kind, text: tok.text.clone() });
            self.pos += 1;
        }
    }

    /// Consume one token if the current token is `kind`.  Returns `true` if
    /// the token was consumed.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Consume `kind` or emit a parse error.
    pub fn expect(&mut self, kind: SyntaxKind) {
        if !self.eat(kind) {
            let msg = format!("expected {:?}", kind);
            self.error(msg);
        }
    }

    /// Record a parse error at the current token position and emit the
    /// current token as [`SyntaxKind::Error`].
    pub fn error(&mut self, msg: String) {
        // Determine offset and length from the current non-trivia token (if
        // any) so that the diagnostic spans the right source range.
        let (offset, len) = self
            .non_trivia_idx(0)
            .map(|i| {
                // Compute byte offset by summing lengths of all preceding tokens.
                let off: usize = self.tokens[..i].iter().map(|t| t.text.len()).sum();
                let l = self.tokens[i].text.len();
                (off, l)
            })
            .unwrap_or((0, 0));

        self.errors.push(ParseError { message: msg, offset, len });

        // Emit the offending token (if any) as an Error node so the tree
        // remains lossless.
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token { kind: SyntaxKind::Error, text: tok.text.clone() });
            self.pos += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Finalisation
    // -----------------------------------------------------------------------

    pub fn finish(self) -> (Vec<Event>, Vec<ParseError>) {
        (self.events, self.errors)
    }
}
