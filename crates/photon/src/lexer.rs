//! lexer.rs
//!
//! This is the lexing component of the `Photon3` sugar layer,
//! responsible for:
//! 
//! - tokenising the content
//! - handling raw blocks of boson3
//! - handling numeric literals

use std::{fmt, str::Chars};

use chumsky::span::Span;

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

/// The actual `Lexer` struct,
/// this is responsible for tokenising the `source` into a set
/// of `Tokens`
pub struct Lexer<'src> {
    /// The input source file
    source: &'src str,

    /// Our cursor over the source file
    chars: Chars<'src>,

    /// The output set of tokens produced from this lexing process.
    tokens: Vec<Token>,

    /// Whether or not we are at the beginning of a line, this
    /// is important for the `BosonMacroStatement` symbol kind,
    /// which should only be parsed if the line starts with a legacy 
    /// macro invocation to prevent unary not from exploding
    cursor_at_start_of_line: bool,

    /// The current depth of expression grouping, this would be
    /// the depth of ()'s and []'s, since we dont want to lex newlines
    /// as a distinct newline, but rather as normal whitespace.
    expression_grouping_depth: usize
}

impl<'src> Lexer<'src> {
    /// Creates a new `Lexer` that will lex all of the contents
    /// of the source file into tokens. 
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            chars: source.chars(),
            cursor_at_start_of_line: true,
            expression_grouping_depth: 0,
            tokens: Vec::new(),
        }
    }

    /// Returns the current byte offset in the source
    fn current_pos(&self) -> usize {
        self.source.len() - self.chars.as_str().len()
    }

    /// Span from `start` to the current position.
    fn span_from(&self, start: usize) -> SourceSpan {
        (start..self.current_pos()).into()
    }

    /// Clone the Chars iterator over the string
    /// (this is essentially free!)
    fn clone_chars(&self) -> Chars<'src> {
        // This is a slice::Iter
        // cloning this does NOT clone each element, instead
        // since a slice is more like a pointer into the source
        // we are kind of just cloning that pointer, so its basically
        // free
        self.chars.clone()
    }

    /// Peek the current char without consuming it.
    fn peek_char(&self) -> Option<char> {
        // By cloning the iterator, we advance the cloned one
        // instead of the actual one
        self.clone_chars().next()
    }

    /// Peek the `nth` char after the current one without consuming anything.
    fn peek_char_nth(&self, nth: usize) -> Option<char> {
        self.clone_chars().nth(nth)
    }

    /// Consume and return the current char
    ///
    /// This advances the cursor
    fn advance(&mut self) -> Option<char> {
        self.chars.next()
    }
    
    /// Push a token to the output of the lexer phase
    /// this assumes this token ends at the current position the
    /// cursor is in
    fn push_token(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(
            Located::new(kind, self.span_from(start))
        );
    }

    /// If the unconsumed text starts with `s`,
    /// consume it and return true.
    ///
    /// Else returns false and does not consume.
    ///
    /// Essentially tries to advance the cursor
    /// by some &str and returns the result as true/false
    fn try_advance_str(&mut self, s: &str) -> bool {
        let from_current = self.chars.as_str();
        if let Some(remaining) = from_current.strip_prefix(s) {
            // Fast forward the iterator by slicing the remaining string
            // and re-charsing it.
            //
            // This is why we cant use CharsIndices because the indices would
            // come from the new slice instead of the source, but chars lets
            // us do it yippie!!!
            self.chars = remaining.chars();
            true
        } else {
            false
        }
    }

    // Each lexer function should be something like this:
    // 
    // try lex <kind>, returns a bool that represents whether or not
    // we lexed that kind and should continue lexer loop, or continue
    // with trying other kinds.

    /// Tries to lex a newline character at this position.
    fn try_lex_newline(&mut self) -> bool {
        let start = self.current_pos();
        if !self.try_advance_str("\n") {
            return false;
        }

        // We are now at the start of line.
        self.cursor_at_start_of_line = true;

        // And if we arent in a grouping-type expression, then push newline
        if self.expression_grouping_depth == 0 {
            self.push_token(TokenKind::Newline, start);
        }

        true
    }
}