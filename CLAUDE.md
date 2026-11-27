# Workflow

- `main` is branch-protected: every change goes through a pull request.
- Never use `git worktree add`; branch with `git checkout -b`.
- The toolchain is pinned in `rust-toolchain.toml` and CI installs the same one. Bump it deliberately and fix any new lints in that PR.
- Before committing, all of these must pass (CI runs them with `RUSTFLAGS=-D warnings`):

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic
cargo test --workspace --all-targets
cargo run -p q-parser --example print_tree -- file.q   # inspect a parse tree
```

- Fix clippy warnings rather than suppressing them; an `#[allow]` needs a comment saying why.

# Architecture

- `q-lexer` (logos) → `q-parser` (event-based parser, lossless rowan CST) → `q-ls` (`tower-lsp-server` binary).
- Parser: `Parser::new` lexes with line context (comments, the `\` terminator and `k)`/`p)` lines need it, so `lex` recognises them before logos sees the text), then merges negative literals; grammar rules emit `Event`s; `sink::build` turns them into a rowan `GreenNode`.
- Server: `WorkspaceIndex` is the only document store, holding open documents (editor text, never overwritten from disk while open) and every indexed `.q` file. Indexing walks the git root with the `ignore` crate, so `.gitignore`/`.ignore` apply; this repo's `.ignore` excludes the parser fixtures.
- `binder::Bindings` (built per parse) is the only place names get meaning: every name token's role (use, param, declare, amend) and symbol (a lambda local, or one qualified global). Features look up the cursor's name with `name_at` and read it; they never classify tokens themselves. The cursor's CST token is `Document::token_at`. Shared CST helpers live in `syntax.rs`.
- Offsets: rowan `TextSize`/`u32` in the CST and for stored definition sites; `usize` byte offsets in `Document`, `ParseError` and feature code; LSP `Position` (UTF-16) only via `Document::offset_of`/`position_of`.

# q semantics the code relies on

Check behaviour against a real q 4.1 (`q script.q -q`) before changing any of these. Intuition about q edge cases is often wrong.

- **Right to left, no precedence, no prefix verbs**: an expression is a run of terms (atom + `[args]`/adverbs). A term ending in an adverb, or a bare primitive (`+`, `in`), is a verb; anything else is a noun. noun verb → infix (`x f/ y`), noun noun → application, verb with an argument → error (`-x`, `+/x`; q needs `neg x`, `(+/)x`). Only `'x` and `:x` are prefix. An unbracketed argument or right operand ending in a verb (`g f+`, `x+y+`) makes a `Composition`, except as the value of a plain `:` assignment (`f:+/`). `f::+` composes like q's `parse` says: in a lambda it reads `f` and assigns nothing (only q's top-level evaluator assigns undotted names here). Do not add precedence.
- **No closures**: a lambda sees only its own params/locals and globals, never an enclosing lambda's names (`{[a] g:{a+1}; g[]}` reads the global `a`).
- **Locals are fixed at compile time**: a name is local to a lambda iff it is a parameter or the target of a plain `name:` anywhere in its body. `::`, `+:`, `+::` and `a[i]:` change that local if there is one, else the global (`{b,:1}` creates global `b`). Names containing a dot are always global. At top level under `\d .ns`, `name:` goes to `.ns.name` but `name::` goes to the root.
- **qSQL keywords are contextual**: `select exec update delete from by where` are plain `Ident` tokens, detected with `p.current_text() == Some("…")`. A name is a qSQL column iff its nearest enclosing qSQL construct is the statement rather than its `FromClause` (`syntax::in_row_context`), so `from f[x]` is ordinary code.
- **Script terminator**: a column-0 line that is only `\`, outside a `/` … `\` block comment, ends the file. An indented `\` or `\ foo` is code.
- **Symbols**: `` `a`b`c `` (no spaces) is one literal token; `` `a `b `` is application. Numeric and temporal literals separated by spaces form a vector (`1 2 3`).

# Parser gotchas

- **Trivia**: rowan attaches whitespace/comments as leading trivia of the *next* token, so `node.first_token()` may be whitespace. Use `syntax::first_token` (first non-trivia token) when extracting names or offsets.
- **qSQL flags**: `qsql_stop` / `qsql_comma_stop` change how juxtaposition and commas parse inside qSQL clauses. Only change them through `with_qsql_flags` (grammar/expressions.rs), which restores the old values; bracketed sub-parsers run with both cleared so they never leak into nested expressions.

# Tests

- Prefer tables that assert exact results:
  - lexer: `CASES` in `lexer/src/token.rs`;
  - parser: `CLEAN` in `parser/tests/corpus.rs`, `SNAPSHOTS`/`SHAPES` in `parser/tests/snapshots.rs`;
  - server: `CASES` in goto-def, references and hover, `UNRESOLVED` in diagnostics.
- Server tables mark positions inline via `workspace_index::tests::markers`: `«` is the cursor and each `»` an expected site, e.g. `"fn:99;\nf:{[»fn] «fn+1}"`. Other helpers: `workspace_index::tests::{uri, doc, index}`, `goto_def::tests::locations`.
- `crates/server/tests/lsp.rs` runs the real binary over stdio: document sync, indexing, file watching, config, root detection, and request routing. Cross-file and config behaviour is tested there, not repeated in unit tests.
- `crates/server/src/binding_oracle.rs` runs each binding case in a real q in six contexts (top level, lambda, `\d .ns`, a lambda in `.ns` called there and from the root, `system "d .ns"`) and checks the globals q changes equal the ones the binder records. Add a case before changing scoping rules.
- `crates/parser/tests/q_oracle.rs` compares every fixture statement's tree with a real q's `parse` (skipped without `$Q_BIN` / `~/.kx/bin/q`). Add q-checked expressions to `tests/data/q_oracle.q`; known gaps live in `q_oracle_known.txt`, and stale entries fail the test. `parse` proves structure only: check what a form does with `value` before describing it.
- Coverage: `rm -rf target/llvm-cov-target && cargo llvm-cov --workspace --summary-only`. Stale builds in that directory skew the results badly, so always clear it first.
