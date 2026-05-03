# q/kdb+ 4.1 Language Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a high-performance, LSP-compliant language server for q/kdb+ 4.1 with a full parser, providing diagnostics, completion, hover, go-to-definition, and document symbols.

**Architecture:** Rust workspace with three crates: `lexer` (tokenization via `logos`), `parser` (lossless CST via `rowan` for incremental reparsing and error recovery), and `server` (LSP protocol via `tower-lsp`). The lossless CST approach (inspired by rust-analyzer) enables fast incremental updates, preserves whitespace/comments for formatting, and provides excellent error recovery.

**Tech Stack:** Rust, `tower-lsp` (LSP protocol), `logos` (lexer generator), `rowan` (lossless syntax trees), `tokio` (async runtime), `serde`/`serde_json` (serialization).

---

## File Structure

```
q_ls/
├── Cargo.toml                          # Workspace root
├── crates/
│   ├── lexer/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Public API, re-exports
│   │       └── token.rs                # Token kinds enum + logos lexer
│   ├── parser/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # Public API: parse entry point
│   │       ├── syntax_kind.rs          # SyntaxKind enum (rowan node types)
│   │       ├── event.rs                # Parser events (Start, Token, Finish, Error)
│   │       ├── parser.rs               # Parser state machine + error recovery
│   │       ├── grammar/
│   │       │   ├── mod.rs              # Grammar rule dispatch
│   │       │   ├── expressions.rs      # Expression parsing (atoms, ops, apply)
│   │       │   ├── statements.rs       # Statement parsing (assign, control flow)
│   │       │   ├── qsql.rs            # qSQL: select, update, exec, delete
│   │       │   └── lambdas.rs          # Lambda expressions {[x;y] ...}
│   │       └── sink.rs                 # Converts events -> rowan GreenTree
│   └── server/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # Entry point, stdio transport
│           ├── backend.rs              # LanguageServer trait impl
│           ├── document.rs             # Document state, incremental updates
│           ├── diagnostics.rs          # Parse error -> LSP Diagnostic
│           ├── completion.rs           # Completion provider
│           ├── hover.rs                # Hover provider
│           ├── symbols.rs              # Document/workspace symbols
│           └── goto_def.rs             # Go-to-definition
├── editors/
│   └── vscode/
│       ├── package.json                # VS Code extension manifest
│       ├── src/
│       │   └── extension.ts            # Extension activation
│       └── tsconfig.json
└── test_data/
    ├── basic.q                         # Test fixture: basic expressions
    ├── qsql.q                          # Test fixture: qSQL queries
    └── errors.q                        # Test fixture: intentional parse errors
```

---

## Task 1: Workspace Setup

**Files:**
- Create: `Cargo.toml`
- Create: `crates/lexer/Cargo.toml`
- Create: `crates/lexer/src/lib.rs`
- Create: `crates/parser/Cargo.toml`
- Create: `crates/parser/src/lib.rs`
- Create: `crates/server/Cargo.toml`
- Create: `crates/server/src/main.rs`
- Create: `.gitignore`

- [ ] **Step 1: Initialize git repository**

```bash
cd /Users/hugo/projects/q_ls
git init
```

- [ ] **Step 2: Create workspace Cargo.toml**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
rust-version = "1.85"

[workspace.dependencies]
logos = "0.15"
rowan = "0.16"
tower-lsp = "0.20"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
smol_str = "0.3"
```

- [ ] **Step 3: Create lexer crate Cargo.toml**

```toml
# crates/lexer/Cargo.toml
[package]
name = "q-lexer"
version.workspace = true
edition.workspace = true

[dependencies]
logos = { workspace = true }
```

- [ ] **Step 4: Create lexer lib.rs placeholder**

```rust
// crates/lexer/src/lib.rs
pub mod token;

pub use token::Token;
```

- [ ] **Step 5: Create parser crate Cargo.toml**

```toml
# crates/parser/Cargo.toml
[package]
name = "q-parser"
version.workspace = true
edition.workspace = true

[dependencies]
q-lexer = { path = "../lexer" }
rowan = { workspace = true }
smol_str = { workspace = true }

[dev-dependencies]
expect-test = "1"
```

- [ ] **Step 6: Create parser lib.rs placeholder**

```rust
// crates/parser/src/lib.rs
pub mod syntax_kind;
```

- [ ] **Step 7: Create server crate Cargo.toml**

```toml
# crates/server/Cargo.toml
[package]
name = "q-ls"
version.workspace = true
edition.workspace = true

[dependencies]
q-lexer = { path = "../lexer" }
q-parser = { path = "../parser" }
tower-lsp = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 8: Create server main.rs placeholder**

```rust
// crates/server/src/main.rs
fn main() {
    println!("q-ls starting...");
}
```

- [ ] **Step 9: Create .gitignore**

```
/target
*.swp
*.swo
.DS_Store
```

- [ ] **Step 10: Verify workspace builds**

Run: `cargo build`
Expected: Compiles successfully with no errors.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat: initialize rust workspace with lexer, parser, and server crates"
```

---

## Task 2: Lexer - Token Definition

**Files:**
- Create: `crates/lexer/src/token.rs`
- Test: inline `#[cfg(test)]` module in `token.rs`

- [ ] **Step 1: Write the failing test**

```rust
// crates/lexer/src/token.rs
#[cfg(test)]
mod tests {
    use super::*;
    use logos::Logos;

    #[test]
    fn lex_integer() {
        let mut lex = Token::lexer("42");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
        assert_eq!(lex.slice(), "42");
    }

    #[test]
    fn lex_symbol() {
        let mut lex = Token::lexer("`hello");
        assert_eq!(lex.next(), Some(Ok(Token::Symbol)));
        assert_eq!(lex.slice(), "`hello");
    }

    #[test]
    fn lex_identifier() {
        let mut lex = Token::lexer("trade");
        assert_eq!(lex.next(), Some(Ok(Token::Ident)));
        assert_eq!(lex.slice(), "trade");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p q-lexer`
Expected: FAIL - Token type not defined.

- [ ] **Step 3: Implement Token enum with logos**

```rust
// crates/lexer/src/token.rs
use logos::Logos;

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t]+")]
pub enum Token {
    // === Literals ===

    // Integers: 42, 42i, 42j, 0x2A, 0Ni, 0Wi, -0Wi
    #[regex(r"-?[0-9]+[ihje]?")]
    #[regex(r"0x[0-9a-fA-F]+")]
    #[regex(r"0[NW][ihje]?")]
    Integer,

    // Floats: 3.14, 3.14e10, 3.14f, 0n, 0w, 0Nf, 0Wf
    #[regex(r"-?[0-9]+\.[0-9]*([eE][+-]?[0-9]+)?[fe]?")]
    #[regex(r"-?\.[0-9]+([eE][+-]?[0-9]+)?[fe]?")]
    #[regex(r"0[nw]")]
    #[regex(r"0[NW][fe]?")]
    Float,

    // Booleans: 0b, 1b, 010101b
    #[regex(r"[01]+b")]
    Boolean,

    // Characters: "a", "ab" (char list)
    #[regex(r#""([^"\\]|\\.)*""#)]
    String,

    // Symbols: `sym, `a.b, ` (null symbol)
    #[regex(r"`[a-zA-Z0-9_.]*")]
    Symbol,

    // Dates: 2024.01.15, 0Nd
    #[regex(r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}[dpmnuvt]?")]
    #[regex(r"0N[dpmnuvt]")]
    Date,

    // Times: 12:30:00.000, 0Nt
    #[regex(r"[0-9]{2}:[0-9]{2}(:[0-9]{2}(\.[0-9]+)?)?")]
    Time,

    // Timestamps: 2024.01.15D12:30:00.000000000
    #[regex(r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}D[0-9]{2}:[0-9]{2}(:[0-9]{2}(\.[0-9]+)?)?")]
    Timestamp,

    // === Identifiers & Keywords ===

    #[regex(r"[a-zA-Z][a-zA-Z0-9_.]*")]
    Ident,

    // Dotted identifiers (namespaces): .q.func, .Q.en
    #[regex(r"\.[a-zA-Z][a-zA-Z0-9_.]*")]
    DottedIdent,

    // === Operators ===

    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("%")]
    Percent,
    #[token("!")]
    Bang,
    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("#")]
    Hash,
    #[token("_")]
    Underscore,
    #[token("~")]
    Tilde,
    #[token("$")]
    Dollar,
    #[token("?")]
    Question,
    #[token("@")]
    At,
    #[token(".")]
    Dot,
    #[token(",")]
    Comma,

    // Comparison
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,

    // Assignment
    #[token(":")]
    Colon,
    #[token("::")]
    ColonColon,

    // Adverbs
    #[token("':")]
    EachPrior,
    #[token("/:")]
    EachRight,
    #[token("\\:")]
    EachLeft,

    // === Delimiters ===

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,

    // === Separators ===

    #[token(";")]
    Semi,

    // === Newline (significant in q) ===

    #[regex(r"\r?\n")]
    Newline,

    // === Comments ===

    // Line comment: / at start of line or after whitespace
    #[regex(r"/[^\n]*")]
    LineComment,

    // === System commands ===

    // \l, \t, \d, etc.
    #[regex(r"\\[a-zA-Z][^\n]*")]
    SystemCmd,

    // Exit: \\
    #[token("\\\\")]
    Exit,

    // Single backslash (adverb: scan/over)
    #[token("\\")]
    Backslash,

    // Single forward slash (adverb: over/converge)
    #[token("/")]
    Slash,

    // === Error ===
    Error,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p q-lexer`
Expected: All 3 tests PASS.

- [ ] **Step 5: Add comprehensive lexer tests**

```rust
// Append to the tests module in crates/lexer/src/token.rs

    #[test]
    fn lex_float() {
        let mut lex = Token::lexer("3.14");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_boolean() {
        let mut lex = Token::lexer("01b");
        assert_eq!(lex.next(), Some(Ok(Token::Boolean)));
    }

    #[test]
    fn lex_operators() {
        let input = "+ - * %";
        let tokens: Vec<_> = Token::lexer(input)
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(tokens, vec![Token::Plus, Token::Minus, Token::Star, Token::Percent]);
    }

    #[test]
    fn lex_assignment() {
        let mut lex = Token::lexer("x:42");
        assert_eq!(lex.next(), Some(Ok(Token::Ident)));
        assert_eq!(lex.next(), Some(Ok(Token::Colon)));
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_lambda() {
        let input = "{[x;y] x+y}";
        let tokens: Vec<_> = Token::lexer(input)
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(tokens, vec![
            Token::LBrace, Token::LBracket, Token::Ident, Token::Semi,
            Token::Ident, Token::RBracket, Token::Ident, Token::Plus,
            Token::Ident, Token::RBrace,
        ]);
    }

    #[test]
    fn lex_dotted_ident() {
        let mut lex = Token::lexer(".q.func");
        assert_eq!(lex.next(), Some(Ok(Token::DottedIdent)));
        assert_eq!(lex.slice(), ".q.func");
    }

    #[test]
    fn lex_string() {
        let mut lex = Token::lexer(r#""hello world""#);
        assert_eq!(lex.next(), Some(Ok(Token::String)));
    }

    #[test]
    fn lex_system_cmd() {
        let mut lex = Token::lexer("\\l file.q");
        assert_eq!(lex.next(), Some(Ok(Token::SystemCmd)));
    }

    #[test]
    fn lex_global_assign() {
        let mut lex = Token::lexer("::");
        assert_eq!(lex.next(), Some(Ok(Token::ColonColon)));
    }
```

- [ ] **Step 6: Run all lexer tests**

Run: `cargo test -p q-lexer`
Expected: All tests PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/lexer/
git commit -m "feat(lexer): implement q token definitions with logos"
```

---

## Task 3: Parser - SyntaxKind and Rowan Integration

**Files:**
- Create: `crates/parser/src/syntax_kind.rs`
- Modify: `crates/parser/src/lib.rs`

- [ ] **Step 1: Write SyntaxKind enum**

```rust
// crates/parser/src/syntax_kind.rs
use q_lexer::Token;

/// All syntax node and token kinds in the q CST.
/// Tokens map 1:1 from the lexer. Nodes represent composite structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // === Tokens (from lexer) ===
    Integer = 0,
    Float,
    Boolean,
    String,
    Symbol,
    Date,
    Time,
    Timestamp,
    Ident,
    DottedIdent,
    Plus,
    Minus,
    Star,
    Percent,
    Bang,
    Ampersand,
    Pipe,
    Caret,
    Hash,
    Underscore,
    Tilde,
    Dollar,
    Question,
    At,
    Dot,
    Comma,
    Eq,
    Lt,
    Gt,
    Colon,
    ColonColon,
    EachPrior,
    EachRight,
    EachLeft,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Semi,
    Newline,
    LineComment,
    SystemCmd,
    Exit,
    Backslash,
    Slash,
    Error,
    Whitespace,

    // === Composite Nodes ===
    Root,              // Top-level file
    ExprStmt,          // Expression statement
    AssignStmt,        // x: expr or x:: expr
    Lambda,            // {[params] body}
    ParamList,         // [x;y;z]
    ArgList,           // [expr;expr;...]
    BinExpr,           // left op right
    UnaryExpr,         // op expr (monadic)
    ApplyExpr,         // f[args] or f x
    AdverbExpr,        // expr adverb
    ParenExpr,         // (expr)
    ListExpr,          // (expr;expr;...)
    CondExpr,          // $[cond;true;false]
    IdentExpr,         // simple identifier
    LiteralExpr,       // any literal value
    TableExpr,         // ([] col:val; ...)
    DictExpr,          // keys!values
    SelectExpr,        // select cols by groups from table where conds
    UpdateExpr,        // update cols by groups from table where conds
    ExecExpr,          // exec cols by groups from table where conds
    DeleteExpr,        // delete cols from table where conds
    ColumnList,        // column specifications in qSQL
    WhereClause,       // where clause in qSQL
    ByClause,          // by clause in qSQL
    SystemCmdStmt,     // system command statement
    Block,             // sequence of statements
    IndexExpr,         // expr[index]

    // Sentinel
    __LAST,
}

impl SyntaxKind {
    pub fn from_token(token: Token) -> Self {
        match token {
            Token::Integer => Self::Integer,
            Token::Float => Self::Float,
            Token::Boolean => Self::Boolean,
            Token::String => Self::String,
            Token::Symbol => Self::Symbol,
            Token::Date => Self::Date,
            Token::Time => Self::Time,
            Token::Timestamp => Self::Timestamp,
            Token::Ident => Self::Ident,
            Token::DottedIdent => Self::DottedIdent,
            Token::Plus => Self::Plus,
            Token::Minus => Self::Minus,
            Token::Star => Self::Star,
            Token::Percent => Self::Percent,
            Token::Bang => Self::Bang,
            Token::Ampersand => Self::Ampersand,
            Token::Pipe => Self::Pipe,
            Token::Caret => Self::Caret,
            Token::Hash => Self::Hash,
            Token::Underscore => Self::Underscore,
            Token::Tilde => Self::Tilde,
            Token::Dollar => Self::Dollar,
            Token::Question => Self::Question,
            Token::At => Self::At,
            Token::Dot => Self::Dot,
            Token::Comma => Self::Comma,
            Token::Eq => Self::Eq,
            Token::Lt => Self::Lt,
            Token::Gt => Self::Gt,
            Token::Colon => Self::Colon,
            Token::ColonColon => Self::ColonColon,
            Token::EachPrior => Self::EachPrior,
            Token::EachRight => Self::EachRight,
            Token::EachLeft => Self::EachLeft,
            Token::LParen => Self::LParen,
            Token::RParen => Self::RParen,
            Token::LBracket => Self::LBracket,
            Token::RBracket => Self::RBracket,
            Token::LBrace => Self::LBrace,
            Token::RBrace => Self::RBrace,
            Token::Semi => Self::Semi,
            Token::Newline => Self::Newline,
            Token::LineComment => Self::LineComment,
            Token::SystemCmd => Self::SystemCmd,
            Token::Exit => Self::Exit,
            Token::Backslash => Self::Backslash,
            Token::Slash => Self::Slash,
            Token::Error => Self::Error,
        }
    }

    pub fn is_trivia(self) -> bool {
        matches!(self, Self::Whitespace | Self::Newline | Self::LineComment)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// The q language definition for rowan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QLang {}

impl rowan::Language for QLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 < SyntaxKind::__LAST as u16);
        // SAFETY: SyntaxKind is repr(u16) and we checked bounds
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<QLang>;
pub type SyntaxToken = rowan::SyntaxToken<QLang>;
pub type SyntaxElement = rowan::SyntaxElement<QLang>;
```

- [ ] **Step 2: Update parser lib.rs**

```rust
// crates/parser/src/lib.rs
pub mod syntax_kind;

pub use syntax_kind::{QLang, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxElement};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p q-parser`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add crates/parser/
git commit -m "feat(parser): define SyntaxKind enum and rowan language integration"
```

---

## Task 4: Parser - Event-Based Parsing Infrastructure

**Files:**
- Create: `crates/parser/src/event.rs`
- Create: `crates/parser/src/parser.rs`
- Create: `crates/parser/src/sink.rs`
- Modify: `crates/parser/src/lib.rs`

- [ ] **Step 1: Write the event types**

```rust
// crates/parser/src/event.rs
use crate::syntax_kind::SyntaxKind;

/// Events emitted by the parser. These are later converted into a rowan GreenTree.
#[derive(Debug, Clone)]
pub enum Event {
    /// Start a new node. `forward_parent` is used for left-recursive structures
    /// (e.g., binary expressions) where we need to wrap a previously-started node.
    Start {
        kind: SyntaxKind,
        forward_parent: Option<usize>,
    },
    /// A token to add to the current node.
    Token {
        kind: SyntaxKind,
        text: String,
    },
    /// Finish the current node.
    Finish,
    /// A placeholder that will be replaced during tree construction.
    Placeholder,
}
```

- [ ] **Step 2: Write the parser state machine**

```rust
// crates/parser/src/parser.rs
use crate::event::Event;
use crate::syntax_kind::SyntaxKind;
use q_lexer::Token;
use logos::Logos;

/// A parse error with location information.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub offset: usize,
    pub len: usize,
}

/// A mark that represents the start of a node. Can be used to wrap
/// previously parsed content in a new parent node (precede).
#[derive(Debug, Clone, Copy)]
pub struct Marker {
    pos: usize,
}

impl Marker {
    /// Complete this marker, wrapping all tokens since it was opened in a node of `kind`.
    pub fn complete(self, p: &mut Parser, kind: SyntaxKind) -> CompletedMarker {
        let event = &mut p.events[self.pos];
        assert!(matches!(event, Event::Placeholder));
        *event = Event::Start {
            kind,
            forward_parent: None,
        };
        p.events.push(Event::Finish);
        CompletedMarker { pos: self.pos }
    }

    /// Abandon this marker (don't create a node).
    pub fn abandon(self, p: &mut Parser) {
        let event = &mut p.events[self.pos];
        assert!(matches!(event, Event::Placeholder));
        if self.pos == p.events.len() - 1 {
            p.events.pop();
        }
    }
}

/// A completed marker that can be used to wrap it in a new parent (precede).
#[derive(Debug, Clone, Copy)]
pub struct CompletedMarker {
    pos: usize,
}

impl CompletedMarker {
    /// Wrap this completed node in a new parent node, using forward_parent linking.
    pub fn precede(self, p: &mut Parser) -> Marker {
        let new_pos = p.start();
        if let Event::Start { forward_parent, .. } = &mut p.events[self.pos] {
            *forward_parent = Some(new_pos.pos);
        }
        new_pos
    }
}

/// Parsed token from source.
#[derive(Debug, Clone)]
pub struct LexedToken {
    pub kind: SyntaxKind,
    pub text: String,
}

/// The parser. Consumes tokens and emits events that build the CST.
pub struct Parser {
    tokens: Vec<LexedToken>,
    pos: usize,
    pub(crate) events: Vec<Event>,
    pub(crate) errors: Vec<ParseError>,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let mut tokens = Vec::new();

        let mut lexer = Token::lexer(source);
        let mut last_end = 0;

        while let Some(result) = lexer.next() {
            let span = lexer.span();

            // Capture any whitespace between tokens
            if span.start > last_end {
                tokens.push(LexedToken {
                    kind: SyntaxKind::Whitespace,
                    text: source[last_end..span.start].to_string(),
                });
            }

            let token = result.unwrap_or(Token::Error);
            tokens.push(LexedToken {
                kind: SyntaxKind::from_token(token),
                text: source[span.start..span.end].to_string(),
            });

            last_end = span.end;
        }

        // Trailing whitespace
        if last_end < source.len() {
            tokens.push(LexedToken {
                kind: SyntaxKind::Whitespace,
                text: source[last_end..].to_string(),
            });
        }

        Self {
            tokens,
            pos: 0,
            events: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Start a new node. Returns a Marker.
    pub fn start(&mut self) -> Marker {
        let pos = self.events.len();
        self.events.push(Event::Placeholder);
        Marker { pos }
    }

    /// Look at current token kind (skipping trivia for lookahead).
    pub fn current(&self) -> Option<SyntaxKind> {
        self.nth(0)
    }

    /// Look ahead n non-trivia tokens.
    pub fn nth(&self, n: usize) -> Option<SyntaxKind> {
        let mut count = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            let kind = self.tokens[i].kind;
            if !kind.is_trivia() {
                if count == n {
                    return Some(kind);
                }
                count += 1;
            }
            i += 1;
        }
        None
    }

    /// Get the text of the current non-trivia token.
    pub fn current_text(&self) -> Option<String> {
        self.nth_text(0)
    }

    /// Get the text of the nth non-trivia token.
    pub fn nth_text(&self, n: usize) -> Option<String> {
        let mut count = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            let kind = self.tokens[i].kind;
            if !kind.is_trivia() {
                if count == n {
                    return Some(self.tokens[i].text.clone());
                }
                count += 1;
            }
            i += 1;
        }
        None
    }

    /// Check if current token matches kind (ignoring trivia).
    pub fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == Some(kind)
    }

    /// Consume the current token (including any leading trivia).
    pub fn bump(&mut self) {
        self.eat_trivia();
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token {
                kind: tok.kind,
                text: tok.text.clone(),
            });
            self.pos += 1;
        }
    }

    /// Consume only if current matches `kind`.
    pub fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Expect a token of `kind`, emitting an error if not found.
    pub fn expect(&mut self, kind: SyntaxKind) {
        if !self.eat(kind) {
            self.error(format!("expected {:?}", kind));
        }
    }

    /// Consume trivia tokens (whitespace, comments, newlines) and attach to current node.
    pub fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.tokens[self.pos].kind.is_trivia() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token {
                kind: tok.kind,
                text: tok.text.clone(),
            });
            self.pos += 1;
        }
    }

    /// Report a parse error.
    pub fn error(&mut self, msg: String) {
        let offset = self.current_offset();
        let len = if self.pos < self.tokens.len() {
            self.tokens[self.pos].text.len()
        } else {
            0
        };
        self.errors.push(ParseError { message: msg, offset, len });
        // Consume the unexpected token as an Error node
        if self.pos < self.tokens.len() {
            let tok = &self.tokens[self.pos];
            self.events.push(Event::Token {
                kind: SyntaxKind::Error,
                text: tok.text.clone(),
            });
            self.pos += 1;
        }
    }

    /// Get byte offset of current position in source.
    fn current_offset(&self) -> usize {
        self.tokens[..self.pos]
            .iter()
            .map(|t| t.text.len())
            .sum()
    }

    /// Check if we've consumed all tokens.
    pub fn at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.current().is_none()
    }

    /// Finish parsing and return events + errors.
    pub fn finish(self) -> (Vec<Event>, Vec<ParseError>) {
        (self.events, self.errors)
    }
}
```

- [ ] **Step 3: Write the sink (events -> GreenTree)**

```rust
// crates/parser/src/sink.rs
use crate::event::Event;
use crate::syntax_kind::SyntaxKind;
use crate::parser::ParseError;
use rowan::{GreenNode, GreenNodeBuilder};

/// Converts parser events into a rowan GreenNode.
pub struct Sink {
    builder: GreenNodeBuilder<'static>,
    events: Vec<Event>,
    errors: Vec<ParseError>,
}

impl Sink {
    pub fn new(events: Vec<Event>, errors: Vec<ParseError>) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            events,
            errors,
        }
    }

    pub fn finish(mut self) -> (GreenNode, Vec<ParseError>) {
        // Resolve forward_parent links
        let mut forward_parents: Vec<usize> = Vec::new();

        for i in 0..self.events.len() {
            match &self.events[i] {
                Event::Start { forward_parent: Some(_), .. } => {
                    // Walk the forward_parent chain
                    let mut idx = i;
                    let mut chain = Vec::new();

                    loop {
                        chain.push(idx);
                        match &self.events[idx] {
                            Event::Start { forward_parent: Some(fp), .. } => {
                                idx = *fp;
                            }
                            _ => break,
                        }
                    }

                    for &c in chain.iter().rev() {
                        if let Event::Start { kind, .. } = &self.events[c] {
                            self.builder.start_node(rowan::SyntaxKind((*kind) as u16));
                        }
                    }

                    forward_parents.extend(chain.iter());
                }
                Event::Start { kind, forward_parent: None } => {
                    if !forward_parents.contains(&i) {
                        self.builder.start_node(rowan::SyntaxKind((*kind) as u16));
                    }
                }
                Event::Token { kind, text } => {
                    self.builder.token(rowan::SyntaxKind((*kind) as u16), text);
                }
                Event::Finish => {
                    self.builder.finish_node();
                }
                Event::Placeholder => {}
            }
        }

        (self.builder.finish(), self.errors)
    }
}
```

- [ ] **Step 4: Update lib.rs with the public parse function**

```rust
// crates/parser/src/lib.rs
pub mod event;
pub mod grammar;
pub mod parser;
pub mod syntax_kind;
pub mod sink;

pub use syntax_kind::{QLang, SyntaxKind, SyntaxNode, SyntaxToken, SyntaxElement};
pub use parser::ParseError;

use rowan::GreenNode;

/// Parse q source code and return a syntax tree + errors.
pub fn parse(source: &str) -> Parse {
    let mut p = parser::Parser::new(source);

    let m = p.start();
    grammar::root(&mut p);
    p.eat_trivia(); // trailing trivia
    m.complete(&mut p, SyntaxKind::Root);

    let (events, errors) = p.finish();
    let (green, errors) = sink::Sink::new(events, errors).finish();

    Parse { green, errors }
}

/// The result of parsing.
#[derive(Debug)]
pub struct Parse {
    green: GreenNode,
    pub errors: Vec<ParseError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    pub fn green(&self) -> &GreenNode {
        &self.green
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_preserves_text() {
        let source = "x:42";
        let parse = parse(source);
        let node = parse.syntax();
        assert_eq!(node.text().to_string(), source);
    }

    #[test]
    fn parse_returns_root_node() {
        let source = "1+2";
        let parse = parse(source);
        let node = parse.syntax();
        assert_eq!(node.kind(), SyntaxKind::Root.into());
    }
}
```

- [ ] **Step 5: Create grammar/mod.rs stub (needed for compilation)**

```rust
// crates/parser/src/grammar/mod.rs
use crate::parser::Parser;

pub fn root(p: &mut Parser) {
    while !p.at_end() {
        p.bump();
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p q-parser`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/parser/
git commit -m "feat(parser): event-based parsing infrastructure with rowan sink"
```

---

## Task 5: Parser - Expression Grammar

**Files:**
- Create: `crates/parser/src/grammar/expressions.rs`
- Modify: `crates/parser/src/grammar/mod.rs`

- [ ] **Step 1: Write failing tests for expression parsing**

Add to `crates/parser/src/lib.rs` tests module:

```rust
    #[test]
    fn parse_binary_expr() {
        let parse = parse("1+2");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        let root = parse.syntax();
        assert_eq!(root.text().to_string(), "1+2");
    }

    #[test]
    fn parse_assignment() {
        let parse = parse("x:42");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_lambda() {
        let parse = parse("{[x;y] x+y}");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_preserves_whitespace() {
        let source = "x : 42 + 3";
        let parse = parse(source);
        assert_eq!(parse.syntax().text().to_string(), source);
    }

    #[test]
    fn parse_right_to_left() {
        let parse = parse("2*3+4");
        assert!(parse.errors.is_empty());
    }

    #[test]
    fn parse_error_recovery() {
        let parse = parse(")invalid[");
        assert!(!parse.errors.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p q-parser`
Expected: Some FAIL because grammar stub just bumps all tokens.

- [ ] **Step 3: Implement expression parsing**

```rust
// crates/parser/src/grammar/expressions.rs
use crate::parser::{CompletedMarker, Parser};
use crate::syntax_kind::SyntaxKind;

/// Parse an expression. q evaluates right-to-left, so binary ops are right-associative.
pub fn expr(p: &mut Parser) {
    expr_bp(p, 0);
}

/// Expression with binding power (for right-to-left evaluation).
fn expr_bp(p: &mut Parser, min_bp: u8) {
    let Some(mut lhs) = atom(p) else {
        return;
    };

    loop {
        // Check for adverbs (', /, \, ':, /:, \:)
        if let Some(_adverb_kind) = adverb_token(p) {
            let m = lhs.precede(p);
            p.bump();
            lhs = m.complete(p, SyntaxKind::AdverbExpr);
            continue;
        }

        // Check for indexing: expr[...]
        if p.at(SyntaxKind::LBracket) {
            let m = lhs.precede(p);
            parse_arg_list(p);
            lhs = m.complete(p, SyntaxKind::IndexExpr);
            continue;
        }

        // Check for binary operator
        let Some(_op) = binary_op(p) else {
            break;
        };

        let (l_bp, r_bp) = binding_power();
        if l_bp < min_bp {
            break;
        }

        let m = lhs.precede(p);
        p.bump(); // consume operator

        // Right-associative: use r_bp for RHS
        expr_bp(p, r_bp);
        lhs = m.complete(p, SyntaxKind::BinExpr);
    }
}

/// Parse an atomic expression.
fn atom(p: &mut Parser) -> Option<CompletedMarker> {
    match p.current()? {
        // Literals
        SyntaxKind::Integer
        | SyntaxKind::Float
        | SyntaxKind::Boolean
        | SyntaxKind::String
        | SyntaxKind::Symbol
        | SyntaxKind::Date
        | SyntaxKind::Time
        | SyntaxKind::Timestamp => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralExpr))
        }

        // Identifiers
        SyntaxKind::Ident | SyntaxKind::DottedIdent => {
            let m = p.start();
            p.bump();
            Some(m.complete(p, SyntaxKind::IdentExpr))
        }

        // Unary operators (monadic use of verbs)
        SyntaxKind::Minus
        | SyntaxKind::Plus
        | SyntaxKind::Star
        | SyntaxKind::Percent
        | SyntaxKind::Bang
        | SyntaxKind::Ampersand
        | SyntaxKind::Pipe
        | SyntaxKind::Caret
        | SyntaxKind::Hash
        | SyntaxKind::Tilde
        | SyntaxKind::At
        | SyntaxKind::Question
        | SyntaxKind::Underscore => {
            let m = p.start();
            p.bump();
            expr_bp(p, 100);
            Some(m.complete(p, SyntaxKind::UnaryExpr))
        }

        // Parenthesized expression or list
        SyntaxKind::LParen => {
            let m = p.start();
            p.bump(); // (

            if p.at(SyntaxKind::RParen) {
                p.bump();
                return Some(m.complete(p, SyntaxKind::ListExpr));
            }

            // Check for table literal: ([] ...)
            if p.at(SyntaxKind::LBracket) {
                if p.nth(1) == Some(SyntaxKind::RBracket) {
                    p.bump(); // [
                    p.bump(); // ]
                    while !p.at(SyntaxKind::RParen) && !p.at_end() {
                        expr(p);
                        if !p.eat(SyntaxKind::Semi) {
                            break;
                        }
                    }
                    p.expect(SyntaxKind::RParen);
                    return Some(m.complete(p, SyntaxKind::TableExpr));
                }
            }

            expr(p);

            if p.at(SyntaxKind::Semi) {
                while p.eat(SyntaxKind::Semi) {
                    if !p.at(SyntaxKind::RParen) {
                        expr(p);
                    }
                }
                p.expect(SyntaxKind::RParen);
                Some(m.complete(p, SyntaxKind::ListExpr))
            } else {
                p.expect(SyntaxKind::RParen);
                Some(m.complete(p, SyntaxKind::ParenExpr))
            }
        }

        // Lambda: {[params] body} or {body}
        SyntaxKind::LBrace => {
            let m = p.start();
            p.bump(); // {

            // Optional parameter list
            if p.at(SyntaxKind::LBracket) {
                let pm = p.start();
                p.bump(); // [
                while !p.at(SyntaxKind::RBracket) && !p.at_end() {
                    p.expect(SyntaxKind::Ident);
                    if !p.eat(SyntaxKind::Semi) {
                        break;
                    }
                }
                p.expect(SyntaxKind::RBracket);
                pm.complete(p, SyntaxKind::ParamList);
            }

            // Body: sequence of expressions separated by ;
            while !p.at(SyntaxKind::RBrace) && !p.at_end() {
                expr(p);
                if !p.eat(SyntaxKind::Semi) {
                    break;
                }
            }
            p.expect(SyntaxKind::RBrace);
            Some(m.complete(p, SyntaxKind::Lambda))
        }

        // Conditional: $[cond;true;false]
        SyntaxKind::Dollar => {
            if p.nth(1) == Some(SyntaxKind::LBracket) {
                let m = p.start();
                p.bump(); // $
                parse_arg_list(p);
                Some(m.complete(p, SyntaxKind::CondExpr))
            } else {
                let m = p.start();
                p.bump();
                expr_bp(p, 100);
                Some(m.complete(p, SyntaxKind::UnaryExpr))
            }
        }

        _ => {
            p.error(format!("unexpected token: {:?}", p.current()));
            None
        }
    }
}

/// Parse bracketed argument list: [expr;expr;...]
pub fn parse_arg_list(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LBracket);
    while !p.at(SyntaxKind::RBracket) && !p.at_end() {
        expr(p);
        if !p.eat(SyntaxKind::Semi) {
            break;
        }
    }
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::ArgList);
}

/// Check if current token is a binary operator.
fn binary_op(p: &Parser) -> Option<SyntaxKind> {
    let kind = p.current()?;
    match kind {
        SyntaxKind::Plus
        | SyntaxKind::Minus
        | SyntaxKind::Star
        | SyntaxKind::Percent
        | SyntaxKind::Bang
        | SyntaxKind::Ampersand
        | SyntaxKind::Pipe
        | SyntaxKind::Caret
        | SyntaxKind::Hash
        | SyntaxKind::Underscore
        | SyntaxKind::Tilde
        | SyntaxKind::At
        | SyntaxKind::Question
        | SyntaxKind::Dot
        | SyntaxKind::Comma
        | SyntaxKind::Eq
        | SyntaxKind::Lt
        | SyntaxKind::Gt => Some(kind),
        _ => None,
    }
}

/// Binding power for operators. q is right-to-left, all operators equal precedence.
fn binding_power() -> (u8, u8) {
    (1, 0)
}

/// Check if current token is an adverb.
fn adverb_token(p: &Parser) -> Option<SyntaxKind> {
    let kind = p.current()?;
    match kind {
        SyntaxKind::Slash
        | SyntaxKind::Backslash
        | SyntaxKind::EachPrior
        | SyntaxKind::EachRight
        | SyntaxKind::EachLeft => Some(kind),
        _ => None,
    }
}
```

- [ ] **Step 4: Update grammar/mod.rs to use expressions**

```rust
// crates/parser/src/grammar/mod.rs
pub mod expressions;

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

/// Parse the top-level file: a sequence of statements.
pub fn root(p: &mut Parser) {
    while !p.at_end() {
        statement(p);
    }
}

/// Parse a single statement.
pub fn statement(p: &mut Parser) {
    // Skip bare newlines between statements
    while p.at(SyntaxKind::Newline) {
        p.bump();
    }
    if p.at_end() {
        return;
    }

    // System command
    if p.at(SyntaxKind::SystemCmd) || p.at(SyntaxKind::Exit) {
        let m = p.start();
        p.bump();
        m.complete(p, SyntaxKind::SystemCmdStmt);
        return;
    }

    // Expression or assignment
    let m = p.start();
    expressions::expr(p);

    if p.at(SyntaxKind::Colon) || p.at(SyntaxKind::ColonColon) {
        p.bump();
        expressions::expr(p);
        m.complete(p, SyntaxKind::AssignStmt);
    } else {
        m.complete(p, SyntaxKind::ExprStmt);
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p q-parser`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/parser/
git commit -m "feat(parser): implement expression grammar with right-to-left evaluation"
```

---

## Task 6: Parser - qSQL Grammar

**Files:**
- Create: `crates/parser/src/grammar/qsql.rs`
- Modify: `crates/parser/src/grammar/mod.rs`

- [ ] **Step 1: Write failing tests for qSQL**

Add to `crates/parser/src/lib.rs` tests module:

```rust
    #[test]
    fn parse_select() {
        let parse = parse("select price,size from trade where sym=`AAPL");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
        assert_eq!(parse.syntax().text().to_string(), "select price,size from trade where sym=`AAPL");
    }

    #[test]
    fn parse_select_by() {
        let parse = parse("select avg price by sym from trade");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_update() {
        let parse = parse("update price:price*1.1 from trade where sym=`AAPL");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }

    #[test]
    fn parse_delete() {
        let parse = parse("delete from trade where price<0");
        assert!(parse.errors.is_empty(), "errors: {:?}", parse.errors);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p q-parser`
Expected: FAIL - qSQL keywords parsed as plain identifiers.

- [ ] **Step 3: Implement qSQL parsing**

```rust
// crates/parser/src/grammar/qsql.rs
use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;
use super::expressions;

/// Check if current identifier is a qSQL keyword.
pub fn at_qsql_keyword(p: &Parser) -> bool {
    if p.current() != Some(SyntaxKind::Ident) {
        return false;
    }
    matches!(
        p.current_text().as_deref(),
        Some("select" | "exec" | "update" | "delete")
    )
}

/// Parse a qSQL expression. Returns true if parsed.
pub fn try_parse_qsql(p: &mut Parser) -> bool {
    let text = match p.current_text() {
        Some(t) => t,
        None => return false,
    };

    match text.as_str() {
        "select" => { parse_select(p); true }
        "exec" => { parse_exec(p); true }
        "update" => { parse_update(p); true }
        "delete" => { parse_delete(p); true }
        _ => false,
    }
}

fn parse_select(p: &mut Parser) {
    let m = p.start();
    p.bump(); // select

    if !at_keyword(p, "from") && !at_keyword(p, "by") && !p.at_end() {
        parse_column_list(p);
    }

    if at_keyword(p, "by") {
        let bm = p.start();
        p.bump();
        parse_column_list(p);
        bm.complete(p, SyntaxKind::ByClause);
    }

    if at_keyword(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_keyword(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_conditions(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::SelectExpr);
}

fn parse_exec(p: &mut Parser) {
    let m = p.start();
    p.bump(); // exec

    if !at_keyword(p, "from") && !at_keyword(p, "by") && !p.at_end() {
        parse_column_list(p);
    }

    if at_keyword(p, "by") {
        let bm = p.start();
        p.bump();
        parse_column_list(p);
        bm.complete(p, SyntaxKind::ByClause);
    }

    if at_keyword(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_keyword(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_conditions(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::ExecExpr);
}

fn parse_update(p: &mut Parser) {
    let m = p.start();
    p.bump(); // update

    if !at_keyword(p, "from") && !p.at_end() {
        parse_column_list(p);
    }

    if at_keyword(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_keyword(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_conditions(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::UpdateExpr);
}

fn parse_delete(p: &mut Parser) {
    let m = p.start();
    p.bump(); // delete

    if !at_keyword(p, "from") && !p.at_end() {
        parse_column_list(p);
    }

    if at_keyword(p, "from") {
        p.bump();
        expressions::expr(p);
    }

    if at_keyword(p, "where") {
        let wm = p.start();
        p.bump();
        parse_where_conditions(p);
        wm.complete(p, SyntaxKind::WhereClause);
    }

    m.complete(p, SyntaxKind::DeleteExpr);
}

fn parse_column_list(p: &mut Parser) {
    let m = p.start();
    loop {
        expressions::expr(p);
        if !p.eat(SyntaxKind::Comma) {
            break;
        }
    }
    m.complete(p, SyntaxKind::ColumnList);
}

fn parse_where_conditions(p: &mut Parser) {
    loop {
        expressions::expr(p);
        if !p.eat(SyntaxKind::Comma) {
            break;
        }
    }
}

fn at_keyword(p: &Parser, kw: &str) -> bool {
    p.at(SyntaxKind::Ident) && p.current_text().as_deref() == Some(kw)
}
```

- [ ] **Step 4: Wire qSQL into the statement parser**

Update `crates/parser/src/grammar/mod.rs`:

```rust
// crates/parser/src/grammar/mod.rs
pub mod expressions;
pub mod qsql;

use crate::parser::Parser;
use crate::syntax_kind::SyntaxKind;

pub fn root(p: &mut Parser) {
    while !p.at_end() {
        statement(p);
    }
}

pub fn statement(p: &mut Parser) {
    while p.at(SyntaxKind::Newline) {
        p.bump();
    }
    if p.at_end() {
        return;
    }

    if p.at(SyntaxKind::SystemCmd) || p.at(SyntaxKind::Exit) {
        let m = p.start();
        p.bump();
        m.complete(p, SyntaxKind::SystemCmdStmt);
        return;
    }

    // Try qSQL
    if p.at(SyntaxKind::Ident) && qsql::at_qsql_keyword(p) {
        let m = p.start();
        qsql::try_parse_qsql(p);
        m.complete(p, SyntaxKind::ExprStmt);
        return;
    }

    // Expression or assignment
    let m = p.start();
    expressions::expr(p);

    if p.at(SyntaxKind::Colon) || p.at(SyntaxKind::ColonColon) {
        p.bump();
        expressions::expr(p);
        m.complete(p, SyntaxKind::AssignStmt);
    } else {
        m.complete(p, SyntaxKind::ExprStmt);
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p q-parser`
Expected: All tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/parser/
git commit -m "feat(parser): implement qSQL grammar (select, update, exec, delete)"
```

---

## Task 7: LSP Server - Basic Lifecycle

**Files:**
- Modify: `crates/server/src/main.rs`
- Create: `crates/server/src/backend.rs`
- Create: `crates/server/src/document.rs`

- [ ] **Step 1: Implement main.rs with tower-lsp setup**

```rust
// crates/server/src/main.rs
mod backend;
mod document;
mod diagnostics;
mod completion;
mod hover;
mod goto_def;
mod symbols;

use backend::QLanguageServer;
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(QLanguageServer::new);
    Server::new(stdin, stdout, socket).serve().await;
}
```

- [ ] **Step 2: Implement the backend (LanguageServer trait)**

```rust
// crates/server/src/backend.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::document::Document;

pub struct QLanguageServer {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
}

impl QLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn on_change(&self, uri: Url, doc: &Document) {
        let diagnostics = crate::diagnostics::compute_diagnostics(doc);
        self.client
            .publish_diagnostics(uri, diagnostics, Some(doc.version()))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for QLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".into(), "`".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "q-ls".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "q-ls initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let doc = Document::new(
            params.text_document.text,
            params.text_document.version,
        );
        self.on_change(uri.clone(), &doc).await;
        self.documents.write().await.insert(uri, doc);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let mut docs = self.documents.write().await;
        if let Some(doc) = docs.get_mut(&uri) {
            doc.apply_changes(params.content_changes, params.text_document.version);
            self.on_change(uri, doc).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.write().await.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let items = crate::completion::complete(doc, pos);
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        Ok(crate::hover::hover(doc, pos))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.clone();
        let pos = params.text_document_position_params.position;
        let docs = self.documents.read().await;
        let doc = match docs.get(&uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        Ok(crate::goto_def::goto_definition(doc, pos, &uri))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let docs = self.documents.read().await;
        let doc = match docs.get(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let symbols = crate::symbols::document_symbols(doc);
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }
}
```

- [ ] **Step 3: Implement the Document struct**

```rust
// crates/server/src/document.rs
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
        let mut line = 0;
        let mut col = 0;
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
```

- [ ] **Step 4: Create stub modules for features (compile check)**

```rust
// crates/server/src/diagnostics.rs
use q_parser::ParseError;
use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn compute_diagnostics(doc: &Document) -> Vec<Diagnostic> {
    doc.parse()
        .errors
        .iter()
        .map(|err| {
            let start = doc.position_of(err.offset);
            let end = doc.position_of(err.offset + err.len);
            Diagnostic {
                range: Range::new(start, end),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("q-ls".into()),
                message: err.message.clone(),
                ..Default::default()
            }
        })
        .collect()
}
```

```rust
// crates/server/src/completion.rs
use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn complete(_doc: &Document, _pos: Position) -> Vec<CompletionItem> {
    Vec::new() // Implemented in Task 9
}
```

```rust
// crates/server/src/hover.rs
use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn hover(_doc: &Document, _pos: Position) -> Option<Hover> {
    None // Implemented in Task 10
}
```

```rust
// crates/server/src/goto_def.rs
use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn goto_definition(_doc: &Document, _pos: Position, _uri: &Url) -> Option<GotoDefinitionResponse> {
    None // Implemented in Task 11
}
```

```rust
// crates/server/src/symbols.rs
use tower_lsp::lsp_types::*;
use crate::document::Document;

pub fn document_symbols(_doc: &Document) -> Vec<DocumentSymbol> {
    Vec::new() // Implemented in Task 12
}
```

- [ ] **Step 5: Build and verify**

Run: `cargo build -p q-ls`
Expected: Compiles successfully.

- [ ] **Step 6: Commit**

```bash
git add crates/server/
git commit -m "feat(server): implement LSP lifecycle with tower-lsp (init, sync, diagnostics)"
```

---

## Task 8: Completion Provider

**Files:**
- Modify: `crates/server/src/completion.rs`

- [ ] **Step 1: Implement keyword and builtin completion**

```rust
// crates/server/src/completion.rs
use tower_lsp::lsp_types::*;
use crate::document::Document;

pub const Q_BUILTINS: &[(&str, &str)] = &[
    ("abs", "Absolute value"),
    ("acos", "Arc cosine"),
    ("aj", "As-of join"),
    ("all", "All true"),
    ("and", "Logical AND / minimum"),
    ("any", "Any true"),
    ("asc", "Ascending sort"),
    ("asin", "Arc sine"),
    ("atan", "Arc tangent"),
    ("attr", "Attributes of a list"),
    ("avg", "Average"),
    ("avgs", "Running averages"),
    ("ceiling", "Round up"),
    ("cols", "Column names of a table"),
    ("cor", "Correlation"),
    ("cos", "Cosine"),
    ("count", "Count elements"),
    ("cov", "Covariance"),
    ("cross", "Cross product"),
    ("csv", "CSV separator"),
    ("cut", "Cut a list into sublists"),
    ("deltas", "Differences between adjacent elements"),
    ("desc", "Descending sort"),
    ("dev", "Standard deviation"),
    ("differ", "Differ from previous"),
    ("distinct", "Distinct/unique elements"),
    ("div", "Integer division"),
    ("each", "Apply to each element"),
    ("ej", "Equi-join"),
    ("enlist", "Make a one-element list"),
    ("eval", "Evaluate a parse tree"),
    ("except", "Set difference"),
    ("exit", "Exit process"),
    ("exp", "Exponential"),
    ("fby", "Filter by"),
    ("fills", "Forward fill nulls"),
    ("first", "First element"),
    ("fkeys", "Foreign keys"),
    ("flip", "Transpose"),
    ("floor", "Round down"),
    ("get", "Read/get a variable"),
    ("getenv", "Get environment variable"),
    ("group", "Group indices by value"),
    ("gtime", "Greenwich time"),
    ("hclose", "Close file handle"),
    ("hcount", "File size"),
    ("hdel", "Delete file"),
    ("hopen", "Open file handle"),
    ("hsym", "File symbol"),
    ("iasc", "Indices for ascending sort"),
    ("idesc", "Indices for descending sort"),
    ("ij", "Inner join"),
    ("in", "Membership"),
    ("insert", "Insert into table"),
    ("inter", "Set intersection"),
    ("inv", "Matrix inverse"),
    ("key", "Keys of a dictionary/table"),
    ("keys", "Key columns"),
    ("last", "Last element"),
    ("like", "Pattern match"),
    ("lj", "Left join"),
    ("load", "Load script/data"),
    ("log", "Natural logarithm"),
    ("lower", "Lowercase"),
    ("lsq", "Least squares"),
    ("ltime", "Local time"),
    ("ltrim", "Left trim"),
    ("mavg", "Moving average"),
    ("max", "Maximum"),
    ("maxs", "Running maximums"),
    ("mcount", "Moving count"),
    ("md5", "MD5 hash"),
    ("mdev", "Moving deviation"),
    ("med", "Median"),
    ("meta", "Table metadata"),
    ("min", "Minimum"),
    ("mins", "Running minimums"),
    ("mmax", "Moving maximum"),
    ("mmin", "Moving minimum"),
    ("mmu", "Matrix multiply"),
    ("mod", "Modulo"),
    ("msum", "Moving sum"),
    ("neg", "Negate"),
    ("next", "Next element"),
    ("not", "Logical NOT"),
    ("null", "Is null"),
    ("or", "Logical OR / maximum"),
    ("over", "Reduce / over"),
    ("parse", "Parse string to tree"),
    ("peach", "Parallel each"),
    ("pj", "Plus join"),
    ("prd", "Product"),
    ("prds", "Running products"),
    ("prev", "Previous element"),
    ("prior", "Apply with prior"),
    ("rand", "Random number"),
    ("rank", "Rank"),
    ("ratios", "Ratios"),
    ("raze", "Flatten nested list"),
    ("read0", "Read lines from file"),
    ("read1", "Read bytes from file"),
    ("reciprocal", "Reciprocal"),
    ("reval", "Restricted eval"),
    ("reverse", "Reverse"),
    ("rotate", "Rotate list"),
    ("rtrim", "Right trim"),
    ("save", "Save to file"),
    ("scan", "Scan / accumulate"),
    ("scov", "Sample covariance"),
    ("sdev", "Sample std deviation"),
    ("set", "Set a variable"),
    ("setenv", "Set environment variable"),
    ("show", "Display value"),
    ("signum", "Sign"),
    ("sin", "Sine"),
    ("sqrt", "Square root"),
    ("ssr", "String search replace"),
    ("ss", "String search"),
    ("string", "Convert to string"),
    ("sublist", "Sublist"),
    ("sum", "Sum"),
    ("sums", "Running sums"),
    ("sv", "Scalar from vector"),
    ("svar", "Sample variance"),
    ("system", "System command"),
    ("tables", "List tables"),
    ("tan", "Tangent"),
    ("til", "Range 0..n-1"),
    ("trim", "Trim whitespace"),
    ("type", "Type of value"),
    ("uj", "Union join"),
    ("ungroup", "Ungroup"),
    ("union", "Set union"),
    ("upper", "Uppercase"),
    ("upsert", "Upsert into table"),
    ("value", "Value of expression"),
    ("var", "Variance"),
    ("view", "View definition"),
    ("views", "List views"),
    ("vs", "Vector from scalar"),
    ("wavg", "Weighted average"),
    ("where", "Where clause / indices"),
    ("within", "Within range"),
    ("wj", "Window join"),
    ("wsum", "Weighted sum"),
    ("xasc", "Sort ascending by column"),
    ("xbar", "Round down to multiple"),
    ("xcol", "Rename columns"),
    ("xcols", "Reorder columns"),
    ("xdesc", "Sort descending by column"),
    ("xexp", "Power/exponent"),
    ("xgroup", "Group by"),
    ("xkey", "Set key columns"),
    ("xlog", "Logarithm base x"),
    ("xprev", "Previous by n"),
    ("xrank", "Bucket rank"),
];

const Q_KEYWORDS: &[&str] = &[
    "select", "exec", "update", "delete", "from", "where", "by",
    "if", "do", "while",
];

pub fn complete(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let offset = doc.offset_of(pos);
    let prefix = get_prefix(doc.text(), offset);

    let mut items = Vec::new();

    for &(name, detail) in Q_BUILTINS {
        if name.starts_with(&prefix) {
            items.push(CompletionItem {
                label: name.into(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail.into()),
                ..Default::default()
            });
        }
    }

    for &kw in Q_KEYWORDS {
        if kw.starts_with(&prefix) {
            items.push(CompletionItem {
                label: kw.into(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    // Document identifiers
    collect_identifiers(doc, &prefix, &mut items);

    items
}

fn get_prefix(text: &str, offset: usize) -> String {
    let before = &text[..offset.min(text.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

fn collect_identifiers(doc: &Document, prefix: &str, items: &mut Vec<CompletionItem>) {
    let root = doc.parse().syntax();
    let mut seen = std::collections::HashSet::new();

    for node in root.descendants_with_tokens() {
        if let Some(token) = node.as_token() {
            let kind: q_parser::SyntaxKind = q_parser::QLang::kind_from_raw(token.kind());
            if kind == q_parser::SyntaxKind::Ident || kind == q_parser::SyntaxKind::DottedIdent {
                let text = token.text().to_string();
                if text.starts_with(prefix) && !seen.contains(&text) {
                    seen.insert(text.clone());
                    items.push(CompletionItem {
                        label: text,
                        kind: Some(CompletionItemKind::VARIABLE),
                        ..Default::default()
                    });
                }
            }
        }
    }
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build -p q-ls`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/completion.rs
git commit -m "feat(completion): keyword, builtin, and identifier completion"
```

---

## Task 9: Hover Provider

**Files:**
- Modify: `crates/server/src/hover.rs`

- [ ] **Step 1: Implement hover with builtin and operator docs**

```rust
// crates/server/src/hover.rs
use tower_lsp::lsp_types::*;
use crate::completion::Q_BUILTINS;
use crate::document::Document;

pub fn hover(doc: &Document, pos: Position) -> Option<Hover> {
    let offset = doc.offset_of(pos);
    let word = get_word_at(doc.text(), offset)?;

    // Check builtins
    for &(name, detail) in Q_BUILTINS {
        if name == word {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**{}** - {}", name, detail),
                }),
                range: None,
            });
        }
    }

    // Check operators
    if let Some(doc_str) = operator_doc(&word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc_str.to_string(),
            }),
            range: None,
        });
    }

    None
}

fn get_word_at(text: &str, offset: usize) -> Option<String> {
    if offset >= text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut start = offset;
    let mut end = offset;

    while start > 0 && is_word_char(bytes[start - 1]) {
        start -= 1;
    }
    while end < bytes.len() && is_word_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        end = (offset + 1).min(bytes.len());
        start = offset;
    }

    Some(text[start..end].to_string())
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

fn operator_doc(op: &str) -> Option<&'static str> {
    match op {
        "+" => Some("**`+`** - Add (dyadic) / Flip (monadic)"),
        "-" => Some("**`-`** - Subtract (dyadic) / Negate (monadic)"),
        "*" => Some("**`*`** - Multiply (dyadic) / First (monadic)"),
        "%" => Some("**`%`** - Divide (dyadic) / Reciprocal (monadic)"),
        "!" => Some("**`!`** - Key/mod (dyadic) / Til/enum (monadic)"),
        "&" => Some("**`&`** - And/min (dyadic) / Where (monadic)"),
        "|" => Some("**`|`** - Or/max (dyadic) / Reverse (monadic)"),
        "^" => Some("**`^`** - Fill (dyadic) / Null? (monadic)"),
        "#" => Some("**`#`** - Take (dyadic) / Count (monadic)"),
        "_" => Some("**`_`** - Drop/cut (dyadic) / Floor (monadic)"),
        "~" => Some("**`~`** - Match (dyadic) / Not (monadic)"),
        "$" => Some("**`$`** - Cast/pad (dyadic) / String (monadic)"),
        "?" => Some("**`?`** - Find/rand (dyadic) / Distinct/type (monadic)"),
        "@" => Some("**`@`** - Apply/index (dyadic) / Type (monadic)"),
        "." => Some("**`.`** - Apply/index deep (dyadic) / Value (monadic)"),
        "," => Some("**`,`** - Join (dyadic) / Enlist (monadic)"),
        "=" => Some("**`=`** - Equal (dyadic) / Group (monadic)"),
        "<" => Some("**`<`** - Less than (dyadic) / Iasc (monadic)"),
        ">" => Some("**`>`** - Greater than (dyadic) / Idesc (monadic)"),
        ":" => Some("**`:`** - Assign (dyadic) / Identity/return (monadic)"),
        _ => None,
    }
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build -p q-ls`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/hover.rs
git commit -m "feat(hover): builtin function and operator documentation on hover"
```

---

## Task 10: Go-to-Definition

**Files:**
- Modify: `crates/server/src/goto_def.rs`

- [ ] **Step 1: Implement definition lookup by scanning assignments**

```rust
// crates/server/src/goto_def.rs
use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode, SyntaxElement, QLang};
use rowan::Language;
use crate::document::Document;

pub fn goto_definition(doc: &Document, pos: Position, uri: &Url) -> Option<GotoDefinitionResponse> {
    let offset = doc.offset_of(pos);
    let target_name = get_identifier_at(doc.text(), offset)?;

    let root = doc.parse().syntax();
    let def_offset = find_definition(&root, &target_name)?;
    let def_pos = doc.position_of(def_offset);

    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: Range::new(def_pos, def_pos),
    }))
}

fn find_definition(root: &SyntaxNode, name: &str) -> Option<usize> {
    for node in root.descendants() {
        let kind: SyntaxKind = QLang::kind_from_raw(node.kind());
        if kind == SyntaxKind::AssignStmt {
            if let Some(first_child) = node.first_child_or_token() {
                match first_child {
                    SyntaxElement::Node(n) => {
                        if let Some(token) = n.first_token() {
                            if token.text() == name {
                                return Some(token.text_range().start().into());
                            }
                        }
                    }
                    SyntaxElement::Token(t) => {
                        if t.text() == name {
                            return Some(t.text_range().start().into());
                        }
                    }
                }
            }
        }
    }
    None
}

fn get_identifier_at(text: &str, offset: usize) -> Option<String> {
    if offset >= text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let mut start = offset;
    let mut end = offset;

    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return None;
    }
    Some(text[start..end].to_string())
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build -p q-ls`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/goto_def.rs
git commit -m "feat(goto-def): navigate to variable definitions within document"
```

---

## Task 11: Document Symbols

**Files:**
- Modify: `crates/server/src/symbols.rs`

- [ ] **Step 1: Implement document symbol extraction**

```rust
// crates/server/src/symbols.rs
use tower_lsp::lsp_types::*;
use q_parser::{SyntaxKind, SyntaxNode, SyntaxElement, QLang};
use rowan::Language;
use crate::document::Document;

pub fn document_symbols(doc: &Document) -> Vec<DocumentSymbol> {
    let root = doc.parse().syntax();
    let mut symbols = Vec::new();

    for node in root.children() {
        let kind: SyntaxKind = QLang::kind_from_raw(node.kind());
        if kind == SyntaxKind::AssignStmt {
            if let Some(sym) = extract_symbol(doc, &node) {
                symbols.push(sym);
            }
        }
    }

    symbols
}

fn extract_symbol(doc: &Document, node: &SyntaxNode) -> Option<DocumentSymbol> {
    let first = node.first_child_or_token()?;
    let name = match first {
        SyntaxElement::Node(n) => n.first_token()?.text().to_string(),
        SyntaxElement::Token(t) => t.text().to_string(),
    };

    let range = node.text_range();
    let start = doc.position_of(range.start().into());
    let end = doc.position_of(range.end().into());
    let full_range = Range::new(start, end);

    let kind = if has_lambda(node) {
        SymbolKind::FUNCTION
    } else {
        SymbolKind::VARIABLE
    };

    #[allow(deprecated)]
    Some(DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range: full_range,
        selection_range: full_range,
        children: None,
    })
}

fn has_lambda(node: &SyntaxNode) -> bool {
    for child in node.descendants() {
        let kind: SyntaxKind = QLang::kind_from_raw(child.kind());
        if kind == SyntaxKind::Lambda {
            return true;
        }
    }
    false
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build -p q-ls`
Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/symbols.rs
git commit -m "feat(symbols): document symbol extraction for assignments and functions"
```

---

## Task 12: VS Code Extension

**Files:**
- Create: `editors/vscode/package.json`
- Create: `editors/vscode/src/extension.ts`
- Create: `editors/vscode/tsconfig.json`
- Create: `editors/vscode/language-configuration.json`

- [ ] **Step 1: Create package.json**

```json
{
  "name": "q-ls",
  "displayName": "q/kdb+ Language Support",
  "description": "Language server for q/kdb+ 4.1",
  "version": "0.1.0",
  "publisher": "q-ls",
  "engines": {
    "vscode": "^1.75.0"
  },
  "categories": ["Programming Languages"],
  "activationEvents": [
    "onLanguage:q"
  ],
  "main": "./out/extension.js",
  "contributes": {
    "languages": [
      {
        "id": "q",
        "aliases": ["q", "kdb+"],
        "extensions": [".q", ".k"],
        "configuration": "./language-configuration.json"
      }
    ]
  },
  "scripts": {
    "build": "tsc -p .",
    "watch": "tsc -watch -p ."
  },
  "dependencies": {
    "vscode-languageclient": "^9.0.1"
  },
  "devDependencies": {
    "@types/vscode": "^1.75.0",
    "typescript": "^5.0.0"
  }
}
```

- [ ] **Step 2: Create extension.ts**

```typescript
// editors/vscode/src/extension.ts
import * as path from "path";
import { ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  const serverPath = context.asAbsolutePath(
    path.join("..", "..", "target", "release", "q-ls")
  );

  const serverOptions: ServerOptions = {
    run: { command: serverPath },
    debug: { command: serverPath },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "q" }],
  };

  client = new LanguageClient("q-ls", "q Language Server", serverOptions, clientOptions);
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
```

- [ ] **Step 3: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "module": "commonjs",
    "target": "ES2020",
    "outDir": "out",
    "lib": ["ES2020"],
    "sourceMap": true,
    "rootDir": "src",
    "strict": true
  },
  "include": ["src"],
  "exclude": ["node_modules"]
}
```

- [ ] **Step 4: Create language-configuration.json**

```json
{
  "comments": {
    "lineComment": "/"
  },
  "brackets": [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"]
  ],
  "autoClosingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"" }
  ],
  "surroundingPairs": [
    { "open": "{", "close": "}" },
    { "open": "[", "close": "]" },
    { "open": "(", "close": ")" },
    { "open": "\"", "close": "\"" }
  ]
}
```

- [ ] **Step 5: Commit**

```bash
git add editors/
git commit -m "feat(vscode): add VS Code extension for q-ls"
```

---

## Task 13: Integration Tests and Test Fixtures

**Files:**
- Create: `test_data/basic.q`
- Create: `test_data/qsql.q`
- Create: `test_data/errors.q`
- Create: `crates/parser/tests/integration.rs`

- [ ] **Step 1: Create test fixtures**

`test_data/basic.q`:
```q
/ Basic q expressions
x:42
y:3.14
name:`hugo
add:{[x;y] x+y}
square:{x*x}
primes:2 3 5 7 11
mixed:(1;`two;3.0)
trade:([] sym:`AAPL`GOOG`MSFT; price:150.0 2800.0 300.0; size:100 200 50)
d:`a`b`c!1 2 3
result:$[x>0;x;neg x]
```

`test_data/qsql.q`:
```q
/ qSQL queries
select from trade
select sym,price from trade
select price,size from trade where sym=`AAPL
select avg price by sym from trade
update price:price*1.1 from trade where sym=`AAPL
exec price from trade where sym=`GOOG
delete from trade where price<100
```

`test_data/errors.q`:
```q
/ Intentional parse errors
f:{[x] x+
a:til[10
valid:42
```

- [ ] **Step 2: Write integration test**

```rust
// crates/parser/tests/integration.rs
use q_parser::parse;

#[test]
fn parse_basic_fixture_lossless() {
    let source = include_str!("../../../test_data/basic.q");
    let result = parse(source);
    assert_eq!(result.syntax().text().to_string(), source);
}

#[test]
fn parse_qsql_fixture_lossless() {
    let source = include_str!("../../../test_data/qsql.q");
    let result = parse(source);
    assert_eq!(result.syntax().text().to_string(), source);
}

#[test]
fn parse_errors_no_panic() {
    let source = include_str!("../../../test_data/errors.q");
    let result = parse(source);
    assert!(!result.errors.is_empty());
    assert_eq!(result.syntax().text().to_string(), source);
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test --test integration -p q-parser`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add test_data/ crates/parser/tests/
git commit -m "test: add integration tests with q fixture files"
```

---

## Task 14: Release Build and README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Verify release build**

Run: `cargo build --release && cargo test --all`
Expected: Compiles and all tests pass.

- [ ] **Step 2: Create README.md**

```markdown
# q-ls

A high-performance language server for q/kdb+ 4.1.

## Features

- **Diagnostics** - Real-time syntax error reporting
- **Completion** - Built-in functions, keywords, and document identifiers
- **Hover** - Documentation for operators and built-in functions
- **Go to Definition** - Navigate to variable assignments
- **Document Symbols** - Outline view of assignments and functions

## Architecture

- **Lexer** - Fast tokenization via `logos`
- **Parser** - Lossless CST via `rowan` (inspired by rust-analyzer)
- **Server** - LSP protocol via `tower-lsp`

## Building

    cargo build --release

The binary is at `target/release/q-ls`.

## VS Code Extension

    cd editors/vscode
    npm install
    npm run build

## Usage

The language server communicates over stdio. Configure your editor's LSP client to launch the `q-ls` binary.

## q/kdb+ 4.1 Coverage

- All q data types (temporal, numeric, symbol, string)
- Lambda expressions with parameters
- qSQL (select, exec, update, delete)
- Adverbs (each, over, scan, prior, each-right, each-left)
- System commands
- Namespaces
- Control flow ($[...])
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add README with feature overview and build instructions"
```

---

## Summary

| Task | Component | What it delivers |
|------|-----------|-----------------|
| 1 | Setup | Rust workspace, crate structure, git |
| 2 | Lexer | Full q tokenizer with logos |
| 3 | Parser | SyntaxKind + rowan types |
| 4 | Parser | Event-based parser infra + sink |
| 5 | Parser | Expression grammar (right-to-left, lambdas, lists) |
| 6 | Parser | qSQL (select, update, exec, delete) |
| 7 | Server | LSP lifecycle, document sync, diagnostics |
| 8 | Server | Completion (builtins + identifiers) |
| 9 | Server | Hover (operator + builtin docs) |
| 10 | Server | Go-to-definition |
| 11 | Server | Document symbols |
| 12 | Extension | VS Code extension |
| 13 | Tests | Integration tests + fixtures |
| 14 | Release | Build verification + docs |
