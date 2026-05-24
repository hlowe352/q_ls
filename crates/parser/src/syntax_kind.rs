/// All syntax kinds for the q/kdb+ 4.1 CST, used with rowan.
///
/// The first group mirrors the lexer's [`q_lexer::Token`] variants 1-to-1.
/// The second group contains composite node kinds for the parser's CST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // -----------------------------------------------------------------------
    // Token kinds — mirror q_lexer::Token exactly (same names, same order)
    // -----------------------------------------------------------------------
    Boolean = 0,
    Integer,
    Float,
    Timestamp,
    Date,
    Month,
    Guid,
    Timespan,
    Datetime,
    Minute,
    Second,
    Time,
    ByteList,
    String,
    Symbol,
    FileSymbol,
    DottedIdent,
    Ident,
    Plus,
    Minus,
    Star,
    Percent,
    Bang,
    Amp,
    Pipe,
    Caret,
    Hash,
    Underscore,
    Tilde,
    Dollar,
    Query,
    At,
    Comma,
    NotEq,
    LtEq,
    GtEq,
    Eq,
    Lt,
    Gt,
    Dot,
    CompoundAssign,
    FileOp0,
    FileOp1,
    FileOp2,
    ColonColon,
    Colon,
    EachPrior,
    EachRight,
    EachLeft,
    Each,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Semi,
    Newline,
    LineComment,
    Shebang,
    Exit,
    SystemCmd,
    Backslash,
    Slash,
    CommentBlock,
    DslLine,
    Error,

    // -----------------------------------------------------------------------
    // Extra token-level kind the parser synthesises
    // -----------------------------------------------------------------------

    /// Horizontal whitespace (spaces / tabs) that the lexer skips but the
    /// parser may re-inject when building a lossless CST.
    Whitespace,

    // -----------------------------------------------------------------------
    // Composite node kinds
    // -----------------------------------------------------------------------

    /// Top-level file node.
    Root,
    /// Expression used as a statement (`expr ;` or `expr <newline>`).
    ExprStmt,
    /// `ident : expr` or `ident :: expr`.
    AssignStmt,
    /// `{[params] body}`.
    Lambda,
    /// `[x;y;z]` — formal parameter list.
    ParamList,
    /// `[expr;expr;...]` — call argument list.
    ArgList,
    /// `left op right` — binary (dyadic) expression.
    BinExpr,
    /// `op expr` — unary (monadic) expression.
    UnaryExpr,
    /// `f[args]` or `f x` — function application.
    ApplyExpr,
    /// `expr adverb` — adverb / iterator application.
    AdverbExpr,
    /// `(expr)` — parenthesised expression.
    ParenExpr,
    /// `(expr;expr;...)` — list literal.
    ListExpr,
    /// `$[cond;true;false]` — conditional expression.
    CondExpr,
    /// Identifier reference.
    IdentExpr,
    /// Any literal value.
    LiteralExpr,
    /// `([] col:val; ...)` — table expression.
    TableExpr,
    /// `keys!values` — dictionary expression.
    DictExpr,
    /// `select … from …` qSQL expression.
    SelectExpr,
    /// `update … from …` qSQL expression.
    UpdateExpr,
    /// `exec … from …` qSQL expression.
    ExecExpr,
    /// `delete … from …` qSQL expression.
    DeleteExpr,
    /// Column list inside qSQL.
    ColumnList,
    /// `where …` clause inside qSQL.
    WhereClause,
    /// `by …` clause inside qSQL.
    ByClause,
    /// System command statement (`\l file.q`, etc.).
    SystemCmdStmt,
    /// Sequence of statements.
    Block,
    /// `expr[index]` — index / slice expression.
    IndexExpr,
    /// `if[cond;expr;...]` — if control word.
    IfExpr,
    /// `do[n;expr;...]` — do control word.
    DoExpr,
    /// `while[cond;expr;...]` — while control word.
    WhileExpr,
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
    /// `` `:path `` literal expression node (wraps the `FileSymbol` token).
    FileSymbolExpr,
    /// `[stmt; stmt; ...]` — progn (sequence of statements, returns last).
    PrognExpr,

    // -----------------------------------------------------------------------
    // Sentinel — must remain last
    // -----------------------------------------------------------------------
    #[doc(hidden)]
    __LAST,
}

impl SyntaxKind {
    /// Map a lexer token to the corresponding [`SyntaxKind`].
    #[must_use] 
    pub fn from_token(token: q_lexer::Token) -> Self {
        match token {
            q_lexer::Token::Boolean     => SyntaxKind::Boolean,
            q_lexer::Token::Integer     => SyntaxKind::Integer,
            q_lexer::Token::Float       => SyntaxKind::Float,
            q_lexer::Token::Timestamp   => SyntaxKind::Timestamp,
            q_lexer::Token::Date        => SyntaxKind::Date,
            q_lexer::Token::Month       => SyntaxKind::Month,
            q_lexer::Token::Guid        => SyntaxKind::Guid,
            q_lexer::Token::Timespan    => SyntaxKind::Timespan,
            q_lexer::Token::Datetime    => SyntaxKind::Datetime,
            q_lexer::Token::Minute      => SyntaxKind::Minute,
            q_lexer::Token::Second      => SyntaxKind::Second,
            q_lexer::Token::Time        => SyntaxKind::Time,
            q_lexer::Token::ByteList    => SyntaxKind::ByteList,
            q_lexer::Token::String      => SyntaxKind::String,
            q_lexer::Token::Symbol      => SyntaxKind::Symbol,
            q_lexer::Token::FileSymbol  => SyntaxKind::FileSymbol,
            q_lexer::Token::DottedIdent => SyntaxKind::DottedIdent,
            q_lexer::Token::Ident       => SyntaxKind::Ident,
            q_lexer::Token::Plus        => SyntaxKind::Plus,
            q_lexer::Token::Minus       => SyntaxKind::Minus,
            q_lexer::Token::Star        => SyntaxKind::Star,
            q_lexer::Token::Percent     => SyntaxKind::Percent,
            q_lexer::Token::Bang        => SyntaxKind::Bang,
            q_lexer::Token::Amp         => SyntaxKind::Amp,
            q_lexer::Token::Pipe        => SyntaxKind::Pipe,
            q_lexer::Token::Caret       => SyntaxKind::Caret,
            q_lexer::Token::Hash        => SyntaxKind::Hash,
            q_lexer::Token::Underscore  => SyntaxKind::Underscore,
            q_lexer::Token::Tilde       => SyntaxKind::Tilde,
            q_lexer::Token::Dollar      => SyntaxKind::Dollar,
            q_lexer::Token::Query       => SyntaxKind::Query,
            q_lexer::Token::At          => SyntaxKind::At,
            q_lexer::Token::Comma       => SyntaxKind::Comma,
            q_lexer::Token::NotEq       => SyntaxKind::NotEq,
            q_lexer::Token::LtEq        => SyntaxKind::LtEq,
            q_lexer::Token::GtEq        => SyntaxKind::GtEq,
            q_lexer::Token::Eq          => SyntaxKind::Eq,
            q_lexer::Token::Lt          => SyntaxKind::Lt,
            q_lexer::Token::Gt          => SyntaxKind::Gt,
            q_lexer::Token::Dot         => SyntaxKind::Dot,
            q_lexer::Token::CompoundAssign => SyntaxKind::CompoundAssign,
            q_lexer::Token::FileOp0     => SyntaxKind::FileOp0,
            q_lexer::Token::FileOp1     => SyntaxKind::FileOp1,
            q_lexer::Token::FileOp2     => SyntaxKind::FileOp2,
            q_lexer::Token::ColonColon  => SyntaxKind::ColonColon,
            q_lexer::Token::Colon       => SyntaxKind::Colon,
            q_lexer::Token::EachPrior   => SyntaxKind::EachPrior,
            q_lexer::Token::EachRight   => SyntaxKind::EachRight,
            q_lexer::Token::EachLeft    => SyntaxKind::EachLeft,
            q_lexer::Token::Each        => SyntaxKind::Each,
            q_lexer::Token::LParen      => SyntaxKind::LParen,
            q_lexer::Token::RParen      => SyntaxKind::RParen,
            q_lexer::Token::LBracket    => SyntaxKind::LBracket,
            q_lexer::Token::RBracket    => SyntaxKind::RBracket,
            q_lexer::Token::LBrace      => SyntaxKind::LBrace,
            q_lexer::Token::RBrace      => SyntaxKind::RBrace,
            q_lexer::Token::Semi        => SyntaxKind::Semi,
            q_lexer::Token::Newline     => SyntaxKind::Newline,
            q_lexer::Token::LineComment => SyntaxKind::LineComment,
            q_lexer::Token::Shebang     => SyntaxKind::Shebang,
            q_lexer::Token::Exit        => SyntaxKind::Exit,
            q_lexer::Token::SystemCmd   => SyntaxKind::SystemCmd,
            q_lexer::Token::Backslash   => SyntaxKind::Backslash,
            q_lexer::Token::Slash       => SyntaxKind::Slash,
            q_lexer::Token::CommentBlock => SyntaxKind::CommentBlock,
            q_lexer::Token::DslLine     => SyntaxKind::DslLine,
            q_lexer::Token::Error       => SyntaxKind::Error,
        }
    }

    /// Returns `true` for trivia kinds that are typically skipped by the
    /// parser but preserved in the lossless CST.
    #[must_use] 
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::Whitespace | SyntaxKind::Newline | SyntaxKind::LineComment | SyntaxKind::Shebang | SyntaxKind::CommentBlock)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

/// Zero-sized marker type that implements [`rowan::Language`] for q/kdb+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QLang {}

impl rowan::Language for QLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        assert!(raw.0 < SyntaxKind::__LAST as u16, "raw SyntaxKind value out of bounds");
        // SAFETY: we just checked the value is within the valid range.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A lossless syntax node parameterised over the q language.
pub type SyntaxNode = rowan::SyntaxNode<QLang>;
/// A lossless syntax token parameterised over the q language.
pub type SyntaxToken = rowan::SyntaxToken<QLang>;
/// A lossless syntax element (either a node or a token) parameterised over the q language.
pub type SyntaxElement = rowan::SyntaxElement<QLang>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_new_tokens() {
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Month),        SyntaxKind::Month);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Guid),         SyntaxKind::Guid);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Timespan),     SyntaxKind::Timespan);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Minute),       SyntaxKind::Minute);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Second),       SyntaxKind::Second);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::Datetime),     SyntaxKind::Datetime);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::ByteList),     SyntaxKind::ByteList);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::CommentBlock), SyntaxKind::CommentBlock);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::DslLine),      SyntaxKind::DslLine);
        assert_eq!(SyntaxKind::from_token(q_lexer::Token::FileSymbol),   SyntaxKind::FileSymbol);
    }

    #[test]
    fn comment_block_is_trivia() {
        assert!(SyntaxKind::CommentBlock.is_trivia());
    }

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
}
