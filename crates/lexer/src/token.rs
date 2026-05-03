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
    ///         `0Nj`, `0Wj`, `0Nh`, `0Wh`, `0x[0-9A-Fa-f]+`
    ///
    /// Note: negative sign is NOT included here; the parser handles unary minus.
    /// Priority 3 keeps hex above the plain decimal regex (default priority 2).
    #[regex(r"0x[0-9A-Fa-f]+", priority = 4)]  // hex
    #[regex(r"0N[ijh]", priority = 5)]          // typed int/long/short nulls
    #[regex(r"0W[ijh]", priority = 5)]          // typed int/long/short infs
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
    #[regex(r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}D[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?")]
    Timestamp,

    /// Date literal: `2024.01.15`, `0Nd`
    #[regex(r"0Nd")]
    #[regex(r"[0-9]{4}\.[0-9]{2}\.[0-9]{2}")]
    Date,

    /// Time literal: `12:30:00.000`, `0Nt`
    #[regex(r"0Nt")]
    #[regex(r"[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?")]
    Time,

    /// String literal: `"hello"`, with escape sequences
    #[regex(r#""([^"\\]|\\.)*""#)]
    String,

    /// Symbol literal: `` `sym ``, `` `a.b ``, `` ` `` (null symbol)
    #[regex(r"`[a-zA-Z_.][a-zA-Z0-9_.]*")]  // named symbol
    #[token("`")]                             // null symbol (lone backtick)
    Symbol,

    // -----------------------------------------------------------------------
    // Identifiers
    // -----------------------------------------------------------------------

    /// Dotted/namespaced identifier: `.q.func`, `.Q.en`, `.z.ts`
    #[regex(r"\.[a-zA-Z][a-zA-Z0-9]*(\.[a-zA-Z][a-zA-Z0-9]*)+")]
    DottedIdent,

    /// Regular identifier: `trade`, `myVar`, `x1`
    #[regex(r"[a-zA-Z][a-zA-Z0-9_]*")]
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
    #[token("=")]  Eq,
    #[token("<")]  Lt,
    #[token(">")]  Gt,
    #[token(".")]  Dot,

    // -----------------------------------------------------------------------
    // Assignment / colon variants
    // -----------------------------------------------------------------------

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
    /// Also covers full-line comments that begin with `/`.
    /// The lexer emits the entire comment (including the leading `/`) as one token.
    #[regex(r"/[^\S\r\n][^\r\n]*")]   // `/ ` followed by rest of line
    #[regex(r"/\r?\n")]               // bare `/` at end of line
    LineComment,

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
    fn lex_no_negative_in_integer() {
        // `-42` should be Minus then Integer, not a single negative integer
        let input = "-42";
        let tokens: Vec<_> = Token::lexer(input).map(|r| r.unwrap()).collect();
        assert_eq!(tokens, vec![Token::Minus, Token::Integer]);
    }
}
