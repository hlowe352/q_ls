# q-ls

A high-performance language server for q/kdb+ 4.1.

## Features

- **Diagnostics** - Real-time syntax error reporting
- **Completion** - Built-in functions, keywords, and document identifiers
- **Hover** - Documentation for operators and built-in functions
- **Go to Definition** - Navigate to variable assignments
- **Document Symbols** - Outline view of assignments and functions

## Architecture

- **Lexer** (`crates/lexer`) - Fast tokenization via `logos`
- **Parser** (`crates/parser`) - Lossless CST via `rowan` (inspired by rust-analyzer)
- **Server** (`crates/server`) - LSP protocol via `tower-lsp`

All operators use q's right-to-left evaluation semantics. The parser produces a
lossless concrete syntax tree that preserves whitespace and comments, enabling
accurate source mapping for diagnostics and navigation.

## Building

```
cargo build --release
```

The binary is at `target/release/q-ls`.

## Running Tests

```
cargo test --all
```

## VS Code Extension

```
cd editors/vscode
npm install
npm run build
```

Then install the extension in VS Code and ensure `q-ls` is on your PATH or
configure the server path in the extension settings.

## Usage

The language server communicates over stdio. Configure your editor's LSP client
to launch the `q-ls` binary.

## q/kdb+ 4.1 Coverage

- All q data types (integer, float, boolean, string, symbol, date, time, timestamp)
- Lambda expressions with explicit and implicit parameters
- qSQL (select, exec, update, delete with by/from/where clauses)
- Adverbs (each, over, scan, prior, each-right, each-left)
- System commands
- Namespaces (dotted identifiers)
- Control flow ($[cond;true;false])
- Tables and dictionaries
