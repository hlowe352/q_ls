use logos::Logos;
use smol_str::SmolStr;

use crate::grammar::expressions::is_scalar_literal;
use crate::syntax_kind::SyntaxKind;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
    pub len: usize,
}

/// Parser output, converted to a tree by [`crate::sink::build`].
#[derive(Debug, Clone)]
pub enum Event {
    /// Open a node. `forward_parent` is the relative index of a later
    /// `Start` that must wrap this one (set by [`CompletedMarker::precede`]).
    Start {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    Token {
        kind: SyntaxKind,
        text: SmolStr,
    },
    Finish,
    /// Not-yet-completed marker, or a parent opened via `forward_parent`; emits nothing.
    Placeholder,
}

/// Post-lex token, with skipped whitespace re-inserted.
#[derive(Debug, Clone)]
struct LexedToken {
    kind: SyntaxKind,
    text: SmolStr,
}

impl LexedToken {
    fn new(kind: SyntaxKind, text: &str) -> Self {
        Self {
            kind,
            text: SmolStr::new(text),
        }
    }
}

/// An open marker returned by [`Parser::start`]. Must be closed with
/// [`Marker::complete`].
pub struct Marker {
    pos: usize,
    /// Drop bomb: catches markers that are silently forgotten.
    completed: bool,
}

impl Marker {
    fn new(pos: usize) -> Self {
        Self {
            pos,
            completed: false,
        }
    }

    /// Close the marker, turning the events since `start()` into a node of
    /// `kind`. The result can be wrapped later via [`CompletedMarker::precede`].
    pub fn complete(mut self, p: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
        self.completed = true;
        let slot = &mut p.events[self.pos];
        assert!(
            matches!(slot, Event::Placeholder),
            "marker position must point to a Placeholder"
        );
        *slot = Event::Start {
            kind,
            forward_parent: None,
        };
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        assert!(self.completed, "Marker dropped without being completed");
    }
}

/// A closed marker. Can be re-opened as a parent node via [`Self::precede`].
pub struct CompletedMarker {
    pos: usize,
}

impl CompletedMarker {
    /// Wrap this completed node inside a new parent. Pushes a Placeholder
    /// for the parent and links this node's `Start` to it via
    /// `forward_parent` so the sink opens the parent first.
    pub fn precede(self, p: &mut Parser) -> Marker {
        let new_pos = p.events.len();
        p.events.push(Event::Placeholder);
        match &mut p.events[self.pos] {
            Event::Start { forward_parent, .. } => *forward_parent = Some(new_pos - self.pos),
            _ => unreachable!("CompletedMarker must point to a Start event"),
        }
        Marker::new(new_pos)
    }
}

/// Lex `source`, re-inserting the whitespace logos skips. Comments and DSL
/// lines depend on line context the lexer can't see, so they are recognised
/// here, before the text they cover is lexed:
/// - a column-0 `/` line opens a block comment closed by a column-0 `\` line;
/// - outside one, a column-0 `\` line ends the script;
/// - a `/` at line start or after whitespace comments to end of line;
/// - `k)` / `p)` at line start make the rest of the line a `DslLine`.
fn lex(source: &str) -> Vec<LexedToken> {
    let mut tokens = Vec::new();
    let mut lexer = q_lexer::Token::lexer(source);
    loop {
        let pos = lexer.span().end;
        let rest = source[pos..].trim_start_matches([' ', '\t']);
        let start = source.len() - rest.len();
        if start > pos {
            tokens.push(LexedToken::new(SyntaxKind::Whitespace, &source[pos..start]));
        }
        let line = &rest[..rest.find(['\r', '\n']).unwrap_or(rest.len())];
        let line_start = pos == 0 || source[..pos].ends_with('\n');
        let col0 = line_start && start == pos;
        let special = if col0 && line.trim_end() == "/" {
            Some((SyntaxKind::CommentBlock, block_comment_len(rest)))
        } else if col0 && line.trim_end() == "\\" {
            Some((SyntaxKind::CommentBlock, rest.len()))
        } else if rest.starts_with('/') && (line_start || start > pos) {
            Some((SyntaxKind::LineComment, line.len()))
        } else if line_start && (rest.starts_with("k)") || rest.starts_with("p)")) {
            Some((SyntaxKind::DslLine, line.len()))
        } else {
            None
        };
        if let Some((kind, len)) = special {
            tokens.push(LexedToken::new(kind, &rest[..len]));
            lexer.bump(start - pos + len);
        } else if let Some(result) = lexer.next() {
            let kind = result.map_or(SyntaxKind::Error, SyntaxKind::from_token);
            tokens.push(LexedToken::new(kind, lexer.slice()));
        } else {
            return tokens;
        }
    }
}

/// Length of the block comment opening `rest`: through the first later line
/// that is exactly `\` in column 0 (trailing whitespace allowed), else all.
fn block_comment_len(rest: &str) -> usize {
    let mut len = 0;
    for (i, line) in rest.split_inclusive('\n').enumerate() {
        if i > 0 && line.trim_end() == "\\" {
            return len + line.trim_end_matches(['\r', '\n']).len();
        }
        len += line.len();
    }
    rest.len()
}

/// Merge `-` directly followed by a number or time (not a date or guid,
/// which q cannot negate) into a negative literal unless
/// it directly follows a noun (`x-1`, `f[a]-1`, `1-1` are subtraction;
/// `x -1`, `x*-1`, `x:-1` are negative literals).
fn merge_negative_literals(tokens: &mut Vec<LexedToken>) {
    let mut i = 0;
    while i + 1 < tokens.len() {
        let next_kind = tokens[i + 1].kind;
        let is_sign = tokens[i].kind == SyntaxKind::Minus
            && is_scalar_literal(next_kind)
            && !matches!(next_kind, SyntaxKind::Date | SyntaxKind::Guid)
            && (i == 0 || !ends_noun(tokens[i - 1].kind));
        if is_sign {
            let merged = LexedToken::new(next_kind, &format!("-{}", tokens[i + 1].text));
            tokens.splice(i..=i + 1, [merged]);
        }
        i += 1;
    }
}

/// Can a token end a noun (so a following `-` is subtraction)?
fn ends_noun(kind: SyntaxKind) -> bool {
    is_scalar_literal(kind)
        || matches!(
            kind,
            SyntaxKind::Ident
                | SyntaxKind::DottedIdent
                | SyntaxKind::Boolean
                | SyntaxKind::ByteList
                | SyntaxKind::String
                | SyntaxKind::Symbol
                | SyntaxKind::FileSymbol
                | SyntaxKind::RParen
                | SyntaxKind::RBracket
                | SyntaxKind::RBrace
        )
}

pub struct Parser {
    tokens: Vec<LexedToken>,
    pos: usize,
    /// Indices into `tokens` of every non-trivia token, so lookahead is O(1).
    nt: Vec<usize>,
    /// Rank in `nt` of the next non-trivia token at or after `pos`.
    nt_cursor: usize,
    pub(crate) events: Vec<Event>,
    errors: Vec<ParseError>,
    /// When `true`, juxtaposition stops before `from`, `by`, and `where` so
    /// qSQL clause boundaries are respected.
    pub(crate) qsql_stop: bool,
    /// When `true`, comma is a separator rather than the join dyad (qSQL
    /// column lists and where clauses).
    pub(crate) qsql_comma_stop: bool,
}

impl Parser {
    /// Lex `source` and apply the token-stream fix-ups q's grammar needs.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut tokens = lex(source);
        merge_negative_literals(&mut tokens);

        let nt = (0..tokens.len())
            .filter(|&i| !tokens[i].kind.is_trivia())
            .collect();
        Self {
            tokens,
            pos: 0,
            nt,
            nt_cursor: 0,
            events: Vec::new(),
            errors: Vec::new(),
            qsql_stop: false,
            qsql_comma_stop: false,
        }
    }

    /// Open a new marker at the current position.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Placeholder);
        Marker::new(pos)
    }

    /// Index into `tokens` of the `n`-th non-trivia token from here.
    fn non_trivia_idx(&self, n: usize) -> Option<usize> {
        self.nt.get(self.nt_cursor + n).copied()
    }

    /// Is there a newline between the current non-trivia token and the next?
    /// Used to decide whether adjacent literals belong to the same vector.
    #[must_use]
    pub fn has_newline_after_current(&self) -> bool {
        let Some(cur) = self.non_trivia_idx(0) else {
            return false;
        };
        let next = self.non_trivia_idx(1).unwrap_or(self.tokens.len());
        self.tokens[cur + 1..next]
            .iter()
            .any(|t| t.kind == SyntaxKind::Newline)
    }

    /// Current (non-trivia) token kind.
    #[must_use]
    pub fn current(&self) -> Option<SyntaxKind> {
        self.nth(0)
    }

    /// Kind of the `n`-th non-trivia token from here.
    #[must_use]
    pub fn nth(&self, n: usize) -> Option<SyntaxKind> {
        self.non_trivia_idx(n).map(|i| self.tokens[i].kind)
    }

    /// Text of the current (non-trivia) token.
    #[must_use]
    pub fn current_text(&self) -> Option<&str> {
        self.non_trivia_idx(0).map(|i| self.tokens[i].text.as_str())
    }

    #[must_use]
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == Some(kind)
    }

    #[must_use]
    pub fn at_end(&self) -> bool {
        self.current().is_none()
    }

    /// Does a statement-ending newline precede the next non-trivia token?
    ///
    /// q's continuation rule: a line starting with whitespace continues the
    /// previous statement; a non-blank line in column 0 starts a new one.
    #[must_use]
    pub fn has_preceding_newline(&self) -> bool {
        let last_newline = self.tokens[self.pos..]
            .iter()
            .take_while(|t| t.kind.is_trivia())
            .enumerate()
            .filter(|(_, t)| t.kind == SyntaxKind::Newline)
            .last()
            .map(|(n, _)| n);
        last_newline.is_some_and(|n| {
            self.tokens
                .get(self.pos + n + 1)
                .is_none_or(|t| t.kind != SyntaxKind::Whitespace)
        })
    }

    /// Emit leading trivia tokens, advancing past them.
    pub fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            self.push_token(None);
        }
    }

    /// Emit leading trivia then consume one non-trivia token.
    pub fn bump(&mut self) {
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            self.push_token(None);
            self.nt_cursor += 1;
        }
    }

    /// Emit the token at `pos` (optionally re-kinded) and advance.
    fn push_token(&mut self, kind: Option<SyntaxKind>) {
        let tok = &self.tokens[self.pos];
        self.events.push(Event::Token {
            kind: kind.unwrap_or(tok.kind),
            text: tok.text.clone(),
        });
        self.pos += 1;
    }

    /// Consume the current token if it is `kind`.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        let at = self.at(kind);
        if at {
            self.bump();
        }
        at
    }

    /// Consume `kind` or emit a parse error.
    pub fn expect(&mut self, kind: SyntaxKind) {
        if !self.eat(kind) {
            self.error(format!("expected {kind:?}"));
        }
    }

    /// Record an error spanning the current token and emit that token as
    /// [`SyntaxKind::Error`] so the tree stays lossless.
    pub fn error(&mut self, message: String) {
        let start = self.nt_cursor;
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            self.push_token(Some(SyntaxKind::Error));
            self.nt_cursor += 1;
        }
        self.error_since(message, start);
    }

    /// Rank of the next non-trivia token, for [`Self::error_since`].
    #[must_use]
    pub fn token_index(&self) -> usize {
        self.nt_cursor
    }

    /// Record an error spanning the tokens consumed since `token_index`
    /// (or the current token if none were), without consuming anything.
    pub fn error_since(&mut self, message: impl Into<String>, token_index: usize) {
        let offset = |i: usize| self.tokens[..i].iter().map(|t| t.text.len()).sum::<usize>();
        let (offset, len) = match (self.nt.get(token_index), self.nt_cursor) {
            (None, _) => (offset(self.tokens.len()), 0),
            (Some(&first), cur) => {
                let last = self.nt[cur.max(token_index + 1) - 1];
                let start = offset(first);
                (start, offset(last) + self.tokens[last].text.len() - start)
            }
        };
        self.errors.push(ParseError {
            message: message.into(),
            offset,
            len,
        });
    }

    #[must_use]
    pub fn finish(self) -> (Vec<Event>, Vec<ParseError>) {
        (self.events, self.errors)
    }
}
