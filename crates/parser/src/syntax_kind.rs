/// Defines [`SyntaxKind`]: the lexer's token kinds (listed once, mirrored
/// 1-to-1 from [`q_lexer::Token`] together with `from_token`), the synthetic
/// `Whitespace` token, then the parser's composite node kinds.
macro_rules! syntax_kinds {
    (tokens { $($tok:ident),* $(,)? } nodes { $($(#[$doc:meta])* $node:ident,)* }) => {
        /// All syntax kinds for the q/kdb+ 4.1 CST, used with rowan.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[repr(u16)]
        pub enum SyntaxKind {
            $($tok,)*
            /// Horizontal whitespace (spaces / tabs) that the lexer skips but
            /// the parser re-injects to keep the CST lossless.
            Whitespace,
            $($(#[$doc])* $node,)*
            #[doc(hidden)]
            __LAST,
        }

        impl SyntaxKind {
            /// Map a lexer token to the corresponding [`SyntaxKind`].
            #[must_use]
            pub fn from_token(token: q_lexer::Token) -> Self {
                match token {
                    $(q_lexer::Token::$tok => Self::$tok,)*
                }
            }
        }
    };
}

syntax_kinds! {
    tokens {
        Boolean, Integer, Float, Timestamp, Date, Month, Guid, Timespan, Datetime,
        Minute, Second, Time, ByteList, String, Symbol, FileSymbol, DottedIdent, Ident,
        Plus, Minus, Star, Percent, Bang, Amp, Pipe, Caret, Hash, Underscore, Tilde,
        Dollar, Query, At, Comma, NotEq, LtEq, GtEq, Eq, Lt, Gt, Dot, CompoundAssign,
        FileOp0, FileOp1, FileOp2, ColonColon, Colon, EachPrior, EachRight, EachLeft,
        Each, LParen, RParen, LBracket, RBracket, LBrace, RBrace, Semi, Newline,
        LineComment, Shebang, Exit, SystemCmd, Backslash, Slash, CommentBlock, DslLine,
        Error
    }
    nodes {
        /// Top-level file node.
        Root,
        /// Expression used as a statement (`expr ;` or `expr <newline>`).
        ExprStmt,
        /// `{[params] body}`.
        Lambda,
        /// `[x;y;z]` — formal parameter list.
        ParamList,
        /// `[expr;expr;...]` — call argument list.
        ArgList,
        /// `x verb y` — infix application. The verb is a direct token for a
        /// primitive (`x+y`, `x in y`), else a node (`x f/ y`, `x+/y`).
        BinExpr,
        /// A bare primitive verb (`+`, `(-)`, `'` in `'[f;g]`).
        VerbExpr,
        /// `f x` — application by juxtaposition.
        ApplyExpr,
        /// `g f+`, `x+y+` — composition: application to, or a verb's right
        /// operand being, an expression that ends in a verb.
        Composition,
        /// `expr adverb` — adverb / iterator application.
        AdverbExpr,
        /// `(expr)` — parenthesised expression.
        ParenExpr,
        /// `(expr;expr;...)` — list literal.
        ListExpr,
        /// `1 2 3` — space-separated homogeneous scalar vector.
        VectorExpr,
        /// Identifier reference.
        IdentExpr,
        /// Any literal value.
        LiteralExpr,
        /// `([] col:val; ...)` — table expression.
        TableExpr,
        /// `([k:v; ...])` — dictionary literal: keys in brackets, nothing after.
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
        /// `from <table>` clause inside qSQL.
        FromClause,
        /// System command statement (`\l file.q`, etc.).
        SystemCmdStmt,
        /// `f[args]` — bracket application (also `if[…]`, `$[…]`, `'[f;g]`).
        IndexExpr,
        /// `x+`, `x f/` — infix verb with no right operand (a projection).
        InfixProjection,
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
    }
}

impl SyntaxKind {
    /// Returns `true` for trivia kinds that are typically skipped by the
    /// parser but preserved in the lossless CST.
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace
                | SyntaxKind::Newline
                | SyntaxKind::LineComment
                | SyntaxKind::Shebang
                | SyntaxKind::CommentBlock
        )
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
        assert!(
            raw.0 < SyntaxKind::__LAST as u16,
            "raw SyntaxKind value out of bounds"
        );
        // SAFETY: `SyntaxKind` is `repr(u16)` with contiguous discriminants
        // below `__LAST`, which we just checked.
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
