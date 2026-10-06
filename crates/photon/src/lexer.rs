//! lexer.rs
//!
//! This is the lexing component of the `Photon3` sugar layer,
//! responsible for:
//!
//! - tokenising the content
//! - handling raw blocks of boson3
//! - handling numeric literals

use std::{fmt, str::Chars};

use crate::{
    ast::{Located, SourceSpan},
    errors::{PhotonErrorKind, PhotonResult},
};

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

    // sugar we build for the std.b3 sugar layer
    Let,
    Return,
    If,
    Else,
    While,
    Do,
    For,
    Foreach,
    In,
    Loop,
    As,
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
    EoF,
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
    expression_grouping_depth: usize,
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
    /// This advances the cursor, if the character
    /// was a newline then this sets the cursor at
    /// start of line state to true
    fn advance(&mut self) -> Option<char> {
        let consumed_char = self.chars.next();

        // Update start of line status
        if consumed_char.is_some_and(|char| char == '\n') {
            self.cursor_at_start_of_line = true;
        } else if consumed_char.is_some_and(|char| !char.is_whitespace()) {
            self.cursor_at_start_of_line = false;
        }

        consumed_char
    }

    /// Fully tokenises the source input into a set of `Token`'s
    ///
    /// # Errors
    ///
    /// This may error in many ways!! See `PhotonErrorKind`, generally
    /// if things are unterminated or if there's an invalid literal
    /// or with some garbage on the end of the number
    pub fn tokenize(mut self) -> PhotonResult<Vec<Token>> {
        // While there are still characters..
        while self.peek_char().is_some() {
            self.skip_whitespace_comments();

            if self.try_lex_legacy_boson_expression()? {
                continue;
            }

            // Try lex the number
            if self.peek_char().is_some_and(|c| c.is_ascii_digit())
                || (self.peek_char() == Some('.')
                    && self.peek_char_nth(1).is_some_and(|d| d.is_ascii_digit()))
            {
                self.lex_number()?;
                continue;
            }

            if self.peek_char().is_some_and(is_valid_ident_char) {
                self.lex_identifier_or_keyword();
                continue;
            }

            if self.try_advance_str("@") {
                self.lex_directive()?;
                continue;
            }

            self.lex_symbol()?;
        }

        self.push_token(TokenKind::EoF, self.current_pos());
        Ok(self.tokens)
    }

    /// Push a token to the output of the lexer phase
    /// this assumes this token ends at the current position the
    /// cursor is in
    fn push_token(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Located::new(kind, self.span_from(start)));
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

            if remaining.ends_with("\n") {
                // We are now at the start of line.
                self.cursor_at_start_of_line = true;
            } else if !remaining.chars().all(char::is_whitespace) {
                // We hit a normal character (non-whitespace), so we are no longer at start of line.
                self.cursor_at_start_of_line = false;
            }

            true
        } else {
            false
        }
    }

    /// Skips all the whitespace until the first non-whitespace character
    fn skip_whitespace_comments(&mut self) {
        loop {
            match self.peek_char() {
                Some(_) if self.try_lex_newline() => {}
                Some(c) if c.is_whitespace() && c != '\n' => {
                    self.advance();
                }
                Some('/') if self.peek_char_nth(1) == Some('/') => {
                    // Advance until the next new line
                    while let Some(c) = self.peek_char() {
                        if c == '\n' {
                            break;
                        }
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    /// Tries to lex a newline character at this position.
    ///
    /// Returns whether or not a newline could be lexed.
    fn try_lex_newline(&mut self) -> bool {
        let start = self.current_pos();
        if !self.try_advance_str("\n") {
            return false;
        }

        // And if we arent in a grouping-type expression, then push newline
        if self.expression_grouping_depth == 0 {
            self.push_token(TokenKind::Newline, start);
        }

        true
    }

    /// Tries to lex a legacy boson expression,
    ///
    /// This is in the form of `b3<Type> { ... }`
    ///
    /// This returns whether or not the the legacy boson expression could be lexxed.
    fn try_lex_legacy_boson_expression(&mut self) -> PhotonResult<bool> {
        // Make a starting point for the chars, to test if this is actually a b3 expression
        // else we may be trying to compare a variable called "b3" with something else
        let boson_checkpoint = self.clone_chars();
        let start_position = self.current_pos();

        // Try parse a boson expression here now.
        let could_parse_boson = {
            if !self.try_advance_str("b3") {
                return Ok(false);
            }

            self.skip_whitespace_comments();

            // must be followed by a <
            if !self.try_advance_str("<") {
                return Ok(false);
            };

            self.skip_whitespace_comments();

            // now we parse the type, types cannot have spaces

            let type_string = self.consume_while(is_valid_ident_char);

            // No type was found to be here.
            if type_string.is_empty() {
                return Ok(false);
            }

            // It must then be followed by the ">"
            self.skip_whitespace_comments();
            if !self.try_advance_str(">") {
                return Ok(false);
            }

            self.skip_whitespace_comments();

            // Must now start with a {
            if !self.try_advance_str("{") {
                return Ok(false);
            }

            // Grab the body including the ending brace
            let after_brace = self.current_pos();
            let body = self.try_lex_legacy_boson_body().ok_or_else(|| {
                PhotonErrorKind::error(
                    PhotonErrorKind::UnterminatedLegacyBoson3Block,
                    (start_position..after_brace).into(),
                )
            })?;

            // This is then our legacy boson token
            let token = BosonBodyToken {
                declared_type: type_string,
                body,
                body_span: self.span_from(after_brace),
            };

            self.push_token(TokenKind::Boson3(token), start_position);
            Ok(true)
        };

        // essentially it didn't actually match a boson3 legacy expression
        if could_parse_boson
            .as_ref()
            .is_ok_and(|matches_boson_expression| !*matches_boson_expression)
        {
            self.chars = boson_checkpoint;
        }

        could_parse_boson
    }

    /// Attempts to fully lex a legacy boson3 expression's
    /// body, returns the body as a String if it could, else None.
    fn try_lex_legacy_boson_body(&mut self) -> Option<String> {
        // Restore point for the cursor
        let restore_point = self.clone_chars();
        let start_point = self.current_pos();

        // Current bracket depth
        let mut depth = 1usize;

        while let Some(char) = self.advance() {
            if self.try_advance_str("@string") {
                // advance until next line
                while let Some(string_char) = self.advance()
                    && string_char != '\n'
                {}
                continue;
            }

            // photon3 strings arent supported, so a quote in a boson3 block is not really anything special

            match char {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;

                    // Found matching brace exit
                    if depth == 0 {
                        return Some(self.source[start_point..self.current_pos() - 1].to_string());
                    }
                }
                _ => {}
            }

            self.skip_whitespace_comments();
        }

        self.chars = restore_point;
        None
    }

    /// Attempt to tokenise a numeric literal
    ///
    /// A number is where the current char is either an ASCII digit,
    /// or `.` followed by an ASCII digit.
    fn lex_number(&mut self) -> PhotonResult<()> {
        // Start of this number
        let start = self.current_pos();

        // If it begins with 0 followed by one of the base
        // "specifiers" then try lex that base
        if self.peek_char() == Some('0') {
            match self.peek_char_nth(1) {
                Some('x' | 'X') => return self.lex_base_int(2, 16),
                Some('b' | 'B') => return self.lex_base_int(2, 2),
                Some('o' | 'O') => return self.lex_base_int(2, 8),
                _ => {}
            }
        }

        // The text of the literal
        // we build this from here while we figure out if its
        // a float, UInt or Int for later parsing
        let mut text = String::new();
        let mut is_float = false;

        // Starts with "." so must be a float
        // such as: `.5`
        if self.peek_char() == Some('.') {
            is_float = true;
            text.push('.');
            self.advance();
            self.consume_base_10_digits(&mut text);
        } else {
            // No start with a "." so we check and consume the integer part
            self.consume_base_10_digits(&mut text);

            // Check whether a fractional part follows with a "." for a float
            if self.peek_char() == Some('.') && self.peek_char_nth(1) != Some('.') {
                is_float = true;
                text.push('.');
                self.advance();
                self.consume_base_10_digits(&mut text);
            }
        }

        // Exponent for a float
        if matches!(self.peek_char(), Some('e' | 'E')) {
            // Make a checkpoint here to grab all of exponent chars
            // if they exist (this is so we can revert the lexer Chars<>
            // if its not actually a valid exponent to let trailing garbage handle
            // the error)
            let exp_checkpoint = self.clone_chars();
            let mut exp_text = String::new();
            exp_text.push(
                self.advance()
                    .expect("expected 'e'/'E' to be here since peek_char returned"),
            );

            if matches!(self.peek_char(), Some('+' | '-')) {
                exp_text.push(
                    self.advance()
                        .expect("expected '+'/'-' to be here since peek_char returned it"),
                );
            }

            // Consume all of the exponent digits
            if self.peek_char().is_some_and(is_char_valid_base_10) {
                self.consume_base_10_digits(&mut exp_text);
                is_float = true;
                text.push_str(&exp_text);
            } else {
                // Not actually an exponent, let trailing
                // garbage handle error later.
                self.chars = exp_checkpoint;
            }
        }

        // Check for `UInt` suffix of `u`/`U` suffix
        // same case with the exponents and trailing garbage
        let has_u_suffix = self.peek_is_valid_uint_suffix();

        if has_u_suffix {
            // floats cant be unsigned...
            if is_float {
                let bad_start = self.current_pos();
                self.advance();
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::MalformedNumber {
                        reason: String::from("float literals cannot have a `u` suffix"),
                    },
                    self.span_from(bad_start),
                ));
            }
            self.advance();
        }

        // Prevent ident gluing onto the numeric literal when unexpected
        self.check_trailing_garbage(start, "invalid trailing characters after numeric literal")?;

        // Produce the actual token from the text we built up while advancing
        let token = if has_u_suffix {
            TokenKind::UIntLiteral(text.parse().unwrap_or(0))
        } else if is_float {
            TokenKind::FloatLiteral(text.parse().unwrap_or(f64::NAN))
        } else {
            TokenKind::IntLiteral(text.parse().unwrap_or(0))
        };

        self.push_token(token, start);

        Ok(())
    }

    /// Consumes a run of base-10 digits at the current position,
    /// appending each digit to `buf`
    ///
    /// `_` is accepted as a visual separator (e.g. `1_000_000`)
    /// but is not part of the final `buf`.
    fn consume_base_10_digits(&mut self, buf: &mut String) {
        while let Some(c) = self.peek_char() {
            // ignore `_``
            if c == '_' {
                self.advance();

            // push other valid base 10 digits
            // `_` is eaten by above
            } else if is_char_valid_base_10(c) {
                buf.push(c);
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Lexes some `Int` or `UInt` at the current position
    ///
    /// `prefix_len` is the number of elements to skip in the prefix as part
    /// of this base, for example `0x` has 2 elements in the prefix before
    /// the actual `Int`/`UInt` starts.
    ///
    /// `base` is the base of the integer.
    fn lex_base_int(&mut self, prefix_len: usize, base: u32) -> PhotonResult<()> {
        // Grab the current start position for errors that start from here
        let start = self.current_pos();

        // Ignore the prefix
        for _ in 0..prefix_len {
            self.advance();
        }

        // The position at which digits start..
        let digits_start = self.current_pos();

        // Yoink all the digits part of the int. `_` is allowed as a
        // visual separator for long Ints/UInts and is dropped here
        // rather than collected, so `digits` is parse-ready as-is.
        let mut digits = String::new();
        while let Some(c) = self.peek_char() {
            if c == '_' {
                self.advance();
            } else if c.is_digit(base) {
                digits.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // No digits since current = start for digits, which is illegal!
        if self.current_pos() == digits_start {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::MalformedNumber {
                    reason: String::from(
                        "expected digits after base prefix such as: `0x`, `0b`, `0o`",
                    ),
                },
                self.span_from(start),
            ));
        }

        // Consume `UInt` suffix if it has one
        let has_uint_suffix = self.peek_is_valid_uint_suffix();
        if has_uint_suffix {
            self.advance();
        }

        // Ensure there's no trailing garbage to prevent accidental ident gluing
        self.check_trailing_garbage(start, "invalid trailing characters after radix literal")?;

        if has_uint_suffix {
            let value = u64::from_str_radix(&digits, base).unwrap_or(0);
            self.push_token(TokenKind::UIntLiteral(value), start);
        } else {
            let value = i64::from_str_radix(&digits, base).unwrap_or(0);
            self.push_token(TokenKind::IntLiteral(value), start);
        }

        Ok(())
    }

    /// Returns whether or not the next character is a valid unsigned
    /// int suffix for an integer (basically is the next `u` or `U`)
    fn peek_is_valid_uint_suffix(&self) -> bool {
        matches!(self.peek_char(), Some('u' | 'U'))
    }

    /// Check that there is no trailing garbage (non-reserved/whitespace)
    /// following this number
    ///
    /// Returns Ok(()) if there is none else a `PhotonError`.
    fn check_trailing_garbage(&mut self, start: usize, reason: &'static str) -> PhotonResult<()> {
        if self
            .peek_char()
            .is_some_and(|c| !c.is_whitespace() && !is_reserved(c))
        {
            // Consume the rest of the unspaced characters so the error span is perfectly sized
            while self
                .peek_char()
                .is_some_and(|c| !c.is_whitespace() && !is_reserved(c))
            {
                self.advance();
            }

            return Err(PhotonErrorKind::error(
                PhotonErrorKind::MalformedNumber {
                    reason: String::from(reason),
                },
                self.span_from(start),
            ));
        }

        Ok(())
    }

    /// Lexes an identifier or a keyword at the current position.
    fn lex_identifier_or_keyword(&mut self) {
        let start = self.current_pos();

        // grab all ident char at this position
        let text = self.consume_while(is_valid_ident_char);

        let token = match text.as_str() {
            "true" => TokenKind::BoolLiteral(true),
            "false" => TokenKind::BoolLiteral(false),
            "let" => TokenKind::Let,
            "return" => TokenKind::Return,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "do" => TokenKind::Do,
            "for" => TokenKind::For,
            "foreach" => TokenKind::Foreach,
            "in" => TokenKind::In,
            "loop" => TokenKind::Loop,
            "as" => TokenKind::As,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            _ => TokenKind::Identifier(text),
        };

        self.push_token(token, start);
    }

    /// Attempts to lex a symbol at the current position
    fn lex_symbol(&mut self) -> PhotonResult<()> {
        let start = self.current_pos();

        // These are candidates which are in order of priority,
        // and are "multi-char" candidates, so we need to take and
        // match these before single char else <<= would be three seperate things
        let candidates: &[(&str, TokenKind)] = &[
            ("<<=", TokenKind::ShiftLeftAssign),
            (">>=", TokenKind::ShiftRightAssign),
            ("::", TokenKind::DoubleColon),
            ("->", TokenKind::Arrow),
            ("==", TokenKind::EqualEqual),
            ("!=", TokenKind::NotEqual),
            ("<=", TokenKind::LessEqual),
            (">=", TokenKind::GreaterEqual),
            ("<<", TokenKind::ShiftLeft),
            (">>", TokenKind::ShiftRight),
            ("&&", TokenKind::LogicalAnd),
            ("||", TokenKind::LogicalOr),
            ("++", TokenKind::PlusPlus),
            ("--", TokenKind::MinusMinus),
            ("+=", TokenKind::PlusAssign),
            ("-=", TokenKind::MinusAssign),
            ("*=", TokenKind::StarAssign),
            ("/=", TokenKind::SlashAssign),
            ("%=", TokenKind::PercentAssign),
            ("&=", TokenKind::BitwiseAndAssign),
            ("|=", TokenKind::BitwiseOrAssign),
            ("^=", TokenKind::BitwiseXorAssign),
        ];

        // Try match the corresponding candidate
        for (text, kind) in candidates {
            if self.try_advance_str(text) {
                self.push_token(kind.clone(), start);
                return Ok(());
            }
        }

        // Match single character symbols
        let Some(character) = self.advance() else {
            return Ok(());
        };

        let kind = match character {
            '(' => {
                self.expression_grouping_depth += 1;
                TokenKind::LeftParen
            }
            ')' => {
                self.expression_grouping_depth = self.expression_grouping_depth.saturating_sub(1);
                TokenKind::RightParen
            }
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => {
                self.expression_grouping_depth += 1;
                TokenKind::LeftBracket
            }
            ']' => {
                self.expression_grouping_depth = self.expression_grouping_depth.saturating_sub(1);
                TokenKind::RightBracket
            }
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            '?' => TokenKind::Question,
            '.' => TokenKind::Dot,
            '=' => TokenKind::Assign,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '<' => TokenKind::Less,
            '>' => TokenKind::Greater,
            '&' => TokenKind::BitwiseAnd,
            '|' => TokenKind::BitwiseOr,
            '^' => TokenKind::BitwiseXor,
            '!' => TokenKind::LogicalNot,
            '~' => TokenKind::BitwiseNot,
            _ => {
                // this should be a symbol!
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnexpectedCharacter { character },
                    self.span_from(start),
                ));
            }
        };

        self.push_token(kind, start);
        Ok(())
    }

    /// Lexes a directive at the current position
    fn lex_directive(&mut self) -> PhotonResult<()> {
        let start = self.current_pos();

        // All directives are some kind of identifier
        if !self.peek_char().is_some_and(is_valid_ident_char) {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::MissingDirectiveName,
                self.span_from(start),
            ));
        }

        // Grab the name of this directive
        let name = self.consume_while(is_valid_ident_char);

        self.push_token(TokenKind::Directive(name), start);
        Ok(())
    }

    /// Consumes tokens which match predicate together into a String
    /// does not overreach
    fn consume_while(
        &mut self,
        mut predicate: impl FnMut(char) -> bool,
    ) -> String {
        let mut text = String::new();

        while let Some(c) = self.peek_char() {
            if !predicate(c) {
                break;
            }

            text.push(c);
            self.advance();
        }

        text
    }
}

/// Returns whether or not the character supplied
/// is a valid character for an identifier.
///
/// This is essentially the ascii alphanumeric numbers
fn is_valid_ident_char(character: char) -> bool {
    (character.is_ascii_alphanumeric() || character == '_' || character == ':')
        && !is_reserved(character)
}

/// Returns whether or not a character is a valid part of a digit
/// in base 10 (including the `_` separator).
fn is_char_valid_base_10(c: char) -> bool {
    is_char_valid_under_base(c, 10)
}

/// Returns whether or not a character is a valid part of a digit
/// under the provided base. `_` is always accepted as a visual
/// separator between digits for long numbers, e.g. `1_000_000`.
///
/// NOTE: If supporting any new bases beyond 16, this will need updating.
fn is_char_valid_under_base(c: char, base: u32) -> bool {
    c.is_digit(base) || c == '_'
}

/// These are reserved symbols which can never end up in an identifer
/// even if its a keyword/symbolic/normal.
///
/// This is because if they could.. it'd probably make the codebase
/// more confusing and harder to parse from a scan of the eyes.
fn is_reserved(c: char) -> bool {
    matches!(
        c,
        '(' | ')'
            | '{'
            | '}'
            | '['
            | ']'
            | ','
            | ';'
            | ':'
            | '"'
            | '`'
            | '@'
            | '#'
            | '.'
            | '='
            | '<'
            | '>'
            | '$'
            | '+'
            | '-'
            | '*'
            | '/'
            | '!'
            | '&'
            | '|'
            | '%'
            | '^'
            | '~'
    )
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
            Self::If => "if",
            Self::Else => "else",
            Self::While => "while",
            Self::Do => "do",
            Self::For => "for",
            Self::Foreach => "foreach",
            Self::In => "in",
            Self::Loop => "loop",
            Self::As => "as",
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
            Self::EoF => "end of file",
            Self::Newline => "newline",
            Self::Identifier(_) => "ident",
            Self::IntLiteral(_) => "Int literal",
            Self::UIntLiteral(_) => "UInt literal",
            Self::FloatLiteral(_) => "Float literal",
            Self::BoolLiteral(_) => "Bool literal",
            Self::Directive(_) => "Boson3 @directive",
            Self::Boson3(_) => "legacy boson3 expr",
        }
    }
}