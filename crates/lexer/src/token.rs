use logos::Logos;

/// All tokens produced by the q/kdb+ 4.1 lexer.
///
/// Horizontal whitespace (spaces and tabs) is skipped automatically.
/// Newlines are significant in q (statement separators) and are emitted as
/// [`Token::Newline`].
// clippy >= 1.100 flags repeated `priority = N` arguments across separate
// `#[regex]` attributes as duplicates; they are distinct logos patterns.
#[allow(clippy::duplicated_attributes)]
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t]+")] // skip horizontal whitespace only
pub enum Token {
    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------
    /// Boolean literal: `0b`, `1b`, `010101b`
    #[regex(r"[01]+b")]
    Boolean,

    /// Integer literal (long/int/short/byte suffixes, hex, special nulls/infs)
    ///
    /// Covers: `42`, `42i`, `42j`, `42h`, byte atoms `0x2A`, `0Ni`, `0Wi`,
    ///         `0Nj`, `0Wj`, `0Nh`, `0Wh`
    ///
    /// Note: negative sign is NOT included here; the parser handles unary minus.
    /// Priority 3 keeps hex above the plain decimal regex (default priority 2).
    #[regex(r"0x[0-9A-Fa-f]{1,2}", priority = 4)] // byte atom (1-2 hex digits)
    #[regex(r"0N[ijh]", priority = 5)] // typed nulls (other types have their own variants)
    #[regex(r"0N", priority = 4)] // bare long null `0N` (longer typed patterns take priority)
    #[regex(r"0W[ijh]", priority = 5)] // typed infs
    #[regex(r"[0-9]+[ijh]?")] // plain decimal, optional suffix
    Integer,

    /// Float literal
    ///
    /// Covers: `3.14`, `3.14e10`, `3.14f`, `3.14e`, `0n`, `0w`, `0Nf`,
    ///         `0Wf`, `0Ne`, `0We`
    #[regex(r"0[nw]", priority = 4)] // generic float null/inf (lower-case)
    #[regex(r"0N[fe]", priority = 5)] // typed float nulls
    #[regex(r"0W[fe]", priority = 5)] // typed float infs
    #[regex(r"[0-9]+\.[0-9]*([eE][0-9]+)?[fe]?")] // decimal float
    #[regex(r"[0-9]+[eE][0-9]+[fe]?")] // scientific without dot
    #[regex(r"[0-9]+f")] // integer-form float: `3f`, `100f`
    #[regex(r"[0-9]+e")]
    // integer-form real: `3e`, `100e` (longest-match ensures `3e10` still hits scientific)
    Float,

    /// Timestamp literal: `2024.01.15D12:30:00.000000000`, `0Np`, `0Wp`
    #[regex(r"0[NW]p", priority = 6)]
    #[regex(r"[12][0-9]{3}\.(0[1-9]|1[0-2])\.(0[1-9]|[12][0-9]|3[01])D[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?")]
    Timestamp,

    /// Date literal: `2024.01.15`, `0Nd`
    #[regex(r"0Nd")]
    #[regex(r"[0-9]{4}\.(0[1-9]|1[0-2])\.(0[1-9]|[12][0-9]|3[01])")]
    Date,

    /// Month literal: `2024.01m`, `0Nm`, `0Wm`
    #[regex(r"0[NW]m", priority = 6)]
    #[regex(r"[0-9]{4}\.(0[1-9]|1[0-2])m", priority = 6)]
    Month,

    /// Guid literal: `0Ng`, `0Wg`
    #[regex(r"0[NW]g", priority = 6)]
    Guid,

    /// Timespan literal: `1D`, `0D00:00:00.000000000`, `0Nn`, `0Wn`
    #[regex(r"0[NW]n", priority = 6)]
    #[regex(r"[0-9]+D[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?", priority = 6)]
    #[regex(r"[0-9]+D", priority = 5)]
    Timespan,

    /// Datetime literal: `0Nz`, `0Wz`
    #[regex(r"0[NW]z", priority = 6)]
    Datetime,

    /// Minute literal: `12:30`, `0Nu`, `0Wu`
    #[regex(r"0[NW]u", priority = 6)]
    #[regex(r"[0-9]{2}:[0-9]{2}", priority = 6)]
    Minute,

    /// Second literal: `12:30:45`, `0Nv`, `0Wv`
    #[regex(r"0[NW]v", priority = 6)]
    #[regex(r"[0-9]{2}:[0-9]{2}:[0-9]{2}", priority = 6)]
    Second,

    /// Time literal: `12:30:45.678`, `0Nt` (requires fractional seconds)
    #[regex(r"0Nt")]
    #[regex(r"[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+")]
    Time,

    /// Byte list literal: `0x2A3`, `0xABCD` (3+ hex digits; 2 per byte)
    #[regex(r"0x[0-9A-Fa-f]{3,}", priority = 6)]
    ByteList,

    /// String literal: `"hello"`, with escape sequences
    #[regex(r#""([^"\\]|\\.)*""#)]
    String,

    /// File handle symbol: `` `:path/to/file ``, `` `:host:port ``
    #[regex(r"`:[^\s;)\]},]*", priority = 4)]
    FileSymbol,

    /// Symbol literal or symbol list: `` `sym ``, `` `1 ``, `` `a:b ``, `` ` ``
    /// (null), `` `a`b`c `` — adjacent backtick symbols are one literal in q.
    /// A lone `` `:path `` with `/` etc. is a [`Token::FileSymbol`].
    #[regex(r"(`[a-zA-Z0-9_.:]*)+", priority = 3)]
    Symbol,

    // -----------------------------------------------------------------------
    // Identifiers
    // -----------------------------------------------------------------------
    /// DSL escape line: `k)expr` or `p)expr`. Entire line is opaque.
    #[regex(r"[kp]\)[^\r\n]*", priority = 5)]
    DslLine,

    /// Dotted/namespaced identifier starting with dot: `.q.func`, `.Q.en`, `.z.ts`, `.d`
    #[regex(r"\.[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*")]
    DottedIdent,

    /// Regular identifier (may include dot-separated segments): `trade`, `myVar`, `assert.true`
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*")]
    Ident,

    // -----------------------------------------------------------------------
    // Operators / verbs
    // -----------------------------------------------------------------------
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
    Amp,
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
    Query,
    #[token("@")]
    At,
    #[token(",")]
    Comma,
    #[token("<>")]
    NotEq,
    #[token("<=")]
    LtEq,
    #[token(">=")]
    GtEq,
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token(".")]
    Dot,

    // -----------------------------------------------------------------------
    // Assignment / colon variants
    // -----------------------------------------------------------------------
    /// Compound assignment operators: `+:`, `-:`, `*:`, etc.
    /// Must come before individual operator + Colon.
    #[regex(r"[-+*%><~=_#$!|&?^,@]:", priority = 3)]
    CompoundAssign,

    /// File I/O operators `0:`, `1:`, `2:`
    #[token("0:", priority = 5)]
    FileOp0,
    #[token("1:", priority = 5)]
    FileOp1,
    #[token("2:", priority = 5)]
    FileOp2,

    /// Global assign `::` — must come before single `:`
    #[token("::")]
    ColonColon,

    /// Assignment / return `:` (also used in adverbs below)
    #[token(":")]
    Colon,

    // -----------------------------------------------------------------------
    // Adverbs (compound tokens — must be before their component tokens)
    // -----------------------------------------------------------------------
    /// Each-prior `':`
    #[token("':")]
    EachPrior,

    /// Each-right `/:`
    #[token("/:")]
    EachRight,

    /// Each-left `\:`
    #[token("\\:")]
    EachLeft,

    /// Each `'`
    #[token("'")]
    Each,

    // -----------------------------------------------------------------------
    // Delimiters
    // -----------------------------------------------------------------------
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
    #[token(";")]
    Semi,

    // -----------------------------------------------------------------------
    // Special / structural
    // -----------------------------------------------------------------------
    /// Newline — significant in q as a statement separator
    #[regex(r"\r?\n")]
    Newline,

    /// Line comment: `/ comment text` (slash followed by space or end-of-line)
    /// Also covers full-line comments that begin with `/` or `//`.
    /// The lexer emits the entire comment (including the leading `/`) as one token.
    #[regex(r"//[^\r\n]*", priority = 8)] // `//` style comment
    #[regex(r"/[^\S\r\n][^\r\n]*", priority = 8)] // `/ ` followed by rest of line
    #[regex(r"/\r?\n", priority = 8)] // bare `/` at end of line
    LineComment,

    /// Multi-line comment block. Opens with a line containing only `/` and
    /// closes with a line containing only `\` (or EOF). Greedy match.
    #[regex(r"/[ \t]*\r?\n(?:[^\n]*\r?\n)*\\[ \t]*(?:\r?\n)?", priority = 7)]
    #[regex(r"/[ \t]*\r?\n(?:[^\n]*\r?\n)*[^\n]*", priority = 6)]
    CommentBlock,

    /// Shebang: `#!/usr/bin/env q`
    #[regex(r"#![^\r\n]*")]
    Shebang,

    /// Exit command: `\\`
    #[token("\\\\")]
    Exit,

    /// System command: `\l file.q`, `\t expr`, `\p 5001`, etc.
    /// Must come after `\\` (Exit).
    #[regex(r"\\[a-zA-Z][^\r\n]*")]
    SystemCmd,

    /// Backslash — scan adverb (bare `\` not followed by a letter or `\`)
    #[token("\\")]
    Backslash,

    /// Forward slash — "over" adverb (bare `/` not matched by `LineComment`)
    #[token("/")]
    Slash,

    // -----------------------------------------------------------------------
    // Error (unrecognized input)
    // -----------------------------------------------------------------------
    /// Catch-all for unrecognized input (logos 0.13+ uses the unit variant
    /// without `#[error]`; the `Err` side of the iterator carries this).
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each input must lex to exactly the listed token kinds (space-separated).
    /// Single-token cases therefore also pin the token's span to the input.
    #[rustfmt::skip]
    const CASES: &[(&str, &str)] = &[
    ("42", "Integer"),
    ("42j", "Integer"),
    ("42i", "Integer"),
    ("42h", "Integer"),
    ("0x2A", "Integer"),
    ("0Ni", "Integer"),
    ("0Wi", "Integer"),
    ("0Nh", "Integer"),
    ("3.14", "Float"),
    ("3.14e10", "Float"),
    ("3.14f", "Float"),
    ("3f", "Float"),
    ("0f", "Float"),
    ("100f", "Float"),
    ("3e", "Float"),
    ("0e", "Float"),
    ("100e", "Float"),
    ("3e10", "Float"),
    ("0n", "Float"),
    ("0w", "Float"),
    ("0Nf", "Float"),
    ("0Wf", "Float"),
    ("010101b", "Boolean"),
    ("0b", "Boolean"),
    ("1b", "Boolean"),
    ("`hello", "Symbol"),
    ("`", "Symbol"),
    ("`a.b", "Symbol"),
    ("`a`b`c", "Symbol"),
    ("``a", "Symbol"),
    ("`a `b", "Symbol Symbol"),
    ("`1", "Symbol"),
    ("`a:b", "Symbol"),
    ("`a32`2`:ad", "Symbol"),
    ("trade", "Ident"),
    ("x1", "Ident"),
    (".q.func", "DottedIdent"),
    (".Q.en", "DottedIdent"),
    ("my_var", "Ident"),
    (".proc.my_func", "DottedIdent"),
    (".my_ns.func", "DottedIdent"),
    ("+ - * %", "Plus Minus Star Percent"),
    ("{[x;y] x+y}", "LBrace LBracket Ident Semi Ident RBracket Ident Plus Ident RBrace"),
    ("\"hello world\"", "String"),
    ("\"a\\nb\"", "String"),
    ("\\l file.q", "SystemCmd"),
    ("\\t expr", "SystemCmd"),
    ("::", "ColonColon"),
    (":", "Colon"),
    ("':", "EachPrior"),
    ("/:", "EachRight"),
    ("\\:", "EachLeft"),
    ("'", "Each"),
    ("a\nb", "Ident Newline Ident"),
    ("\\\\", "Exit"),
    ("/", "Slash"),
    ("\\", "Backslash"),
    ("/ this is a comment", "LineComment"),
    ("2024.01.15", "Date"),
    ("0Nd", "Date"),
    ("12:30:00.000", "Time"),
    ("0Nt", "Time"),
    ("2024.01.15D12:30:00.000000000", "Timestamp"),
    ("= < >", "Eq Lt Gt"),
    ("! & | ^ # _ ~ $ ? @ , .", "Bang Amp Pipe Caret Hash Underscore Tilde Dollar Query At Comma Dot"),
    ("x:42", "Ident Colon Integer"),
    ("assert.true", "Ident"),
    ("-42", "Minus Integer"),
    ("<=", "LtEq"),
    (">=", "GtEq"),
    ("<>", "NotEq"),
    ("x<=y", "Ident LtEq Ident"),
    ("0: x", "FileOp0 Ident"),
    ("1:", "FileOp1"),
    ("2:", "FileOp2"),
    ("10:x", "Integer Colon Ident"),
    ("x:0", "Ident Colon Integer"),
    ("`:data.csv", "FileSymbol"),
    ("`:/abs/path", "FileSymbol"),
    ("`:host:5001", "FileSymbol"),
    ("x+:1", "Ident CompoundAssign Integer"),
    ("x,:y", "Ident CompoundAssign Ident"),
    ("0Nm", "Month"),
    ("0Ng", "Guid"),
    ("0N", "Integer"),
    ("0Nj", "Integer"),
    ("0Nn", "Timespan"),
    ("0Np", "Timestamp"),
    ("0Wm", "Month"),
    ("0Wg", "Guid"),
    ("0Wn", "Timespan"),
    ("0Wz", "Datetime"),
    ("0Wu", "Minute"),
    ("0Wv", "Second"),
    ("0Wp", "Timestamp"),
    ("#!/usr/bin/env q", "Shebang"),
    ("# !x", "Hash Bang Ident"),
    ("2024.01m", "Month"),
    ("0D00:00:00.000000000", "Timespan"),
    ("1D", "Timespan"),
    ("2D", "Timespan"),
    ("0D", "Timespan"),
    ("0Nz", "Datetime"),
    ("12:30", "Minute"),
    ("0Nu", "Minute"),
    ("12:30:45", "Second"),
    ("0Nv", "Second"),
    ("12:30:45.678", "Time"),
    ("0xABCD", "ByteList"),
    ("0x0011223344", "ByteList"),
    ("0xAB", "Integer"),
    ("0x2A3", "ByteList"),
    ("/\nthis is\nblock comment\n\\\n", "CommentBlock"),
    ("/\nuntil end of file", "CommentBlock"),
    ("k)1+2", "DslLine"),
    ("p)select * from t", "DslLine"),
    ("k)foo\nbar", "DslLine Newline Ident"),
    ("k", "Ident"),
    ("/\nblock\nstuff\n\\\ny:2\n", "CommentBlock"),
    ("+:", "CompoundAssign"),
    ("-:", "CompoundAssign"),
    ("*:", "CompoundAssign"),
    ("%:", "CompoundAssign"),
    (">:", "CompoundAssign"),
    ("<:", "CompoundAssign"),
    ("~:", "CompoundAssign"),
    ("=:", "CompoundAssign"),
    ("_:", "CompoundAssign"),
    ("#:", "CompoundAssign"),
    ("$:", "CompoundAssign"),
    ("!:", "CompoundAssign"),
    ("|:", "CompoundAssign"),
    ("&:", "CompoundAssign"),
    ("?:", "CompoundAssign"),
    ("^:", "CompoundAssign"),
    (",:", "CompoundAssign"),
    ("@:", "CompoundAssign"),
    ];

    #[test]
    fn token_sequences() {
        for (src, expected) in CASES {
            let mut lex = Token::lexer(src);
            let mut kinds = Vec::new();
            while let Some(tok) = lex.next() {
                kinds.push(tok.map_or_else(|()| "ERR".to_string(), |t| format!("{t:?}")));
                if kinds.len() == 1 && !expected.contains(' ') {
                    assert_eq!(lex.slice(), *src, "span of {src:?}");
                }
            }
            assert_eq!(kinds.join(" "), *expected, "tokens of {src:?}");
        }
    }
}
