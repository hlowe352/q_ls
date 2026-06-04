# Code Style

- All Rust code must be formatted with `cargo fmt`. Run before committing.
- All Rust code must be clean under `cargo clippy --workspace --all-targets -- -W clippy::pedantic`. Fix warnings rather than suppressing them unless there is a documented reason.

# Git Workflow

- Never use `git worktree add`. Always use `git checkout -b` for new branches.
- `main` is branch-protected — all changes go through pull requests.

# Running Tests

```sh
cargo test --workspace                                        # all tests
cargo run -p q-parser --example print_tree -- file.q         # inspect parse tree
```

# Architecture Quick Reference

- 3 crates: `q-lexer` (logos), `q-parser` (rowan CST), `q-ls` (tower-lsp server)
- Parser emits `Event` stream → `Sink` converts to rowan `GreenNode`
- All text offsets are `u32` internally (`TextSize` / `rowan`); widen to `usize` only at LSP boundaries

# Parser Gotchas

**Trivia** — rowan attaches whitespace/comments as leading trivia on the *next* non-trivia token. `node.first_token()` may return a whitespace token. Always use a `find(|t| !t.kind().is_trivia())` scan when extracting names or offsets from nodes.

**Right-to-left evaluation** — q has no operator precedence; all operators are right-associative. The Pratt parser uses `(l_bp=1, r_bp=0)` throughout. Do not add precedence tiers.

**Contextual qSQL keywords** — `select`, `exec`, `update`, `delete`, `from`, `by`, `where` are plain `Ident` tokens. Detection is always `p.current_text() == Some("keyword")`. There are no reserved-word token variants.

**Parser state flags** — `qsql_stop` and `qsql_comma_stop` on `Parser` control juxtaposition and comma parsing in qSQL clause context. Both flags must be saved and restored whenever entering a bracketed sub-expression (`parse_paren`, `parse_lambda`, `parse_arg_list`, `parse_progn`) so they do not leak into nested expressions.
