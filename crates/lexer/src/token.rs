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
#[logos(subpattern ymd = r"[0-9]{4}\.(0[1-9]|1[0-2])\.(0[1-9]|[12][0-9]|3[01])")]
// Time after `D`/`T`: a bare hour (`D0`), or `hh:mm:ss.fff` cut short after
// any colon (`D12:`, `D12:30`).
#[logos(subpattern hms = r"([0-9]{1,2}|[0-9]{2}:([0-5][0-9](:([0-5][0-9](\.[0-9]*)?)?)?)?)?")]
pub enum Token {
    // -----------------------------------------------------------------------
    // Literals
    // -----------------------------------------------------------------------
    // Numbers and times. `0[NWnw]` + a type letter is that type's null or
    // infinity. Any other number-like word is one `Error` token (`2b`,
    // `1.5i`, `12:60`), as in q, rather than a literal followed by a name.
    /// Boolean literal: `0b`, `1b`, `010101b`
    #[regex(r"[01]+b")]
    Boolean,

    /// Integer literal: `42`, `42i`, `42j`, `42h`, `0N`, `0W`, `0Ni`, `0wj`.
    /// The sign is merged by the parser.
    #[regex(r"[0-9]+[ijh]?|0[NW]|0[NWnw][ijh]")]
    Integer,

    /// Float literal: `3.14`, `.5`, `1.`, `1e-5`, `3f`, `3e`, `0n`, `0w`, `0Nf`
    /// (priority: `0n` is a float, not `[0-9]+n`)
    #[regex(r"([0-9]+\.[0-9]*|\.[0-9]+)(e[-+]?[0-9]+)?[ef]?|[0-9]+(e[-+]?[0-9]+)?[ef]|[0-9]+e[-+]?[0-9]+|0[nw]|0[NWnw][ef]", priority = 10)]
    Float,

    /// Timestamp literal: `2024.01.15D12:30:00.000000000`, `2024.01.15D`, `0p`, `0Np`
    #[regex(r"(?&ymd)D(?&hms)|[0-9]+p|0[NWnw]p")]
    Timestamp,

    /// Date literal: `2024.01.15`, `0Nd`
    #[regex(r"(?&ymd)|0[NWnw]d")]
    Date,

    /// Month literal: `2024.01m`, `0Nm`
    #[regex(r"[0-9]{4}\.(0[1-9]|1[0-2])m|0[NWnw]m")]
    Month,

    /// Guid null: `0Ng`
    #[regex(r"0[Nn]g")]
    Guid,

    /// Timespan literal: `1D`, `0D00`, `0D12:30:00.000000000`, `5n`, `0Nn`
    #[regex(r"[0-9]+D(?&hms)|[0-9]+n|0[NWnw]n")]
    Timespan,

    /// Datetime literal: `2024.01.15T12:30:00.000`, `0Nz`
    #[regex(r"(?&ymd)T(?&hms)|0[NWnw]z")]
    Datetime,

    /// Minute literal: `12:30`, `12:` and `12:30:` (a trailing colon ends a
    /// time early), `325:`, `12:30u`, `5u`, `0Nu`
    #[regex(r"[0-9]{2,}:([0-5][0-9]:?)?u?|[0-9]+u|0[NWnw]u")]
    Minute,

    /// Second literal: `12:30:45`, `12:30:45:`, `5v`, `0Nv`
    #[regex(r"[0-9]{2,}:[0-5][0-9]:[0-5][0-9]:?v?|[0-9]+v|0[NWnw]v")]
    Second,

    /// Time literal: `12:30:45.678`, `12:30:45t`, `5t`, `0Nt`
    #[regex(r"[0-9]{2,}:[0-5][0-9]:[0-5][0-9](\.[0-9]*t?|t)|[0-9]+t|0[NWnw]t")]
    Time,

    /// Byte literal: `0x2A` (atom), `0x`, `0x2A3B` (list)
    #[regex(r"0x[0-9A-Fa-f]*")]
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
    /// DSL escape line: `k)expr` or `p)expr` at line start (recognised by
    /// the parser, which knows the line context).
    DslLine,

    /// Identifier starting with a dot: `.q.func`, `.Q.en`, `.z.ts`, `.d`
    #[regex(r"\.[a-zA-Z][a-zA-Z0-9_.]*")]
    DottedIdent,

    /// Identifier; may contain dots (`assert.true`, `.a.1b`, `x.`).
    #[regex(r"[a-zA-Z][a-zA-Z0-9_.]*")]
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
    /// Compound assignment: `+:`, `,:`, and `+::` (which, like `::`, amends
    /// a global unless a local of that name exists).
    #[regex(r"[-+*%><~=_#$!|&?^,@]::?", priority = 3)]
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

    /// Line comment: `/` at line start or after whitespace, to end of line.
    /// Comments need line context, so the parser recognises them.
    LineComment,

    /// Block comment: a column-0 `/` line up to a column-0 `\` line, or a
    /// column-0 `\` line to the end of the script. Recognised by the parser.
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
    /// Unrecognised input, including number-like words that are no literal
    /// (`2b`, `1.5i`). Lowest priority, so only wins as the longest match.
    #[regex(r"\.?[0-9]([0-9A-Za-z.]|e[-+][0-9]|[0-9]:[0-9A-Za-z])*", priority = 0)]
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
    ("0x2A", "ByteList"),
    ("0Ni", "Integer"),
    ("0Wi", "Integer"),
    ("0Nh", "Integer"),
    ("3.14", "Float"),
    (".5", "Float"),
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
    ("0Wg", "Error"), // q has no guid infinity
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
    ("0xAB", "ByteList"),
    ("0x2A3", "ByteList"),
    ("k", "Ident"),
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
    ("x+::1", "Ident CompoundAssign Integer"),
    // Forms checked against q 4.1.
    ("0W", "Integer"),
    ("0wj", "Integer"),
    (".5", "Float"),
    ("1.", "Float"),
    ("1e-5", "Float"),
    ("1e+5", "Float"),
    ("1.5e5e", "Float"),
    ("0nf", "Float"),
    ("0p", "Timestamp"),
    ("2000.01.05D", "Timestamp"),
    ("1970.01.01D00:00", "Timestamp"),
    ("0wz", "Datetime"),
    ("2000.01.01T12", "Datetime"),
    ("0D00", "Timespan"),
    ("0D1", "Timespan"),
    ("2000.01.01D0", "Timestamp"),
    ("0D1:30", "Error"),
    ("5n", "Timespan"),
    ("0wu", "Minute"),
    ("12:30u", "Minute"),
    ("99:59", "Minute"),
    ("12:30:00t", "Time"),
    ("0Wd", "Date"),
    ("0x", "ByteList"),
    (".adv.sr.223", "DottedIdent"),
    ("x.", "Ident"),
    ("a.1", "Ident"),
    ("1:2", "FileOp1 Integer"),
    ("12:", "Minute"),
    ("325:", "Minute"),
    ("12:30:", "Minute"),
    ("12:30:45:", "Second"),
    ("100D12:", "Timespan"),
    ("2000.01.01D12:", "Timestamp"),
    ("10: x", "Minute Ident"),
    ("0:x", "FileOp0 Ident"),
    // Number-like words that are no literal are one error token.
    ("2b", "Error"),
    ("1E5", "Error"),
    ("1.22i", "Error"),
    ("1e22i", "Error"),
    ("12:60", "Error"),
    ("10:x", "Error"),
    ("10:1", "Error"),
    ("12:5", "Error"),
    ("2024.13.01", "Error"),
    ("1..2", "Error"),
    ("12:42:62346.123", "Error"),
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
