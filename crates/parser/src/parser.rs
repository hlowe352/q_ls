use logos::Logos;
use smol_str::SmolStr;

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
    pub text: SmolStr,
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
        // In debug builds, panic so the programmer notices.
        assert!(self.completed, "Marker dropped without being completed or abandoned");
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

/// Returns `true` if the token at index `i` in `tokens` is at "line start":
/// i.e., it's at position 0 or all tokens before it (from position 0 or the
/// most-recent Newline) are Whitespace tokens.
fn is_at_line_start(tokens: &[LexedToken], i: usize) -> bool {
    // Walk backwards to find the most recent Newline (or start of file)
    let start = tokens[..i].iter().rposition(|t| t.kind == SyntaxKind::Newline);
    let from = start.map_or(0, |p| p + 1);
    // Every token from `from` to `i` (exclusive) must be Whitespace
    tokens[from..i].iter().all(|t| t.kind == SyntaxKind::Whitespace)
}

/// Normalize slash/backslash comment tokens in the raw token stream.
///
/// In q:
/// 1. A bare `/word` (slash immediately followed by text, no space) at the
///    start of a line is a line comment.  The lexer only recognises `/ text`
///    (with space) as `LineComment`, so we fix up the remaining cases here.
///
/// 2. A bare `\` at the start of a line opens a "terminal comment block"
///    that extends until the next `/` at line start (or EOF).  Everything
///    including the opening `\`, body lines, and closing `/` is merged into
///    a single `CommentBlock` token.
fn normalize_slash_comments(tokens: &mut Vec<LexedToken>) {
    let mut i = 0;
    while i < tokens.len() {
        // Case 1: Slash at line start — rest of the line is a comment.
        if tokens[i].kind == SyntaxKind::Slash && is_at_line_start(tokens, i) {
            // Collect text from this Slash up to (and including) the next Newline.
            let mut text = String::from(tokens[i].text.as_str());
            let mut end = i + 1;
            while end < tokens.len() && tokens[end].kind != SyntaxKind::Newline {
                text.push_str(&tokens[end].text);
                end += 1;
            }
            // Optionally include the trailing newline in the comment token.
            // (Don't include it — let the parser see the Newline as trivia.)
            let new_tok = LexedToken { kind: SyntaxKind::LineComment, text: SmolStr::from(text) };
            tokens.splice(i..end, [new_tok]);
            // i stays at i; we continue scanning from the token after our new LineComment
        }

        // Case 2: Backslash at line start — terminal comment block.
        else if tokens[i].kind == SyntaxKind::Backslash && is_at_line_start(tokens, i) {
            // Find the closing `/` at line start, or EOF.
            let mut end = i + 1;
            let mut found_close = false;
            while end < tokens.len() {
                if tokens[end].kind == SyntaxKind::Slash && is_at_line_start(tokens, end) {
                    // Include the closing slash in the comment block.
                    end += 1;
                    found_close = true;
                    break;
                }
                end += 1;
            }
            let _ = found_close; // may be false at EOF, which is fine
            let text: String = tokens[i..end].iter().map(|t| t.text.as_str()).collect();
            let new_tok = LexedToken { kind: SyntaxKind::CommentBlock, text: SmolStr::from(text) };
            tokens.splice(i..end, [new_tok]);
            // Do NOT advance i — the next iteration will move past this CommentBlock.
        }

        i += 1;
    }
}

/// Collapse multi-line comment blocks into single `LineComment` tokens.
/// In q, `/` alone at the start of a line opens a block comment,
/// and `\` alone at the start of a line closes it.
fn collapse_block_comments(tokens: &mut Vec<LexedToken>) {
    // The lexer generates CommentBlock tokens, but due to regex limitations in logos,
    // they may over-match content after the closing backslash. This function splits
    // such over-matched tokens by finding the line with only a closing backslash.
    //
    // A closing line is: optional whitespace, then `\`, then optional whitespace, then EOL.
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == SyntaxKind::CommentBlock {
            let text = tokens[i].text.clone();

            // Find the byte position where the closing backslash line ends
            let lines: Vec<&str> = text.split('\n').collect();

            // Closing line must be at position > 0 (after opening /)
            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();

                // Check if this is a closing backslash line
                if trimmed == "\\" && idx > 0 {
                    // Found the closing line; calculate the byte position of its end
                    let mut line_end_pos = 0;
                    for (j, l) in lines[..=idx].iter().enumerate() {
                        line_end_pos += l.len();
                        if j < idx {
                            line_end_pos += 1; // +1 for '\n'
                        }
                    }
                    // Add 1 for the final newline if it exists
                    if line_end_pos < text.len() {
                        line_end_pos += 1;
                    }

                    // Check if there's content after the closing line
                    if line_end_pos < text.len() {
                        // Split the token
                        let comment_part = text[..line_end_pos].to_string();
                        let remaining_part = text[line_end_pos..].to_string();

                        // Replace with just the comment
                        tokens[i].text = SmolStr::from(comment_part);

                        // Re-tokenize the remaining part
                        let mut remaining_tokens = Vec::new();
                        let mut remaining_lexer = q_lexer::Token::lexer(&remaining_part);
                        let mut remaining_last_end: usize = 0;

                        while let Some(result) = remaining_lexer.next() {
                            let span = remaining_lexer.span();

                            // Insert whitespace if there's a gap
                            if span.start > remaining_last_end {
                                let ws_text = SmolStr::new(&remaining_part[remaining_last_end..span.start]);
                                remaining_tokens.push(LexedToken {
                                    kind: SyntaxKind::Whitespace,
                                    text: ws_text,
                                });
                            }

                            let kind = match result {
                                Ok(tok) => SyntaxKind::from_token(tok),
                                Err(()) => SyntaxKind::Error,
                            };
                            let text = SmolStr::new(&remaining_part[span.clone()]);
                            remaining_tokens.push(LexedToken { kind, text });
                            remaining_last_end = span.end;
                        }

                        // Insert trailing whitespace if any
                        if remaining_last_end < remaining_part.len() {
                            let ws_text = SmolStr::new(&remaining_part[remaining_last_end..]);
                            remaining_tokens.push(LexedToken {
                                kind: SyntaxKind::Whitespace,
                                text: ws_text,
                            });
                        }

                        // Insert the remaining tokens after the current position
                        for (j, tok) in remaining_tokens.into_iter().enumerate() {
                            tokens.insert(i + 1 + j, tok);
                        }
                    }
                    break;
                }
            }
        }
        i += 1;
    }
}

/// Split `DslLine` tokens that aren't actually at line start back into their
/// constituent tokens. The lexer matches `[kp]\)[^\r\n]*` anywhere, but q only
/// treats `k)...` / `p)...` as DSL escape lines when they begin a line.
fn split_misplaced_dsl_lines(tokens: &mut Vec<LexedToken>) {
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == SyntaxKind::DslLine && !is_at_line_start(tokens, i) {
            let text = tokens[i].text.clone();
            // text is guaranteed to start with `k)` or `p)` (ASCII).
            let mut new_toks: Vec<LexedToken> = Vec::with_capacity(4);
            new_toks.push(LexedToken { kind: SyntaxKind::Ident,  text: SmolStr::new(&text[0..1]) });
            new_toks.push(LexedToken { kind: SyntaxKind::RParen, text: SmolStr::new(&text[1..2]) });

            let rest = &text[2..];
            let mut lexer = q_lexer::Token::lexer(rest);
            let mut last_end: usize = 0;
            while let Some(result) = lexer.next() {
                let span = lexer.span();
                if span.start > last_end {
                    new_toks.push(LexedToken {
                        kind: SyntaxKind::Whitespace,
                        text: SmolStr::new(&rest[last_end..span.start]),
                    });
                }
                let kind = match result {
                    Ok(tok) => SyntaxKind::from_token(tok),
                    Err(()) => SyntaxKind::Error,
                };
                new_toks.push(LexedToken { kind, text: SmolStr::new(&rest[span.clone()]) });
                last_end = span.end;
            }
            if last_end < rest.len() {
                new_toks.push(LexedToken {
                    kind: SyntaxKind::Whitespace,
                    text: SmolStr::new(&rest[last_end..]),
                });
            }

            tokens.splice(i..=i, new_toks);
            // Advance past Ident + RParen so any nested `p)` in the re-lexed
            // remainder gets re-checked on the next iteration.
            i += 2;
        } else {
            i += 1;
        }
    }
}

pub struct Parser {
    tokens: Vec<LexedToken>,
    pos: usize,
    /// Indices into `tokens` of every non-trivia token, in order. Built
    /// once after lex/post-processing so `non_trivia_idx` is O(1) instead
    /// of a linear scan from `self.pos`.
    nt: Vec<usize>,
    /// Rank of the next non-trivia token at or after `self.pos`. Maintained
    /// by `bump`/`error` (each consumes exactly one non-trivia token after
    /// any leading trivia).
    nt_cursor: usize,
    pub(crate) events: Vec<Event>,
    errors: Vec<ParseError>,
}

impl Parser {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Lex `source` and build the flat token list, inserting synthetic
    /// [`SyntaxKind::Whitespace`] tokens for any gaps the lexer skipped.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut tokens: Vec<LexedToken> = Vec::new();
        let mut lexer = q_lexer::Token::lexer(source);
        let mut last_end: usize = 0;

        while let Some(result) = lexer.next() {
            let span = lexer.span();

            // If there's a gap between the last token's end and this token's
            // start, the lexer skipped whitespace — re-inject it.
            if span.start > last_end {
                let ws_text = SmolStr::new(&source[last_end..span.start]);
                tokens.push(LexedToken { kind: SyntaxKind::Whitespace, text: ws_text });
            }

            let kind = match result {
                Ok(tok) => SyntaxKind::from_token(tok),
                Err(()) => SyntaxKind::Error,
            };
            let text = SmolStr::new(&source[span.clone()]);
            tokens.push(LexedToken { kind, text });
            last_end = span.end;
        }

        // Trailing whitespace after the last token.
        if last_end < source.len() {
            let ws_text = SmolStr::new(&source[last_end..]);
            tokens.push(LexedToken { kind: SyntaxKind::Whitespace, text: ws_text });
        }

        // Post-process step 1: normalize slash/backslash comments.
        // In q, `/word` (no space) at line start is a comment, and a bare `\`
        // at line start opens a terminal comment block that runs to the next
        // `/` at line start (or EOF).
        normalize_slash_comments(&mut tokens);

        // Post-process step 2: collapse multi-line comment blocks.
        // In q, `/` alone at start of a line opens a block comment,
        // `\` alone at start of a line closes it.
        collapse_block_comments(&mut tokens);

        // Post-process step 3: split misplaced DSL prefix tokens.
        // The lexer regex for `k)...` / `p)...` matches anywhere; q only
        // treats them as DSL escape lines when they appear at the start of
        // a line. Mid-expression occurrences (e.g. `(3#p),fn` containing
        // `p)`) must be split back into Ident + RParen + rest.
        split_misplaced_dsl_lines(&mut tokens);

        let nt: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.kind.is_trivia())
            .map(|(i, _)| i)
            .collect();

        Self {
            tokens,
            pos: 0,
            nt,
            nt_cursor: 0,
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
    ///
    /// O(1): the non-trivia indices are precomputed in `Parser::new`, and
    /// `nt_cursor` is kept in sync by `bump` / `error`.
    fn non_trivia_idx(&self, n: usize) -> Option<usize> {
        self.nt.get(self.nt_cursor + n).copied()
    }

    /// Peek at the current (non-trivia) token kind.
    #[must_use]
    pub fn current(&self) -> Option<SyntaxKind> {
        self.nth(0)
    }

    /// Lookahead: peek at the `n`-th non-trivia token kind from now.
    #[must_use]
    pub fn nth(&self, n: usize) -> Option<SyntaxKind> {
        self.non_trivia_idx(n).map(|i| self.tokens[i].kind)
    }

    /// Text of the current (non-trivia) token.
    #[must_use]
    pub fn current_text(&self) -> Option<&str> {
        self.nth_text(0)
    }

    /// Text of the `n`-th non-trivia token.
    #[must_use]
    pub fn nth_text(&self, n: usize) -> Option<&str> {
        self.non_trivia_idx(n).map(|i| self.tokens[i].text.as_str())
    }

    /// Returns `true` if the current non-trivia token is `kind`.
    #[must_use]
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == Some(kind)
    }

    /// Returns `true` when there are no more non-trivia tokens.
    #[must_use]
    pub fn at_end(&self) -> bool {
        self.current().is_none()
    }

    /// Returns `true` if a newline separates the current position from the
    /// next non-trivia token AND the next line is *not* an indented continuation.
    ///
    /// q's line-continuation rule: a logical statement continues onto the next
    /// physical line when that line begins with whitespace (space or tab).
    /// A non-blank line starting in column 0 is a new statement.
    #[must_use]
    pub fn has_preceding_newline(&self) -> bool {
        // Find the last Newline within the leading trivia block.
        let mut last_newline = None;
        for i in self.pos..self.tokens.len() {
            match self.tokens[i].kind {
                SyntaxKind::Newline => last_newline = Some(i),
                k if k.is_trivia() => {}
                _ => break,
            }
        }
        let Some(nl_idx) = last_newline else { return false; };

        // If the very next token after the newline is Whitespace, the next
        // line is indented — treat as a continuation, not a boundary.
        if let Some(next) = self.tokens.get(nl_idx + 1)
            && next.kind == SyntaxKind::Whitespace
        {
            return false;
        }
        true
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
            self.nt_cursor += 1;
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
            let msg = format!("expected {kind:?}");
            self.error(msg);
        }
    }

    /// Record a parse error at the current token position and emit the
    /// current token as [`SyntaxKind::Error`].
    pub fn error(&mut self, msg: String) {
        // Determine offset and length from the current non-trivia token (if
        // any) so that the diagnostic spans the right source range.
        let (offset, len) = self.non_trivia_idx(0).map_or((0, 0), |i| {
            // Compute byte offset by summing lengths of all preceding tokens.
            let off: usize = self.tokens[..i].iter().map(|t| t.text.len()).sum();
            let l = self.tokens[i].text.len();
            (off, l)
        });

        self.errors.push(ParseError { message: msg, offset, len });

        // Emit the offending token (if any) as an Error node so the tree
        // remains lossless.
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token { kind: SyntaxKind::Error, text: tok.text.clone() });
            self.pos += 1;
            self.nt_cursor += 1;
        }
    }

    // -----------------------------------------------------------------------
    // Finalisation
    // -----------------------------------------------------------------------

    #[must_use]
    pub fn finish(self) -> (Vec<Event>, Vec<ParseError>) {
        (self.events, self.errors)
    }
}
