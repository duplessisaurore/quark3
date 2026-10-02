//! lexer.rs
//!
//! This is the lexing component of the `Photon3` sugar layer,
//! responsible for:
//! 
//! - tokenising the content
//! - handling raw blocks of boson3
//! - handling numeric literals

use std::fmt;

use crate::ast::{Located, SourceSpan};

/// One token produced by the lexer
pub type Token = Located<TokenKind>;

/// A boson3 body token, this is essentially
/// some boson3 code that exists in photon3/legacy
/// pass over
#[derive(Debug, Clone, PartialEq)]
pub struct BosonBodyToken {
    pub declared_type: String,
    pub body: String,
    pub body_span: SourceSpan,
}

/// All kinds of lexed tokens by Boson3
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Basic literals/idents
    Identifier(String),
    IntLiteral(i64),
    UIntLiteral(u64),
    FloatLiteral(f64),
    BoolLiteral(bool),

    // Boson3 directives/body/macro statements
    Directive(String),
    Boson3(BosonBodyToken),
    BosonMacroStatement(String),

    // sugar we build for the std.b3 sugar layer
    Let,
    Return,
    TailCall,
    If,
    Else,
    Unless,
    While,
    Until,
    Do,
    For,
    Repeat,
    Foreach,
    In,
    Forever,
    Loop,
    Break,
    Continue,

    // seperators
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Semicolon,
    Question,
    Dot,
    DoubleColon,
    Arrow,

    // operators
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqualEqual,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    LogicalAnd,
    LogicalOr,
    LogicalNot,
    BitwiseNot,
    PlusPlus,
    MinusMinus,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    ShiftLeftAssign,
    ShiftRightAssign,
    BitwiseAndAssign,
    BitwiseOrAssign,
    BitwiseXorAssign,

    Newline,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(name) => write!(formatter, "identifier `{name}`"),
            Self::IntLiteral(value) => write!(formatter, "Int literal `{value}`"),
            Self::UIntLiteral(value) => write!(formatter, "UInt literal `{value}u`"),
            Self::FloatLiteral(value) => write!(formatter, "Float literal `{value}`"),
            Self::BoolLiteral(value) => write!(formatter, "Bool literal `{value}`"),
            Self::Directive(name) => write!(formatter, "directive `@{name}`"),
            other => formatter.write_str(other.symbol_name()),
        }
    }
}

impl TokenKind {
    /// Returns the name/kind of the symbol expressed as a str
    /// (essentially stringify)
    /// 
    /// Returns the name of the "type" rather than the contents (etc for literals,
    /// we return the "type" rather than the content)
    fn symbol_name(&self) -> &'static str {
        match self {
            Self::Let => "let",
            Self::Return => "return",
            Self::TailCall => "tailcall",
            Self::If => "if",
            Self::Else => "else",
            Self::Unless => "unless",
            Self::While => "while",
            Self::Until => "until",
            Self::Do => "do",
            Self::For => "for",
            Self::Repeat => "repeat",
            Self::Foreach => "foreach",
            Self::In => "in",
            Self::Forever => "forever",
            Self::Loop => "loop",
            Self::Break => "break",
            Self::Continue => "continue",
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::LeftBracket => "[",
            Self::RightBracket => "]",
            Self::Comma => ",",
            Self::Colon => ":",
            Self::Semicolon => ";",
            Self::Question => "?",
            Self::Dot => ".",
            Self::DoubleColon => "::",
            Self::Arrow => "->",
            Self::Assign => "=",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::EqualEqual => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::BitwiseAnd => "&",
            Self::BitwiseOr => "|",
            Self::BitwiseXor => "^",
            Self::LogicalAnd => "&&",
            Self::LogicalOr => "||",
            Self::LogicalNot => "!",
            Self::BitwiseNot => "~",
            Self::PlusPlus => "++",
            Self::MinusMinus => "--",
            Self::PlusAssign => "+=",
            Self::MinusAssign => "-=",
            Self::StarAssign => "*=",
            Self::SlashAssign => "/=",
            Self::PercentAssign => "%=",
            Self::ShiftLeftAssign => "<<=",
            Self::ShiftRightAssign => ">>=",
            Self::BitwiseAndAssign => "&=",
            Self::BitwiseOrAssign => "|=",
            Self::BitwiseXorAssign => "^=",
            Self::Newline => "newline",
            Self::Identifier(_) => "ident",
            Self::IntLiteral(_) => "Int literal",
            Self::UIntLiteral(_) => "UInt literal",
            Self::FloatLiteral(_) => "Float literal",
            Self::BoolLiteral(_) => "Bool literal",
            Self::Directive(_) => "Boson3 @directive",
            Self::Boson3(_) => "legacy boson3 expr",
            Self::BosonMacroStatement(_) => "legacy boson3 macro stmt",
        }
    }
}
