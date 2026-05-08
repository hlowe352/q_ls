use logos::Logos;

/// All tokens produced by the q/kdb+ 4.1 lexer.
///
/// Horizontal whitespace (spaces and tabs) is skipped automatically.
/// Newlines are significant in q (statement separators) and are emitted as
/// [`Token::Newline`].
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
    /// Covers: `42`, `42i`, `42j`, `42h`, `0x2A`, `0Ni`, `0Wi`, `-0Wi`,
    ///         `0Nj`, `0Wj`, `0Nh`, `0Wh`, `0x[0-9A-Fa-f]{1,3}`
    ///
    /// Note: negative sign is NOT included here; the parser handles unary minus.
    /// Priority 3 keeps hex above the plain decimal regex (default priority 2).
    #[regex(r"0x[0-9A-Fa-f]{1,3}", priority = 4)]  // hex (1-3 digits)
    #[regex(r"0N[ijhp]", priority = 5)]        // typed nulls (guid/timespan/datetime/minute/second handled separately)
    #[regex(r"0W[ijhp]", priority = 5)]        // typed infs
    #[regex(r"[0-9]+[ijh]?")]                   // plain decimal, optional suffix
    Integer,

    /// Float literal
    ///
    /// Covers: `3.14`, `3.14e10`, `3.14f`, `3.14e`, `0n`, `0w`, `0Nf`,
    ///         `0Wf`, `0Ne`, `0We`
    #[regex(r"0[nw]", priority = 4)]            // generic float null/inf (lower-case)
    #[regex(r"0N[fe]", priority = 5)]           // typed float nulls
    #[regex(r"0W[fe]", priority = 5)]           // typed float infs
    #[regex(r"[0-9]+\.[0-9]*([eE][0-9]+)?[fe]?")]  // decimal float
    #[regex(r"[0-9]+[eE][0-9]+[fe]?")]         // scientific without dot
    Float,

    /// Timestamp literal: `2024.01.15D12:30:00.000000000`
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

    /// Timespan literal: `0D00:00:00.000000000`, `0Nn`, `0Wn`
    #[regex(r"0[NW]n", priority = 6)]
    #[regex(r"[0-9]+D[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?", priority = 6)]
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

    /// Byte list literal: `0xABCD`, `0x0011...` (4+ hex chars)
    #[regex(r"0x[0-9A-Fa-f]{4,}", priority = 6)]
    ByteList,

    /// String literal: `"hello"`, with escape sequences
    #[regex(r#""([^"\\]|\\.)*""#)]
    String,

    /// Symbol literal: `` `sym ``, `` `a.b ``, `` ` `` (null symbol)
    /// File handles: `` `:q1a.txt ``, `` `:/path/to/file ``, `` `:host:5001 ``
    #[regex(r"`:[^\s;)\]},]*", priority = 3)] // file handle / connection symbol
    #[regex(r"`[a-zA-Z_.][a-zA-Z0-9_.]*")]   // named symbol
    #[token("`")]                              // null symbol (lone backtick)
    Symbol,

    // -----------------------------------------------------------------------
    // Identifiers
    // -----------------------------------------------------------------------

    /// Dotted/namespaced identifier starting with dot: `.q.func`, `.Q.en`, `.z.ts`, `.d`
    #[regex(r"\.[a-zA-Z][a-zA-Z0-9]*(\.[a-zA-Z][a-zA-Z0-9]*)*")]
    DottedIdent,

    /// Regular identifier (may include dot-separated segments): `trade`, `myVar`, `assert.true`
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*")]
    Ident,

    // -----------------------------------------------------------------------
    // Operators / verbs
    // -----------------------------------------------------------------------

    #[token("+")]  Plus,
    #[token("-")]  Minus,
    #[token("*")]  Star,
    #[token("%")]  Percent,
    #[token("!")]  Bang,
    #[token("&")]  Amp,
    #[token("|")]  Pipe,
    #[token("^")]  Caret,
    #[token("#")]  Hash,
    #[token("_")]  Underscore,
    #[token("~")]  Tilde,
    #[token("$")]  Dollar,
    #[token("?")]  Query,
    #[token("@")]  At,
    #[token(",")]  Comma,
    #[token("<>")]  NotEq,
    #[token("<=")]  LtEq,
    #[token(">=")]  GtEq,
    #[token("=")]  Eq,
    #[token("<")]  Lt,
    #[token(">")]  Gt,
    #[token(".")]  Dot,

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

    #[token("(")]  LParen,
    #[token(")")]  RParen,
    #[token("[")]  LBracket,
    #[token("]")]  RBracket,
    #[token("{")]  LBrace,
    #[token("}")]  RBrace,
    #[token(";")]  Semi,

    // -----------------------------------------------------------------------
    // Special / structural
    // -----------------------------------------------------------------------

    /// Newline — significant in q as a statement separator
    #[regex(r"\r?\n")]
    Newline,

    /// Line comment: `/ comment text` (slash followed by space or end-of-line)
    /// Also covers full-line comments that begin with `/` or `//`.
    /// The lexer emits the entire comment (including the leading `/`) as one token.
    #[regex(r"//[^\r\n]*", priority = 8)]           // `//` style comment
    #[regex(r"/[^\S\r\n][^\r\n]*", priority = 8)]   // `/ ` followed by rest of line
    #[regex(r"/\r?\n", priority = 8)]               // bare `/` at end of line
    LineComment,

    /// Multi-line comment block. Opens with a line containing only `/` and
    /// closes with a line containing only `\` (or EOF). Greedy match.
    #[regex(r"/[ \t]*\r?\n([^\n]*\n)*\\[ \t]*(?:\r?\n)?", priority = 7)]
    #[regex(r"/[ \t]*\r?\n(?:[^\n]*\n)*[^\n]*", priority = 6)]
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

    /// Forward slash — "over" adverb (bare `/` not matched by LineComment)
    #[token("/")]
    Slash,

    // -----------------------------------------------------------------------
    // Error (unrecognized input)
    // -----------------------------------------------------------------------

    /// Catch-all for unrecognized input (logos 0.13+ uses the unit variant
    /// without `#[error]`; the `Err` side of the iterator carries this).
    Error,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
    fn lex_long_suffix() {
        let mut lex = Token::lexer("42j");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_int_suffix() {
        let mut lex = Token::lexer("42i");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_short_suffix() {
        let mut lex = Token::lexer("42h");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_hex() {
        let mut lex = Token::lexer("0x2A");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_typed_null_int() {
        let mut lex = Token::lexer("0Ni");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_typed_inf_int() {
        let mut lex = Token::lexer("0Wi");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_typed_null_short() {
        let mut lex = Token::lexer("0Nh");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
    }

    #[test]
    fn lex_float() {
        let mut lex = Token::lexer("3.14");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_float_scientific() {
        let mut lex = Token::lexer("3.14e10");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_float_suffix() {
        let mut lex = Token::lexer("3.14f");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_float_null() {
        let mut lex = Token::lexer("0n");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_float_inf() {
        let mut lex = Token::lexer("0w");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_typed_null_float() {
        let mut lex = Token::lexer("0Nf");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_typed_inf_float() {
        let mut lex = Token::lexer("0Wf");
        assert_eq!(lex.next(), Some(Ok(Token::Float)));
    }

    #[test]
    fn lex_boolean_list() {
        let mut lex = Token::lexer("010101b");
        assert_eq!(lex.next(), Some(Ok(Token::Boolean)));
    }

    #[test]
    fn lex_boolean_single_zero() {
        let mut lex = Token::lexer("0b");
        assert_eq!(lex.next(), Some(Ok(Token::Boolean)));
    }

    #[test]
    fn lex_boolean_single_one() {
        let mut lex = Token::lexer("1b");
        assert_eq!(lex.next(), Some(Ok(Token::Boolean)));
    }

    #[test]
    fn lex_symbol() {
        let mut lex = Token::lexer("`hello");
        assert_eq!(lex.next(), Some(Ok(Token::Symbol)));
        assert_eq!(lex.slice(), "`hello");
    }

    #[test]
    fn lex_null_symbol() {
        let mut lex = Token::lexer("`");
        assert_eq!(lex.next(), Some(Ok(Token::Symbol)));
    }

    #[test]
    fn lex_dotted_symbol() {
        let mut lex = Token::lexer("`a.b");
        assert_eq!(lex.next(), Some(Ok(Token::Symbol)));
        assert_eq!(lex.slice(), "`a.b");
    }

    #[test]
    fn lex_identifier() {
        let mut lex = Token::lexer("trade");
        assert_eq!(lex.next(), Some(Ok(Token::Ident)));
    }

    #[test]
    fn lex_identifier_with_digits() {
        let mut lex = Token::lexer("x1");
        assert_eq!(lex.next(), Some(Ok(Token::Ident)));
    }

    #[test]
    fn lex_dotted_ident() {
        let mut lex = Token::lexer(".q.func");
        assert_eq!(lex.next(), Some(Ok(Token::DottedIdent)));
    }

    #[test]
    fn lex_dotted_ident_upper() {
        let mut lex = Token::lexer(".Q.en");
        assert_eq!(lex.next(), Some(Ok(Token::DottedIdent)));
    }

    #[test]
    fn lex_operators() {
        let input = "+ - * %";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(
            tokens,
            vec![Token::Plus, Token::Minus, Token::Star, Token::Percent]
        );
    }

    #[test]
    fn lex_lambda() {
        let input = "{[x;y] x+y}";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(
            tokens,
            vec![
                Token::LBrace,
                Token::LBracket,
                Token::Ident,
                Token::Semi,
                Token::Ident,
                Token::RBracket,
                Token::Ident,
                Token::Plus,
                Token::Ident,
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn lex_string() {
        let mut lex = Token::lexer(r#""hello world""#);
        assert_eq!(lex.next(), Some(Ok(Token::String)));
    }

    #[test]
    fn lex_string_with_escape() {
        let mut lex = Token::lexer(r#""a\nb""#);
        assert_eq!(lex.next(), Some(Ok(Token::String)));
    }

    #[test]
    fn lex_system_cmd() {
        let mut lex = Token::lexer("\\l file.q");
        assert_eq!(lex.next(), Some(Ok(Token::SystemCmd)));
    }

    #[test]
    fn lex_system_cmd_t() {
        let mut lex = Token::lexer("\\t expr");
        assert_eq!(lex.next(), Some(Ok(Token::SystemCmd)));
    }

    #[test]
    fn lex_global_assign() {
        let mut lex = Token::lexer("::");
        assert_eq!(lex.next(), Some(Ok(Token::ColonColon)));
    }

    #[test]
    fn lex_colon() {
        let mut lex = Token::lexer(":");
        assert_eq!(lex.next(), Some(Ok(Token::Colon)));
    }

    #[test]
    fn lex_adverbs() {
        let mut lex = Token::lexer("':");
        assert_eq!(lex.next(), Some(Ok(Token::EachPrior)));
    }

    #[test]
    fn lex_each_right() {
        let mut lex = Token::lexer("/:");
        assert_eq!(lex.next(), Some(Ok(Token::EachRight)));
    }

    #[test]
    fn lex_each_left() {
        let mut lex = Token::lexer("\\:");
        assert_eq!(lex.next(), Some(Ok(Token::EachLeft)));
    }

    #[test]
    fn lex_each() {
        let mut lex = Token::lexer("'");
        assert_eq!(lex.next(), Some(Ok(Token::Each)));
    }

    #[test]
    fn lex_newlines_preserved() {
        let input = "a\nb";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Ident, Token::Newline, Token::Ident]);
    }

    #[test]
    fn lex_exit() {
        let mut lex = Token::lexer("\\\\");
        assert_eq!(lex.next(), Some(Ok(Token::Exit)));
    }

    #[test]
    fn lex_slash_over() {
        let mut lex = Token::lexer("/");
        assert_eq!(lex.next(), Some(Ok(Token::Slash)));
    }

    #[test]
    fn lex_backslash_scan() {
        // bare `\` not followed by letter → Backslash (scan adverb)
        let mut lex = Token::lexer("\\");
        assert_eq!(lex.next(), Some(Ok(Token::Backslash)));
    }

    #[test]
    fn lex_line_comment() {
        let mut lex = Token::lexer("/ this is a comment");
        assert_eq!(lex.next(), Some(Ok(Token::LineComment)));
    }

    #[test]
    fn lex_date() {
        let mut lex = Token::lexer("2024.01.15");
        assert_eq!(lex.next(), Some(Ok(Token::Date)));
    }

    #[test]
    fn lex_date_null() {
        let mut lex = Token::lexer("0Nd");
        assert_eq!(lex.next(), Some(Ok(Token::Date)));
    }

    #[test]
    fn lex_time() {
        let mut lex = Token::lexer("12:30:00.000");
        assert_eq!(lex.next(), Some(Ok(Token::Time)));
    }

    #[test]
    fn lex_time_null() {
        let mut lex = Token::lexer("0Nt");
        assert_eq!(lex.next(), Some(Ok(Token::Time)));
    }

    #[test]
    fn lex_timestamp() {
        let mut lex = Token::lexer("2024.01.15D12:30:00.000000000");
        assert_eq!(lex.next(), Some(Ok(Token::Timestamp)));
    }

    #[test]
    fn lex_comparison_ops() {
        let input = "= < >";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Eq, Token::Lt, Token::Gt]);
    }

    #[test]
    fn lex_other_ops() {
        let input = "! & | ^ # _ ~ $ ? @ , .";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(
            tokens,
            vec![
                Token::Bang,
                Token::Amp,
                Token::Pipe,
                Token::Caret,
                Token::Hash,
                Token::Underscore,
                Token::Tilde,
                Token::Dollar,
                Token::Query,
                Token::At,
                Token::Comma,
                Token::Dot,
            ]
        );
    }

    #[test]
    fn lex_mixed_statement() {
        // `x:42` — assignment
        let input = "x:42";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Ident, Token::Colon, Token::Integer]);
    }

    #[test]
    fn lex_assert_dot_name() {
        let tokens: Vec<_> = Token::lexer("assert.true").collect();
        for (i, t) in tokens.iter().enumerate() {
            match t {
                Ok(tok) => println!("  {}: {:?}", i, tok),
                Err(()) => println!("  {}: ERROR", i),
            }
        }
        assert_eq!(tokens.len(), 1); // single namespaced ident
        assert_eq!(tokens[0], Ok(Token::Ident));
    }

    #[test]
    fn lex_no_negative_in_integer() {
        // `-42` should be Minus then Integer, not a single negative integer
        let input = "-42";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Minus, Token::Integer]);
    }

    #[test]
    fn lex_less_equal() {
        let mut lex = Token::lexer("<=");
        assert_eq!(lex.next(), Some(Ok(Token::LtEq)));
    }

    #[test]
    fn lex_greater_equal() {
        let mut lex = Token::lexer(">=");
        assert_eq!(lex.next(), Some(Ok(Token::GtEq)));
    }

    #[test]
    fn lex_not_equal() {
        let mut lex = Token::lexer("<>");
        assert_eq!(lex.next(), Some(Ok(Token::NotEq)));
    }

    #[test]
    fn lex_comparison_mixed() {
        // x<=y should be Ident LtEq Ident, not Ident Lt Eq Ident
        let tokens: Vec<_> = Token::lexer("x<=y").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Ident, Token::LtEq, Token::Ident]);
    }

    #[test]
    fn lex_file_op_0() {
        let tokens: Vec<_> = Token::lexer("0: x").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::FileOp0, Token::Ident]);
    }

    #[test]
    fn lex_file_op_1() {
        let mut lex = Token::lexer("1:");
        assert_eq!(lex.next(), Some(Ok(Token::FileOp1)));
    }

    #[test]
    fn lex_file_op_2() {
        let mut lex = Token::lexer("2:");
        assert_eq!(lex.next(), Some(Ok(Token::FileOp2)));
    }

    #[test]
    fn lex_file_op_not_10_colon() {
        // `10:` should be Integer(10) Colon, NOT Integer(1) FileOp0
        let tokens: Vec<_> = Token::lexer("10:x").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Integer, Token::Colon, Token::Ident]);
    }

    #[test]
    fn lex_assign_zero() {
        // `x:0` should be Ident Colon Integer, not Ident FileOp0 with missing text
        let tokens: Vec<_> = Token::lexer("x:0").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Ident, Token::Colon, Token::Integer]);
    }

    #[test]
    fn lex_file_handle_symbol() {
        let mut lex = Token::lexer("`:data.csv");
        assert_eq!(lex.next(), Some(Ok(Token::Symbol)));
        assert_eq!(lex.slice(), "`:data.csv");
    }

    #[test]
    fn lex_compound_assign_plus() {
        let tokens: Vec<_> = Token::lexer("x+:1").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Ident, Token::CompoundAssign, Token::Integer]);
    }

    #[test]
    fn lex_compound_assign_comma() {
        let tokens: Vec<_> = Token::lexer("x,:y").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Ident, Token::CompoundAssign, Token::Ident]);
    }

    #[test]
    fn lex_compound_assign_all() {
        for op in ["+:", "-:", "*:", "%:", ">:", "<:", "~:", "=:", "_:", "#:", "$:", "!:", "|:", "&:", "?:", "^:", ",:", "@:"] {
            let mut lex = Token::lexer(op);
            assert_eq!(lex.next(), Some(Ok(Token::CompoundAssign)), "failed for {}", op);
        }
    }

    #[test]
    fn lex_compound_assign_not_colon_colon() {
        // :: should still be ColonColon, not CompoundAssign
        let mut lex = Token::lexer("::");
        assert_eq!(lex.next(), Some(Ok(Token::ColonColon)));
    }

    #[test]
    fn lex_compound_assign_not_adverbs() {
        // '/: \: ': should still be adverbs
        assert_eq!(Token::lexer("/:").next(), Some(Ok(Token::EachRight)));
        assert_eq!(Token::lexer("\\:").next(), Some(Ok(Token::EachLeft)));
        assert_eq!(Token::lexer("':").next(), Some(Ok(Token::EachPrior)));
    }

    #[test]
    fn lex_month_null() {
        let mut lex = Token::lexer("0Nm");
        assert_eq!(lex.next(), Some(Ok(Token::Month)));
        assert_eq!(lex.slice(), "0Nm");
    }

    #[test]
    fn lex_guid_null() {
        let mut lex = Token::lexer("0Ng");
        assert_eq!(lex.next(), Some(Ok(Token::Guid)));
        assert_eq!(lex.slice(), "0Ng");
    }

    #[test]
    fn lex_timespan_null() {
        let mut lex = Token::lexer("0Nn");
        assert_eq!(lex.next(), Some(Ok(Token::Timespan)));
        assert_eq!(lex.slice(), "0Nn");
    }

    #[test]
    fn lex_timestamp_null() {
        let mut lex = Token::lexer("0Np");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)));
        assert_eq!(lex.slice(), "0Np");
    }

    #[test]
    fn lex_temporal_infs() {
        // 0Wm is Month, 0Wg/0Wn/0Wz are their respective types
        // 0Wu is Minute, 0Wv is Second, 0Wp is Integer
        assert_eq!(Token::lexer("0Wm").next(), Some(Ok(Token::Month)));
        assert_eq!(Token::lexer("0Wg").next(), Some(Ok(Token::Guid)));
        assert_eq!(Token::lexer("0Wn").next(), Some(Ok(Token::Timespan)));
        assert_eq!(Token::lexer("0Wz").next(), Some(Ok(Token::Datetime)));
        assert_eq!(Token::lexer("0Wu").next(), Some(Ok(Token::Minute)));
        assert_eq!(Token::lexer("0Wv").next(), Some(Ok(Token::Second)));
        // 0Wp is still Integer
        let mut lex = Token::lexer("0Wp");
        assert_eq!(lex.next(), Some(Ok(Token::Integer)), "failed for 0Wp");
    }

    #[test]
    fn lex_shebang() {
        let mut lex = Token::lexer("#!/usr/bin/env q");
        assert_eq!(lex.next(), Some(Ok(Token::Shebang)));
        assert_eq!(lex.slice(), "#!/usr/bin/env q");
    }

    #[test]
    fn lex_shebang_not_hash_bang() {
        // Without `!` immediately after `#`, should be Hash then Bang
        let tokens: Vec<_> = Token::lexer("# !x").map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Hash, Token::Bang, Token::Ident]);
    }

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

    #[test]
    fn lex_byte_list() {
        assert_eq!(Token::lexer("0xABCD").next(), Some(Ok(Token::ByteList)));
        assert_eq!(Token::lexer("0x0011223344").next(), Some(Ok(Token::ByteList)));
    }

    #[test]
    fn lex_single_byte_stays_integer() {
        assert_eq!(Token::lexer("0xAB").next(), Some(Ok(Token::Integer)));
    }

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
}
