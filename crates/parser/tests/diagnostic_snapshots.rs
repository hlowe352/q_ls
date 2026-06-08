//! Snapshot tests for parser diagnostics — errors that produce `ParseError` entries
//! rather than silently mis-parsing.

use q_parser::parse;

/// Parse `src` and return the error messages, asserting at least one error.
fn errors(src: &str) -> Vec<String> {
    let p = parse(src);
    assert!(
        !p.errors.is_empty(),
        "expected parse errors for {src:?} but got none",
    );
    p.errors.iter().map(|e| e.message.clone()).collect()
}

// ---------------------------------------------------------------------------
// Regression: String followed by Integer must not trigger VectorExpr
// ---------------------------------------------------------------------------

#[test]
fn string_followed_by_integer_does_not_stack_overflow() {
    // `String` is in the same match arm as scalar literals but is NOT a scalar
    // literal kind — without the `is_scalar_literal_kind(kind)` guard on the
    // VectorExpr entry, `"a" 32` would cause infinite recursion.
    let p = parse(r#"f["a" 32 "b"]"#);
    // Must terminate (no stack overflow); errors are fine.
    let _ = p;
}

// ---------------------------------------------------------------------------
// SystemCmd must appear at line start
// ---------------------------------------------------------------------------

#[test]
fn system_cmd_mid_line_is_error() {
    // `\l file` after an expression on the same line is invalid in kdb+
    let msgs = errors("x \\l file.q");
    assert!(
        msgs.iter().any(|m| m.contains("\\l") || m.contains("scan adverb") || m.contains("start of a line")),
        "expected a diagnostic about misplaced \\l, got: {msgs:?}",
    );
}

#[test]
fn scan_adverb_not_system_cmd() {
    // `f\` by itself is a valid scan adverb — must NOT produce an error
    let p = parse("f\\");
    assert!(
        p.errors.is_empty(),
        "f\\ should parse as scan adverb without errors, got: {:?}",
        p.errors,
    );
}

#[test]
fn system_cmd_at_line_start_ok() {
    // `\l file.q` at start of line is valid
    let p = parse("\\l file.q");
    assert!(
        p.errors.is_empty(),
        "\\l at line start should parse without errors, got: {:?}",
        p.errors,
    );
}

#[test]
fn system_cmd_after_newline_ok() {
    // `x:1\n\l file.q` — system command on its own line is valid
    let p = parse("x:1\n\\l file.q");
    assert!(
        p.errors.is_empty(),
        "\\l after newline should parse without errors, got: {:?}",
        p.errors,
    );
}
