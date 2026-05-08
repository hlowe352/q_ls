# Tree-sitter-q Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `q_ls` lexer + parser to full feature-coverage parity with the reference tree-sitter-q grammar at `~/repos/tree-sitter-q/grammar.js`, so that any q file accepted by tree-sitter-q is parsed losslessly into a CST with equivalent structure.

**Architecture:** Three-layer change — (1) lexer gains new token kinds for the full set of temporal types, byte lists, multi-line comment blocks, DSL prefix lines and file symbols; (2) parser gains new `SyntaxKind` composite nodes (`InfixProjection`, `Composition`, `InfixModExpr`, `LimitClause`, `OrderClause`, `TableKeys`, `Namespace`, `FileSymbolExpr`, `ReturnExpr`, `SignalExpr`, `DslStmt`, `CommentBlock`) plus grammar rules to produce them; (3) a builtin-keyword table makes contextual identifiers like `mmu`, `fby`, `each`, `xasc` recognisable as binary verbs.

**Tech Stack:** Rust 2024, `logos` 0.15 (lexer), `rowan` 0.16 (lossless CST), event-based Pratt parser. No new dependencies. Reference grammar: `~/repos/tree-sitter-q/grammar.js`. Reference corpus: `~/repos/tree-sitter-q/test/corpus/*.txt`.

**Workflow:** All work commits directly to `main` — repo is unreleased and has no remote. Do **not** create a feature branch and do **not** push.

**Out of scope:** AST-shape symmetry beyond what is documented below (we do not need byte-for-byte node-name parity, only behavioural parity); semantic analysis (type-checking, name resolution); editor integration changes beyond what compiles after the parser shape changes.

**Verification harness:** Each phase ends with `cargo test -p q_lexer` / `cargo test -p q_parser`. The final phase adds a corpus-driven integration test that loads each `~/repos/tree-sitter-q/test/corpus/*.txt` example, parses it with `q_parser`, and asserts no `SyntaxKind::Error` token is produced.

---

## File Structure

**Files modified:**
- `crates/lexer/src/token.rs` — add new token variants
- `crates/lexer/src/lib.rs` — re-export, no-op if already public
- `crates/parser/src/syntax_kind.rs` — mirror new tokens, add new composite kinds
- `crates/parser/src/grammar/expressions.rs` — add infix-projection, infix-mod, composition, return/signal, namespace, file-symbol parsing
- `crates/parser/src/grammar/qsql.rs` — add limit/order clauses
- `crates/parser/src/grammar/mod.rs` — add DSL line + comment-block statement entry points
- `crates/server/src/lib.rs` (and any module the server uses for symbol/goto-def) — handle new node kinds (extend match arms only)

**Files created:**
- `crates/parser/src/grammar/keywords.rs` — builtin infix keyword table + `is_builtin_infix(text: &str) -> bool`
- `crates/parser/tests/corpus.rs` — corpus-driven integration test against tree-sitter-q examples
- `crates/parser/tests/data/corpus/*.q` — extracted inputs from tree-sitter-q corpus
- `scripts/extract_corpus.sh` — extractor used to populate the corpus directory

---

## Phase 0 — Baseline

### Task 0: Confirm green baseline

**Files:** none

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: all 187+ tests pass. Record the count — every later phase must keep this number monotonically increasing (no regressions).

- [ ] **Step 2: Confirm working tree clean and on main**

Run: `git status && git rev-parse --abbrev-ref HEAD`
Expected: branch `main`, clean working tree (or only this plan file untracked). Do not switch branches.

---

## Phase 1 — Lexer: temporal type splits

q has eight distinct temporal types. The current lexer collapses six of them into `Integer`. This phase gives each its own token. Logos picks the longest match; with explicit suffixes (`m u v p n z g`) and explicit shapes (`HH:MM`, `HH:MM:SS`, `DDDDD`, `…D…`) the regexes do not collide with `Integer`/`Float` provided the new ones have higher priority.

Reference: tree-sitter-q `temporal` rule, lines 465-514 of `grammar.js`.

### Task 1: Month token

**Files:**
- Modify: `crates/lexer/src/token.rs`

- [ ] **Step 1: Write the failing test**

Append to the `tests` module in `crates/lexer/src/token.rs`:

```rust
#[test]
fn lex_month_literal() {
    let mut lex = Token::lexer("2024.01m");
    assert_eq!(lex.next(), Some(Ok(Token::Month)));
    assert_eq!(lex.slice(), "2024.01m");
}

#[test]
fn lex_month_null_typed() {
    assert_eq!(Token::lexer("0Nm").next(), Some(Ok(Token::Month)));
}

#[test]
fn lex_month_inf_typed() {
    assert_eq!(Token::lexer("0Wm").next(), Some(Ok(Token::Month)));
}
```

- [ ] **Step 2: Run test — verify it fails**

Run: `cargo test -p q_lexer lex_month_literal`
Expected: FAIL — `Token::Month` does not exist.

- [ ] **Step 3: Add the variant**

In `crates/lexer/src/token.rs`, after the `Date` variant, add:

```rust
/// Month literal: `2024.01m`, `0Nm`, `0Wm`
#[regex(r"0[NW]m", priority = 6)]
#[regex(r"[0-9]{4}\.[0-9]{2}m", priority = 6)]
Month,
```

Also remove the now-shadowed `m` cases from the `Integer` regexes: change `0N[ijhgmnpuvz]` → `0N[ijh]` and `0W[ijhgmnpuvz]` → `0W[ijh]`.

The pre-existing test `lex_month_null` currently asserts `Token::Integer`. Update it to assert `Token::Month`.

- [ ] **Step 4: Run tests — verify pass**

Run: `cargo test -p q_lexer`
Expected: PASS, including the three new month tests and the updated `lex_month_null`.

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): split month literal from integer"
```

### Task 2: Guid, Timespan, Datetime tokens

**Files:**
- Modify: `crates/lexer/src/token.rs`

- [ ] **Step 1: Write the failing tests**

Append to the `tests` module:

```rust
#[test]
fn lex_guid_literal_typed() {
    assert_eq!(Token::lexer("0Ng").next(), Some(Ok(Token::Guid)));
}

#[test]
fn lex_timespan_literal() {
    assert_eq!(Token::lexer("0D00:00:00.000000000").next(), Some(Ok(Token::Timespan)));
    assert_eq!(Token::lexer("0Nn").next(), Some(Ok(Token::Timespan)));
    assert_eq!(Token::lexer("0Wn").next(), Some(Ok(Token::Timespan)));
}

#[test]
fn lex_datetime_literal_typed() {
    assert_eq!(Token::lexer("0Nz").next(), Some(Ok(Token::Datetime)));
    assert_eq!(Token::lexer("0Wz").next(), Some(Ok(Token::Datetime)));
}
```

Find existing tests `lex_guid_null`, `lex_timespan_null` and update their expected variants to `Token::Guid` / `Token::Timespan`. Update the `lex_temporal_infs` test similarly — `0Wg`, `0Wm`, `0Wn`, `0Wp`, `0Wu`, `0Wv`, `0Wz` should map to `Guid`/`Month`/`Timespan`/`Timestamp`-or-existing/`Minute`/`Second`/`Datetime` (some of these are added in later tasks; for now drop the still-unsplit suffixes from the test and re-add them in Tasks 3–5).

- [ ] **Step 2: Run — verify fail**

Run: `cargo test -p q_lexer lex_guid_literal_typed lex_timespan_literal lex_datetime_literal_typed`
Expected: FAIL.

- [ ] **Step 3: Add variants**

After `Month`, in `token.rs`:

```rust
/// Guid literal: `0Ng`, `0Wg`
#[regex(r"0[NW]g", priority = 6)]
Guid,

/// Timespan literal: `0D00:00:00.000000000`, `0Nn`, `0Wn`
#[regex(r"0[NW]n", priority = 6)]
#[regex(r"-?[0-9]+D[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?", priority = 6)]
Timespan,

/// Datetime literal: `0Nz`, `0Wz`
#[regex(r"0[NW]z", priority = 6)]
Datetime,
```

Trim `Integer` typed-null/inf regexes further: `0N[ijh]` and `0W[ijh]`.

- [ ] **Step 4: Run — verify pass**

Run: `cargo test -p q_lexer`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): add guid, timespan, datetime tokens"
```

### Task 3: Minute and Second tokens

**Files:**
- Modify: `crates/lexer/src/token.rs`

The existing `Time` regex `[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?` already matches second/time. We need `Minute` (`HH:MM`) as a distinct shorter form, and `Second` (`HH:MM:SS` *without* fractional part). Tree-sitter-q distinguishes these by suffix-less form length.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn lex_minute_literal() {
    assert_eq!(Token::lexer("12:30").next(), Some(Ok(Token::Minute)));
    assert_eq!(Token::lexer("0Nu").next(), Some(Ok(Token::Minute)));
    assert_eq!(Token::lexer("0Wu").next(), Some(Ok(Token::Minute)));
}

#[test]
fn lex_second_literal() {
    assert_eq!(Token::lexer("12:30:45").next(), Some(Ok(Token::Second)));
    assert_eq!(Token::lexer("0Nv").next(), Some(Ok(Token::Second)));
    assert_eq!(Token::lexer("0Wv").next(), Some(Ok(Token::Second)));
}

#[test]
fn lex_time_keeps_fractional() {
    assert_eq!(Token::lexer("12:30:45.678").next(), Some(Ok(Token::Time)));
}
```

- [ ] **Step 2: Run — fail**

Run: `cargo test -p q_lexer lex_minute_literal lex_second_literal lex_time_keeps_fractional`

- [ ] **Step 3: Add variants and tighten `Time`**

After `Datetime`:

```rust
/// Minute literal: `12:30`, `0Nu`, `0Wu`
#[regex(r"0[NW]u", priority = 6)]
#[regex(r"[0-9]{2}:[0-9]{2}", priority = 6)]
Minute,

/// Second literal: `12:30:45`, `0Nv`, `0Wv`
#[regex(r"0[NW]v", priority = 6)]
#[regex(r"[0-9]{2}:[0-9]{2}:[0-9]{2}", priority = 6)]
Second,
```

Update the `Time` regex to require the fractional part: change `[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?` to `[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+`. Keep the `0Nt` typed-null entry.

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_lexer`
Expected: all pass. The pre-existing `lex_time` test (input `12:30:00.000`) still expects `Time` — it has a fractional part, still matches. The bare `12:30:00` form will now lex to `Second`.

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): split minute/second from time"
```

---

## Phase 2 — Lexer: byte list, comment block, DSL prefix, file symbol

### Task 4: ByteList token

q distinguishes single-byte hex literal `0xAB` (one byte) from `0xABCD…` (a list of bytes). Tree-sitter-q emits these as `byte_list` for any 4+ hex chars. Currently we collapse both into `Integer`.

**Files:**
- Modify: `crates/lexer/src/token.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn lex_byte_list() {
    assert_eq!(Token::lexer("0xABCD").next(), Some(Ok(Token::ByteList)));
    assert_eq!(Token::lexer("0x0011223344").next(), Some(Ok(Token::ByteList)));
}

#[test]
fn lex_single_byte_stays_integer() {
    assert_eq!(Token::lexer("0xAB").next(), Some(Ok(Token::Integer)));
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Add variant + tighten Integer hex**

```rust
/// Byte list literal: `0xABCD`, `0x0011...` (4+ hex chars)
#[regex(r"0x[0-9A-Fa-f]{4,}", priority = 6)]
ByteList,
```

Change the existing Integer hex regex from `0x[0-9A-Fa-f]+` to `0x[0-9A-Fa-f]{1,3}` so `0xAB` and `0xA` still match Integer but `0xABCD` falls through to `ByteList` (priority 6 > 4).

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_lexer`

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): byte_list token for 4+ hex digits"
```

### Task 5: CommentBlock token

q multi-line comment: a line starting with `/` (in column 0) terminates with a line starting with `\`. Logos can match across newlines if we encode the rule precisely.

**Files:**
- Modify: `crates/lexer/src/token.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn lex_comment_block_closed() {
    let src = "/\nthis is\nblock comment\n\\\n";
    let mut lex = Token::lexer(src);
    assert_eq!(lex.next(), Some(Ok(Token::CommentBlock)));
}

#[test]
fn lex_comment_block_terminal() {
    let src = "/\nuntil end of file";
    let mut lex = Token::lexer(src);
    assert_eq!(lex.next(), Some(Ok(Token::CommentBlock)));
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Add variant**

After `LineComment` add:

```rust
/// Multi-line comment block. Opens with a line containing only `/` and
/// closes with a line containing only `\` (or EOF). Greedy match.
#[regex(r"/[ \t]*\r?\n([^\n]*\n)*\\[ \t]*", priority = 7)]
#[regex(r"/[ \t]*\r?\n([^\n]*\n?)*", priority = 6)]
CommentBlock,
```

The first regex matches a closed block (terminator `\` on its own line). The second is the lower-priority "terminal" form running to EOF.

If `lex_line_comment` regresses (because `/<newline>` mid-line now matches `CommentBlock`), bump existing `LineComment` regex priorities to `priority = 8`.

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_lexer`

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): multi-line comment block token"
```

### Task 6: DSL prefix tokens (`k)…`, `p)…`)

q allows embedding k or PL/SQL DSL via a line prefix `k)expr` or `p)expr`. Tree-sitter-q lexes the entire line as one token aliased to `dsl`.

**Files:**
- Modify: `crates/lexer/src/token.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn lex_dsl_k_line() {
    let mut lex = Token::lexer("k)1+2");
    assert_eq!(lex.next(), Some(Ok(Token::DslLine)));
    assert_eq!(lex.slice(), "k)1+2");
}

#[test]
fn lex_dsl_p_line() {
    let mut lex = Token::lexer("p)select * from t");
    assert_eq!(lex.next(), Some(Ok(Token::DslLine)));
}

#[test]
fn lex_dsl_stops_at_newline() {
    let mut lex = Token::lexer("k)foo\nbar");
    assert_eq!(lex.next(), Some(Ok(Token::DslLine)));
    assert_eq!(lex.slice(), "k)foo");
    assert_eq!(lex.next(), Some(Ok(Token::Newline)));
}

#[test]
fn lex_bare_k_is_ident() {
    assert_eq!(Token::lexer("k").next(), Some(Ok(Token::Ident)));
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Add variant**

```rust
/// DSL escape line: `k)expr` or `p)expr`. The entire line is opaque.
#[regex(r"[kp]\)[^\r\n]*", priority = 5)]
DslLine,
```

Place it before `Ident`.

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_lexer`

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): k)/p) DSL prefix line token"
```

### Task 7: FileSymbol distinguished from Symbol

The lexer already accepts `` `:foo `` via the file-handle regex but lumps it under `Token::Symbol`. Tree-sitter-q aliases it to `file_symbol`. Split it.

**Files:**
- Modify: `crates/lexer/src/token.rs`

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn lex_file_symbol_split() {
    assert_eq!(Token::lexer("`:data.csv").next(), Some(Ok(Token::FileSymbol)));
    assert_eq!(Token::lexer("`:/abs/path").next(), Some(Ok(Token::FileSymbol)));
    assert_eq!(Token::lexer("`:host:5001").next(), Some(Ok(Token::FileSymbol)));
    assert_eq!(Token::lexer("`hello").next(), Some(Ok(Token::Symbol)));
    assert_eq!(Token::lexer("`").next(), Some(Ok(Token::Symbol)));
}
```

Update the existing `lex_file_handle_symbol` test to expect `Token::FileSymbol`.

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Restructure Symbol**

Replace the `Symbol` block:

```rust
/// File handle symbol: `` `:path/to/file ``, `` `:host:port ``
#[regex(r"`:[^\s;)\]},]*", priority = 4)]
FileSymbol,

/// Symbol literal: `` `sym ``, `` `a.b ``, `` ` `` (null symbol)
#[regex(r"`[a-zA-Z_.][a-zA-Z0-9_.]*", priority = 3)]
#[token("`")]
Symbol,
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_lexer`

- [ ] **Step 5: Commit**

```bash
git add crates/lexer/src/token.rs
git commit -m "feat(lexer): split file_symbol from symbol"
```

---

## Phase 3 — SyntaxKind expansion

### Task 8: Mirror new tokens into SyntaxKind

**Files:**
- Modify: `crates/parser/src/syntax_kind.rs`

- [ ] **Step 1: Write the failing test**

In `crates/parser/src/syntax_kind.rs`, append a `tests` module if absent, otherwise add:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_new_tokens() {
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Month),       SyntaxKind::Month);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Guid),        SyntaxKind::Guid);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Timespan),    SyntaxKind::Timespan);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Minute),      SyntaxKind::Minute);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Second),      SyntaxKind::Second);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Datetime),    SyntaxKind::Datetime);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::ByteList),    SyntaxKind::ByteList);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::CommentBlock),SyntaxKind::CommentBlock);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::DslLine),     SyntaxKind::DslLine);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::FileSymbol),  SyntaxKind::FileSymbol);
    }

    #[test]
    fn comment_block_is_trivia() {
        assert!(SyntaxKind::CommentBlock.is_trivia());
    }
}
```

- [ ] **Step 2: Run — fail**

Run: `cargo test -p q_parser maps_new_tokens`

- [ ] **Step 3: Add variants and mappings**

In `SyntaxKind` enum, just before the operator block, insert:

```rust
Month,
Guid,
Timespan,
Minute,
Second,
Datetime,
ByteList,
CommentBlock,
DslLine,
FileSymbol,
```

In `from_token`, add:

```rust
q_lexer::Token::Month        => SyntaxKind::Month,
q_lexer::Token::Guid         => SyntaxKind::Guid,
q_lexer::Token::Timespan     => SyntaxKind::Timespan,
q_lexer::Token::Minute       => SyntaxKind::Minute,
q_lexer::Token::Second       => SyntaxKind::Second,
q_lexer::Token::Datetime     => SyntaxKind::Datetime,
q_lexer::Token::ByteList     => SyntaxKind::ByteList,
q_lexer::Token::CommentBlock => SyntaxKind::CommentBlock,
q_lexer::Token::DslLine      => SyntaxKind::DslLine,
q_lexer::Token::FileSymbol   => SyntaxKind::FileSymbol,
```

In `is_trivia`, append `SyntaxKind::CommentBlock`:

```rust
matches!(self,
    SyntaxKind::Whitespace
    | SyntaxKind::Newline
    | SyntaxKind::LineComment
    | SyntaxKind::Shebang
    | SyntaxKind::CommentBlock)
```

- [ ] **Step 4: Run — pass**

Run: `cargo test --workspace`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/syntax_kind.rs
git commit -m "feat(parser): mirror new lexer tokens into SyntaxKind"
```

### Task 9: Add new composite SyntaxKinds

**Files:**
- Modify: `crates/parser/src/syntax_kind.rs`

- [ ] **Step 1: Write the test**

```rust
#[test]
fn composite_kinds_distinct() {
    let kinds = [
        SyntaxKind::InfixProjection,
        SyntaxKind::Composition,
        SyntaxKind::InfixModExpr,
        SyntaxKind::LimitClause,
        SyntaxKind::OrderClause,
        SyntaxKind::TableKeys,
        SyntaxKind::Namespace,
        SyntaxKind::ReturnExpr,
        SyntaxKind::SignalExpr,
        SyntaxKind::DslStmt,
        SyntaxKind::FileSymbolExpr,
    ];
    let set: std::collections::HashSet<_> = kinds.iter().collect();
    assert_eq!(set.len(), kinds.len(), "duplicate composite kinds");
    for k in kinds {
        assert!((k as u16) < (SyntaxKind::__LAST as u16));
    }
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Add variants**

In `SyntaxKind`, before `__LAST`:

```rust
/// `1+` — binary operator with no RHS, treated as a projection.
InfixProjection,
/// `'[f;g]` — function composition (bracket form).
Composition,
/// `f' x`, `f/: y` — infix function with adverb modifier.
InfixModExpr,
/// `select[5]` / `select[5;>col]` — qSQL limit clause.
LimitClause,
/// `>col` / `<col` inside a limit clause.
OrderClause,
/// `[k:v;...]` — keyed-table key list, inside a `TableExpr`.
TableKeys,
/// `.q` / `.Q` / `.z` — bare namespace identifier (without trailing
/// member). Distinguished from `DottedIdent` (which is namespace + member).
Namespace,
/// `:expr` at expression position — function return.
ReturnExpr,
/// `'expr` at expression position — signal/throw.
SignalExpr,
/// `k)…` or `p)…` — DSL escape statement.
DslStmt,
/// `` `:path `` literal expression node (wraps the FileSymbol token).
FileSymbolExpr,
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser composite_kinds_distinct`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/syntax_kind.rs
git commit -m "feat(parser): add composite syntax kinds for tree-sitter parity"
```

---

## Phase 4 — Parser: literals and trivia

### Task 10: Recognise new literal tokens as atoms

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Write the failing test**

Locate the existing parser test harness — likely a function `parse(src: &str) -> SyntaxNode` (or similar) reachable from any module's `tests` block. Search:

```bash
rg "fn parse\(" crates/parser/src
```

Reuse that helper. If none exists, add one in a new `crates/parser/src/test_util.rs` exporting:

```rust
#[cfg(test)]
pub fn parse(src: &str) -> crate::syntax_kind::SyntaxNode {
    crate::lib_or_root_entry(src).syntax_node()  // adjust to actual API
}
```

Append to `crates/parser/src/grammar/expressions.rs` `tests` module:

```rust
#[test]
fn parse_temporal_literals() {
    use crate::test_util::parse;
    for (src, kind_name) in [
        ("0Nm",       "Month"),
        ("0Ng",       "Guid"),
        ("0Nn",       "Timespan"),
        ("12:30",     "Minute"),
        ("12:30:45",  "Second"),
        ("0Nz",       "Datetime"),
        ("0xABCD",    "ByteList"),
        ("`:foo.csv", "FileSymbol"),
    ] {
        let cst = parse(src);
        let dump = format!("{:#?}", cst);
        assert!(dump.contains(kind_name), "expected {kind_name} in:\n{dump}");
    }
}
```

- [ ] **Step 2: Run — fail**

Run: `cargo test -p q_parser parse_temporal_literals`
Expected: FAIL — the parser's `atom` does not recognise the new token kinds.

- [ ] **Step 3: Extend atom-literal arm**

In `crates/parser/src/grammar/expressions.rs`, the literal arm of `atom`:

```rust
SyntaxKind::Integer
| SyntaxKind::Float
| SyntaxKind::Boolean
| SyntaxKind::String
| SyntaxKind::Symbol
| SyntaxKind::Date
| SyntaxKind::Time
| SyntaxKind::Timestamp
| SyntaxKind::Month
| SyntaxKind::Guid
| SyntaxKind::Timespan
| SyntaxKind::Minute
| SyntaxKind::Second
| SyntaxKind::Datetime
| SyntaxKind::ByteList => {
    let m = p.start();
    p.bump();
    Some(m.complete(p, SyntaxKind::LiteralExpr))
}

SyntaxKind::FileSymbol => {
    let m = p.start();
    p.bump();
    Some(m.complete(p, SyntaxKind::FileSymbolExpr))
}
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src
git commit -m "feat(parser): atom recognises new literal token kinds"
```

---

## Phase 5 — Parser: infix projection and infix-mod

### Task 11: Mark trailing-operator as InfixProjection

Currently `1+` is parsed as `BinExpr(LiteralExpr(1), +, <missing>)`. Tree-sitter-q labels it `infix_projection`. We change the completion kind based on whether the RHS was actually parsed.

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`
- Modify: `crates/parser/src/parser.rs` (add `events_len` accessor)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn parse_infix_projection() {
    use crate::test_util::parse;
    let cst = parse("1+");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("InfixProjection"), "got:\n{dump}");
    assert!(!dump.contains("BinExpr"), "should not be BinExpr:\n{dump}");
}

#[test]
fn parse_full_binary_still_binexpr() {
    use crate::test_util::parse;
    let cst = parse("1+2");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("BinExpr"), "got:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Branch the completion kind**

Add to `crates/parser/src/parser.rs` (place inside the `impl Parser` block):

```rust
pub(crate) fn events_len(&self) -> usize {
    self.events.len()
}
```

(If the field is named differently, e.g. `events_buf`, mirror the actual name.)

In `expr_bp` (inside `expressions.rs`), the binary-operator branch — replace:

```rust
let m = lhs.precede(p);
p.bump(); // consume operator
if !at_expr_boundary(p) {
    expr_bp(p, r_bp);
}
lhs = m.complete(p, SyntaxKind::BinExpr);
```

with:

```rust
let m = lhs.precede(p);
p.bump(); // consume operator
let had_rhs = if !at_expr_boundary(p) {
    let before = p.events_len();
    expr_bp(p, r_bp);
    p.events_len() > before
} else {
    false
};
let kind = if had_rhs { SyntaxKind::BinExpr } else { SyntaxKind::InfixProjection };
lhs = m.complete(p, kind);
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`. If any pre-existing test asserts `BinExpr` for trailing-operator inputs (e.g. a parser snapshot test), update it to `InfixProjection`.

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src
git commit -m "feat(parser): emit InfixProjection for trailing infix op"
```

### Task 12: InfixModExpr for adverb-decorated infix

Tree-sitter-q distinguishes `a f' b` (each), `a f/: b` (each-right), etc., from a plain `BinExpr`. The decorator wraps the function position. We implement it as: when the binary-operator slot is followed *immediately* by an adverb token, produce `InfixModExpr` instead of `BinExpr`.

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_infix_mod_each() {
    use crate::test_util::parse;
    let cst = parse("a +' b");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("InfixModExpr"), "got:\n{dump}");
}

#[test]
fn parse_infix_mod_each_right() {
    use crate::test_util::parse;
    let cst = parse("a +/: b");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("InfixModExpr"), "got:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

Replace the binary-operator branch from Task 11 with:

```rust
let m = lhs.precede(p);
p.bump(); // operator
let mut kind = SyntaxKind::BinExpr;
if is_adverb(p) {
    p.bump();   // adverb modifier
    kind = SyntaxKind::InfixModExpr;
}
let had_rhs = if !at_expr_boundary(p) {
    let before = p.events_len();
    expr_bp(p, r_bp);
    p.events_len() > before
} else {
    false
};
let final_kind = match (kind, had_rhs) {
    (SyntaxKind::BinExpr, false) => SyntaxKind::InfixProjection,
    (k, _) => k,
};
lhs = m.complete(p, final_kind);
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/expressions.rs
git commit -m "feat(parser): InfixModExpr for adverb-decorated infix"
```

### Task 13: Composition `'[f;g]` and SignalExpr

Tree-sitter-q has an explicit `Composition` node for `'[f;g]` and a separate `signal` node for `'expr`. We special-case both. Juxtaposed `f g` (implicit composition) is already covered by `ApplyExpr` and we leave it.

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_composition_bracket_form() {
    use crate::test_util::parse;
    let cst = parse("'[f;g]");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("Composition"), "got:\n{dump}");
}

#[test]
fn parse_signal_expr() {
    use crate::test_util::parse;
    let cst = parse("'`err");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("SignalExpr"), "got:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

In `atom`, the `Each` arm currently falls under the unary-operator block. Pull `Each` out and replace:

```rust
SyntaxKind::Each => {
    let m = p.start();
    p.bump(); // '
    if p.at(SyntaxKind::LBracket) {
        // '[f;g] form — composition
        parse_arg_list(p);
        Some(m.complete(p, SyntaxKind::Composition))
    } else if !is_adverb(p) && !at_expr_boundary(p) {
        expr_bp(p, 100);
        Some(m.complete(p, SyntaxKind::SignalExpr))
    } else {
        Some(m.complete(p, SyntaxKind::UnaryExpr))
    }
}
```

Remove `Each` from the broad unary-operator alternation list.

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/expressions.rs
git commit -m "feat(parser): Composition and SignalExpr nodes"
```

---

## Phase 6 — Parser: namespace, return

### Task 14: Namespace vs DottedIdent

Tree-sitter-q's `namespace` matches `\.[a-zA-Z][a-zA-Z0-9_]*` *without* a trailing member, while `variable` is `namespace . identifier`. Today `q_lexer` produces a single `DottedIdent` for the whole `.q.func` and a *bare* `.q` also lexes as `DottedIdent`. Split at the *parser* level: when a `DottedIdent`'s text contains exactly one segment after the leading dot, wrap it in `SyntaxKind::Namespace`; otherwise leave as `IdentExpr`.

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_bare_namespace() {
    use crate::test_util::parse;
    let cst = parse(".q");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("Namespace"), "got:\n{dump}");
}

#[test]
fn parse_dotted_member_remains_dotted() {
    use crate::test_util::parse;
    let cst = parse(".q.func");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("DottedIdent"), "got:\n{dump}");
    assert!(!dump.contains("Namespace"), "should not be Namespace:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

In `atom`, the identifier arm:

```rust
SyntaxKind::Ident | SyntaxKind::DottedIdent => {
    if kind == SyntaxKind::Ident && p.nth(1) == Some(SyntaxKind::LBracket)
        && let Some(text) = p.current_text()
    {
        let ctrl_kind = match text.as_str() {
            "if" => Some(SyntaxKind::IfExpr),
            "do" => Some(SyntaxKind::DoExpr),
            "while" => Some(SyntaxKind::WhileExpr),
            _ => None,
        };
        if let Some(sk) = ctrl_kind {
            return parse_control_word(p, sk);
        }
    }

    // Bare-namespace detection: DottedIdent with exactly one '.'.
    let bare_ns = kind == SyntaxKind::DottedIdent
        && p.current_text()
            .map(|t| t.matches('.').count() == 1)
            .unwrap_or(false);

    let m = p.start();
    p.bump();
    let node_kind = if bare_ns { SyntaxKind::Namespace } else { SyntaxKind::IdentExpr };
    Some(m.complete(p, node_kind))
}
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/expressions.rs
git commit -m "feat(parser): bare namespace recognised as Namespace node"
```

### Task 15: Return expression `:expr`

Today `:expr` becomes `UnaryExpr`. Tree-sitter-q labels it `return`. Re-tag.

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_return_expr() {
    use crate::test_util::parse;
    let cst = parse("{:42}");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("ReturnExpr"), "got:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

Replace the `SyntaxKind::Colon` arm in `atom`:

```rust
SyntaxKind::Colon => {
    let m = p.start();
    p.bump();
    let kind = if !at_expr_boundary(p) && p.current() != Some(SyntaxKind::LBracket) {
        expr_bp(p, 100);
        SyntaxKind::ReturnExpr
    } else {
        SyntaxKind::UnaryExpr
    };
    Some(m.complete(p, kind))
}
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`. Note: a top-level `:42` will also become `ReturnExpr` — this matches tree-sitter-q's behaviour (also positionless).

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/expressions.rs
git commit -m "feat(parser): tag :expr as ReturnExpr"
```

---

## Phase 7 — Parser: keyed table form

### Task 16: TableKeys node

In `parse_paren`, when we see `([key:val;…] …)`, the bracketed key list currently parses as `ArgList` (because `parse_arg_list` is called). Wrap it in a `TableKeys` node instead.

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_keyed_table() {
    use crate::test_util::parse;
    let cst = parse("([k:1 2] v:3 4)");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("TableExpr"), "got:\n{dump}");
    assert!(dump.contains("TableKeys"), "got:\n{dump}");
}

#[test]
fn parse_unkeyed_table_no_table_keys() {
    use crate::test_util::parse;
    let cst = parse("([] v:3 4)");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("TableExpr"), "got:\n{dump}");
    assert!(!dump.contains("TableKeys"), "should not have TableKeys:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

In `parse_paren`, the keyed-table branch:

```rust
if p.at(SyntaxKind::LBracket) {
    if p.nth(1) == Some(SyntaxKind::RBracket) {
        p.bump(); // [
        p.bump(); // ]
    } else {
        let km = p.start();
        parse_arg_list(p);
        km.complete(p, SyntaxKind::TableKeys);
    }
    while !p.at(SyntaxKind::RParen) && !p.at_end() {
        parse_list_entry(p);
        if !p.eat(SyntaxKind::Semi) {
            break;
        }
    }
    p.expect(SyntaxKind::RParen);
    return Some(m.complete(p, SyntaxKind::TableExpr));
}
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/expressions.rs
git commit -m "feat(parser): TableKeys node for keyed table form"
```

---

## Phase 8 — Parser: qSQL limit and order

### Task 17: LimitClause and OrderClause

`select[5;>price] col from t` — the bracketed expression after `select` is currently parsed as `ArgList`. Wrap it as `LimitClause`, and any `>` / `<` prefix inside as `OrderClause`.

**Files:**
- Modify: `crates/parser/src/grammar/qsql.rs`

- [ ] **Step 1: Failing test**

Add to `qsql.rs` (or wherever qSQL tests live):

```rust
#[cfg(test)]
mod tests {
    use crate::test_util::parse;

    #[test]
    fn parse_select_limit() {
        let cst = parse("select[5] col from t");
        let dump = format!("{:#?}", cst);
        assert!(dump.contains("LimitClause"), "got:\n{dump}");
    }

    #[test]
    fn parse_select_limit_with_order() {
        let cst = parse("select[5;>price] col from t");
        let dump = format!("{:#?}", cst);
        assert!(dump.contains("LimitClause"), "got:\n{dump}");
        assert!(dump.contains("OrderClause"), "got:\n{dump}");
    }

    #[test]
    fn parse_select_order_only() {
        let cst = parse("select[>price] col from t");
        let dump = format!("{:#?}", cst);
        assert!(dump.contains("OrderClause"), "got:\n{dump}");
    }
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

Replace the `if p.at(SyntaxKind::LBracket)` block in `parse_select`:

```rust
if p.at(SyntaxKind::LBracket) {
    let lm = p.start();
    p.bump(); // [
    if !p.at(SyntaxKind::RBracket) && !p.at(SyntaxKind::Semi) {
        if p.at(SyntaxKind::Lt) || p.at(SyntaxKind::Gt) {
            parse_order(p);
        } else {
            expressions::expr(p);
        }
    }
    if p.eat(SyntaxKind::Semi) {
        if p.at(SyntaxKind::Lt) || p.at(SyntaxKind::Gt) {
            parse_order(p);
        }
    }
    p.expect(SyntaxKind::RBracket);
    lm.complete(p, SyntaxKind::LimitClause);
}
```

Add helper at the bottom of `qsql.rs`:

```rust
fn parse_order(p: &mut Parser) {
    let om = p.start();
    p.bump(); // > or <
    expressions::expr(p);
    om.complete(p, SyntaxKind::OrderClause);
}
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/qsql.rs
git commit -m "feat(parser): qSQL LimitClause and OrderClause"
```

---

## Phase 9 — Parser: DSL line and comment block as statements

### Task 18: DslStmt

**Files:**
- Modify: `crates/parser/src/grammar/mod.rs`

- [ ] **Step 1: Failing test**

```rust
#[cfg(test)]
mod tests {
    use crate::test_util::parse;

    #[test]
    fn parse_dsl_k_stmt() {
        let cst = parse("k)1+2");
        let dump = format!("{:#?}", cst);
        assert!(dump.contains("DslStmt"), "got:\n{dump}");
    }

    #[test]
    fn parse_dsl_p_stmt() {
        let cst = parse("p)select * from t");
        let dump = format!("{:#?}", cst);
        assert!(dump.contains("DslStmt"), "got:\n{dump}");
    }
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

In `crates/parser/src/grammar/mod.rs` `statement`, add a branch immediately after the bare-newline / semi skip and before the system-command branch:

```rust
if p.at(SyntaxKind::DslLine) {
    let m = p.start();
    p.bump();
    m.complete(p, SyntaxKind::DslStmt);
    return;
}
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/mod.rs
git commit -m "feat(parser): DslStmt for k)/p) lines"
```

### Task 19: Confirm comment-block trivia handling end-to-end

`CommentBlock` is already trivia-classified in Task 8. Verify the parser's trivia-skipping path covers it.

**Files:**
- (potentially) `crates/parser/src/parser.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_with_comment_block_between_stmts() {
    use crate::test_util::parse;
    let src = "x:1\n/\nblock\nstuff\n\\\ny:2\n";
    let cst = parse(src);
    let dump = format!("{:#?}", cst);
    let count = dump.matches("ExprStmt").count();
    assert!(count >= 2, "expected 2 stmts, got:\n{dump}");
}
```

- [ ] **Step 2: Run**

If green: skip step 3 and commit empty. If red: locate the trivia-skip path in `parser.rs` (e.g. `fn skip_trivia` or inside `current()`), confirm it consults `SyntaxKind::is_trivia(kind)`. If it instead matches a hand-rolled list (`Whitespace | Newline | LineComment | Shebang`), append `| CommentBlock`.

- [ ] **Step 3: Implement (only if needed)**

```rust
fn is_trivia_kind(kind: SyntaxKind) -> bool {
    kind.is_trivia()
}
```

(Refactor any open-coded set to use this.)

- [ ] **Step 4: Run — pass**

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src
git commit -m "test(parser): confirm comment_block trivia handling"
```

(Use `--allow-empty` if no source change.)

---

## Phase 10 — Builtin keyword recognition

### Task 20: Keyword table

**Files:**
- Create: `crates/parser/src/grammar/keywords.rs`
- Modify: `crates/parser/src/grammar/mod.rs` (re-export module)

- [ ] **Step 1: Failing test**

Create the file with only the test stub:

```rust
//! Builtin keyword recognition for q.

#[cfg(test)]
mod tests {
    use super::is_builtin_infix;

    #[test]
    fn recognises_core_keywords() {
        for kw in ["mmu", "lsq", "in", "within", "each", "peach", "div", "mod",
                   "wavg", "wsum", "cor", "cov", "scov", "cross", "union",
                   "inter", "except", "sublist", "vs", "sv", "ss", "like",
                   "mavg", "mmax", "mmin", "msum", "mdev", "mcount", "ema",
                   "ij", "ijf", "uj", "ujf", "lj", "ljf", "asof", "pj",
                   "insert", "upsert", "xasc", "xdesc", "xcol", "xcols", "xkey",
                   "xprev", "xrank", "xbar", "xexp", "xlog", "dsave", "fby",
                   "bin", "binr", "and", "or", "setenv"] {
            assert!(is_builtin_infix(kw), "{kw} should be recognised");
        }
    }

    #[test]
    fn rejects_non_keywords() {
        for s in ["foo", "bar", "select", "from", "where", "by", "x", ""] {
            assert!(!is_builtin_infix(s), "{s} should not be recognised");
        }
    }
}
```

- [ ] **Step 2: Run — fail**

Run: `cargo test -p q_parser recognises_core_keywords` — compile error (`is_builtin_infix` undefined).

- [ ] **Step 3: Implement**

Prepend to `keywords.rs`:

```rust
//! Builtin keyword recognition for q.
//!
//! Lifted from tree-sitter-q `builtin_infix_func` (grammar.js:285-314).
//! Keep alphabetised for greppability and binary-search correctness.

const BUILTIN_INFIX: &[&str] = &[
    "and", "asof", "bin", "binr", "cor", "cov", "cross",
    "div", "dsave", "each", "ema", "except", "fby",
    "ij", "ijf", "in", "insert", "inter",
    "lj", "ljf", "like", "lsq",
    "mavg", "mcount", "mdev", "mmax", "mmin", "mmu", "mod", "msum",
    "or", "peach", "pj",
    "scov", "setenv", "ss", "sublist", "sv",
    "uj", "ujf", "union", "upsert",
    "vs",
    "wavg", "within", "wsum",
    "xasc", "xbar", "xcol", "xcols", "xdesc", "xexp", "xkey",
    "xlog", "xprev", "xrank",
];

/// True if `text` names a builtin q verb that is conventionally used infix.
pub fn is_builtin_infix(text: &str) -> bool {
    BUILTIN_INFIX.binary_search(&text).is_ok()
}
```

In `crates/parser/src/grammar/mod.rs`:

```rust
pub mod expressions;
pub mod keywords;
pub mod qsql;
```

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser keywords`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/keywords.rs crates/parser/src/grammar/mod.rs
git commit -m "feat(parser): builtin keyword table"
```

### Task 21: Treat builtin keywords as binary operators

**Files:**
- Modify: `crates/parser/src/grammar/expressions.rs`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn parse_keyword_as_binary() {
    use crate::test_util::parse;
    for src in ["x mmu y", "a in b", "1 within 2 5", "t lj u"] {
        let cst = parse(src);
        let dump = format!("{:#?}", cst);
        assert!(dump.contains("BinExpr"), "{src} → no BinExpr:\n{dump}");
    }
}

#[test]
fn parse_keyword_as_atom_when_alone() {
    use crate::test_util::parse;
    let cst = parse("mmu");
    let dump = format!("{:#?}", cst);
    assert!(dump.contains("IdentExpr"), "got:\n{dump}");
}
```

- [ ] **Step 2: Run — fail**

- [ ] **Step 3: Implement**

Locate the binary-op detection in `expr_bp` (currently inline or in a helper `binary_op`). Refactor to:

```rust
fn binary_op(p: &Parser) -> Option<()> {
    let kind = p.current()?;
    if matches_builtin_op_token(kind) {
        return Some(());
    }
    if kind == SyntaxKind::Ident
        && let Some(t) = p.current_text()
        && super::keywords::is_builtin_infix(&t)
    {
        return Some(());
    }
    None
}

fn matches_builtin_op_token(kind: SyntaxKind) -> bool {
    matches!(kind,
        SyntaxKind::Plus | SyntaxKind::Minus | SyntaxKind::Star | SyntaxKind::Percent
        | SyntaxKind::Bang | SyntaxKind::Amp | SyntaxKind::Pipe | SyntaxKind::Caret
        | SyntaxKind::Hash | SyntaxKind::Underscore | SyntaxKind::Tilde
        | SyntaxKind::Dollar | SyntaxKind::Query | SyntaxKind::At | SyntaxKind::Comma
        | SyntaxKind::Eq | SyntaxKind::Lt | SyntaxKind::Gt
        | SyntaxKind::NotEq | SyntaxKind::LtEq | SyntaxKind::GtEq
        | SyntaxKind::Dot | SyntaxKind::Colon | SyntaxKind::ColonColon
        | SyntaxKind::CompoundAssign
        | SyntaxKind::FileOp0 | SyntaxKind::FileOp1 | SyntaxKind::FileOp2)
}
```

(Match the actual list in the existing code; some entries above may not currently be included as binary ops — copy the existing predicate verbatim and just add the keyword arm.)

- [ ] **Step 4: Run — pass**

Run: `cargo test -p q_parser`

- [ ] **Step 5: Commit**

```bash
git add crates/parser/src/grammar/expressions.rs
git commit -m "feat(parser): builtin keywords parse as infix verbs"
```

---

## Phase 11 — Server integration

### Task 22: Update server match arms for new node kinds

The LSP server switches on `SyntaxKind` in goto-def, document symbols, and semantic tokens. Adding new variants without updating these matches will (a) cause exhaustiveness warnings or (b) silently skip the new kinds.

**Files:**
- Modify: `crates/server/src/lib.rs` and any sibling module that has `match kind` over `SyntaxKind`

- [ ] **Step 1: Compile and survey**

Run: `cargo check -p q_server 2>&1 | tee /tmp/q_server_check.log`
Then:

```bash
rg "SyntaxKind::" crates/server/src
```

For each match site, decide if a new kind needs explicit handling.

- [ ] **Step 2: Add minimal handling**

For literal-classification sites (semantic-token coloring), treat the new temporal/byte/file-symbol variants identically to existing literals. Concrete edit:

```rust
matches!(kind,
    SyntaxKind::Integer
        | SyntaxKind::Float
        | SyntaxKind::Boolean
        | SyntaxKind::String
        | SyntaxKind::Symbol
        | SyntaxKind::Date
        | SyntaxKind::Time
        | SyntaxKind::Timestamp
        | SyntaxKind::Month
        | SyntaxKind::Guid
        | SyntaxKind::Timespan
        | SyntaxKind::Minute
        | SyntaxKind::Second
        | SyntaxKind::Datetime
        | SyntaxKind::ByteList
        | SyntaxKind::FileSymbol)
```

For symbol-name sites (document symbols / goto-def), treat `Namespace` like `DottedIdent`.

For `DslStmt`, `LimitClause`, `OrderClause`, `TableKeys`, `Composition`, `InfixModExpr`, `InfixProjection`, `ReturnExpr`, `SignalExpr`, `FileSymbolExpr`, `CommentBlock` — generally pass-through; if there is a `match` with a wildcard that does the right thing, leave it.

- [ ] **Step 3: Run server tests**

Run: `cargo test -p q_server`
Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src
git commit -m "feat(server): handle new SyntaxKind variants"
```

---

## Phase 12 — Corpus integration test

### Task 23: Vendor selected corpus inputs

Tree-sitter-q's corpus lives at `~/repos/tree-sitter-q/test/corpus/*.txt`. Each file alternates `===\nname\n===\ninput\n---\nexpected sexp\n` blocks. We extract only the *input* sections and store them as `.q` files in `crates/parser/tests/data/corpus/`.

**Files:**
- Create: `scripts/extract_corpus.sh`
- Create: `crates/parser/tests/data/corpus/*.q`

- [ ] **Step 1: Inspect format and write extractor**

Open one corpus file and confirm the block delimiter shape:

```bash
head -40 ~/repos/tree-sitter-q/test/corpus/assignment.txt
```

Confirmed format:

```
=================
test name here
=================

input source...

---

(expected_tree)
```

Create `scripts/extract_corpus.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SRC=${1:-$HOME/repos/tree-sitter-q/test/corpus}
DST=crates/parser/tests/data/corpus
mkdir -p "$DST"
rm -f "$DST"/*.q

python3 - "$SRC" "$DST" <<'PY'
import os, re, sys, pathlib
src_dir, dst_dir = sys.argv[1], sys.argv[2]
header = re.compile(r"^=+$")
sep    = re.compile(r"^-+$")
for path in sorted(pathlib.Path(src_dir).glob("*.txt")):
    text = path.read_text()
    lines = text.splitlines()
    i, n = 0, len(lines)
    idx = 0
    while i < n:
        if header.match(lines[i]):
            j = i + 1
            while j < n and not header.match(lines[j]):
                j += 1
            name = "_".join(lines[i+1:j]).strip()
            name = re.sub(r"[^A-Za-z0-9_]", "_", name) or f"case{idx}"
            i = j + 1
            buf = []
            while i < n and not sep.match(lines[i]):
                buf.append(lines[i])
                i += 1
            slug = f"{path.stem}__{name}__{idx}.q"
            (pathlib.Path(dst_dir)/slug).write_text("\n".join(buf).rstrip() + "\n")
            idx += 1
            while i < n and not header.match(lines[i]):
                i += 1
        else:
            i += 1
PY

count=$(ls "$DST"/*.q 2>/dev/null | wc -l)
echo "extracted $count corpus inputs to $DST"
```

```bash
chmod +x scripts/extract_corpus.sh
```

- [ ] **Step 2: Run the extractor**

```bash
./scripts/extract_corpus.sh
ls crates/parser/tests/data/corpus | wc -l
```

Expected: dozens of `.q` files.

- [ ] **Step 3: Commit the vendored data**

```bash
git add crates/parser/tests/data/corpus scripts/extract_corpus.sh
git commit -m "test(parser): vendor tree-sitter-q corpus inputs"
```

### Task 24: Corpus parse-clean test

**Files:**
- Create: `crates/parser/tests/corpus.rs`

- [ ] **Step 1: Inspect public API**

```bash
rg "pub fn parse" crates/parser/src
```

Note the actual signature (likely `pub fn parse(src: &str) -> Parsed` returning a struct with a `syntax_node()` or `syntax()` method). Use this in the test.

- [ ] **Step 2: Write the test**

```rust
//! Corpus-driven smoke test: every example from tree-sitter-q's corpus
//! must parse without producing `SyntaxKind::Error` tokens.

use q_parser::{parse, SyntaxKind};

fn first_error(node: &q_parser::SyntaxNode) -> Option<String> {
    for elem in node.descendants_with_tokens() {
        if elem.kind() == SyntaxKind::Error {
            return Some(format!("error at {:?}: {:?}", elem.text_range(), elem));
        }
    }
    None
}

#[test]
fn parses_tree_sitter_q_corpus_clean() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/corpus");
    let mut failures = Vec::new();
    let mut total = 0;
    for entry in std::fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("q") {
            continue;
        }
        total += 1;
        let src = std::fs::read_to_string(&path).unwrap();
        let parsed = parse(&src);
        let root: q_parser::SyntaxNode = parsed.syntax();   // adjust to actual API
        if let Some(msg) = first_error(&root) {
            failures.push(format!("{}: {}", path.display(), msg));
        }
    }
    assert!(failures.is_empty(),
        "{}/{} corpus files failed:\n{}",
        failures.len(), total, failures.join("\n"));
}
```

(If `q_parser` does not yet re-export `SyntaxNode` and `SyntaxKind` from the crate root, add `pub use crate::syntax_kind::{SyntaxKind, SyntaxNode};` in `crates/parser/src/lib.rs`.)

- [ ] **Step 3: Run — likely fail**

Run: `cargo test -p q_parser parses_tree_sitter_q_corpus_clean`
Expected: a list of files still producing `Error` tokens. Each failure is a real coverage gap.

- [ ] **Step 4: Iterate**

For each failing corpus file, in priority order (smallest input first):
1. `cat crates/parser/tests/data/corpus/<file>.q`
2. Add a focused test in the relevant phase's tests module reproducing the smallest failing slice.
3. Fix the parser (or rarely, the lexer).
4. Re-run the corpus test.

Time-box at 4 hours. If a specific corpus example is genuinely beyond scope (e.g. obscure `\\` system command variants documented as "non-standard" in tree-sitter-q), move it to `crates/parser/tests/data/corpus_known_unsupported/` and document the gap with a one-line entry in `docs/parser-coverage.md`.

- [ ] **Step 5: Commit**

```bash
git add crates/parser
git commit -m "test(parser): corpus-driven parity test against tree-sitter-q"
```

---

## Phase 13 — Final verification

### Task 25: End-to-end check

**Files:** none (verification + memory update)

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace --release`
Expected: clean.

- [ ] **Step 2: Full test run**

Run: `cargo test --workspace --release`
Expected: all baseline tests + new tests pass; corpus test passes (or has documented exceptions).

- [ ] **Step 3: Manual editor smoke test**

Open `editors/vscode/` and launch the extension against this test buffer:

```q
.q.foo:{[x;y] x+y}
t:([] sym:`a`b`c; px:1.0 2.0 3.0)
select[5;>px] sym, px from t where sym in `a`b
2024.01m + 0Nm
0xABCDEF
/
multi
line
\
k)1+2
```

Confirm: no syntax errors reported by the server; semantic-token colours appear; goto-def on `.q.foo` jumps correctly; document symbols list `.q.foo` and `t`.

- [ ] **Step 4: Update memory**

Edit `~/.claude/projects/-Users-hugo-projects-q-ls/memory/MEMORY.md` (and `architecture.md` / `patterns.md` if they exist) to record:
- The new SyntaxKinds added (one paragraph).
- The corpus parity status (X/Y files pass) and link to `docs/parser-coverage.md` if non-empty.
- A note that q_ls is now ~at parity with tree-sitter-q grammar lines 1-707.

- [ ] **Step 5: Final commit**

```bash
git add docs/ ~/.claude/projects/-Users-hugo-projects-q-ls/memory/MEMORY.md
git commit -m "docs: tree-sitter-q parity status + coverage notes"
git log --oneline -25
```

Do **not** push — repo has no remote.

---

## Coverage matrix (self-review)

| Gap (from audit)                                | Task # |
|-------------------------------------------------|--------|
| Month / 0Nm / 0Wm                               | 1      |
| Guid / 0Ng                                      | 2      |
| Timespan / 0Nn                                  | 2      |
| Datetime / 0Nz                                  | 2      |
| Minute / Second                                 | 3      |
| ByteList                                        | 4      |
| Multi-line CommentBlock                         | 5, 19  |
| DSL prefix `k)` / `p)`                          | 6, 18  |
| FileSymbol token                                | 7, 10  |
| Mirror tokens into SyntaxKind                   | 8      |
| Composite SyntaxKinds                           | 9      |
| New literal atoms                               | 10     |
| InfixProjection                                 | 11     |
| InfixModExpr                                    | 12     |
| Composition + SignalExpr                        | 13     |
| Namespace                                       | 14     |
| ReturnExpr                                      | 15     |
| TableKeys                                       | 16     |
| LimitClause / OrderClause                       | 17     |
| DslStmt                                         | 18     |
| Builtin keyword table                           | 20     |
| Builtin keywords as infix verbs                 | 21     |
| Server match-arm exhaustiveness                 | 22     |
| Corpus parity                                   | 23, 24 |
| Final verify                                    | 25     |

Every audited gap maps to at least one task. Type names are consistent across tasks (`InfixModExpr`, `LimitClause`, `OrderClause`, `TableKeys`, `Namespace`, `ReturnExpr`, `SignalExpr`, `DslStmt`, `FileSymbolExpr`, `CommentBlock`, `Composition`, `InfixProjection`).
