//! Lexer for q/kdb+ 4.1 source text.
//!
//! Converts raw source bytes into a flat [`Token`] stream via
//! [Logos](https://docs.rs/logos). Horizontal whitespace is skipped;
//! newlines are emitted as [`Token::Newline`] because they act as
//! statement separators in q.
//!
//! # Example
//! ```
//! use q_lexer::Token;
//! use logos::Logos;
//!
//! let tokens: Vec<_> = Token::lexer("x:42+y").collect();
//! ```

pub mod token;

pub use token::Token;
