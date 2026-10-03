//! parser.rs
//!
//! This is the parsing component of the `Photon3` sugar layer,
//! responsible for:
//!
//! - taking the full lexed token output
//! - turning it into the AST

use crate::{
    ast::{Module, SourceSpan}, errors::{PhotonError, PhotonErrorKind, PhotonResult}, lexer::{Token, TokenKind},
};

/// The actual parser class itself
struct Parser<'tokens> {
    /// All of the tokens we are parsing
    tokens: &'tokens [Token],

    /// Our current position in the token stream
    ///
    /// We do not use Peekable or something similar since
    /// we may want to do backtracking or other things
    cursor: usize,
}

impl<'tokens> Parser<'tokens> {
    /// Create a new parser over `tokens` that will parse all of the
    /// tokens into a singular `Module` for this file
    pub fn new(tokens: &'tokens [Token]) -> Self {
        Self { tokens, cursor: 0 }
    }

    /// Look at the current token without consuming it.
    fn peek_token(&self) -> Option<&Token> {
        self.tokens.get(self.cursor)
    }

    /// Look ahead at the `offset` token in the token stream without consuming it.
    fn peek_token_nth(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.cursor + offset)
    }

    /// Check if we have reached the end of the token stream.
    fn is_at_end(&self) -> bool {
        self.peek_token()
            .map(|token| matches!(token.value, TokenKind::EoF))
            // If for some reason we don't have an EoF marker (lexer kaboomy?) then we have
            // also hit the EoF i suppose.
            .unwrap_or(true)
    }

    /// Gets the span of the token we just consumed.
    fn previous_span(&self) -> SourceSpan {
        // At the start.. no token possible
        if self.cursor == 0 {
            (0..0).into()
        } else {
            self.tokens[self.cursor - 1].span
        }
    }

    /// Gets the span of the current token we are looking at.
    fn current_span(&self) -> SourceSpan {
        self.peek_token()
            .map(|t| t.span)
            .unwrap_or_else(|| self.previous_span())
    }

    /// Consume the current token and advance the cursor.
    /// Returns an UnexpectedEof error if we try to advance past the end.
    fn advance(&mut self) -> PhotonResult<Token> {
        if self.is_at_end() || self.cursor >= self.tokens.len() {
            return Err(self.unexpected_eof());
        }

        let token = self.tokens[self.cursor].clone();
        self.cursor += 1;
        Ok(token)
    }

    /// Check if the current token matches a specific kind without consuming it.
    fn check(&self, kind: &TokenKind) -> bool {
        if self.is_at_end() {
            return false;
        }

        self.peek_token()
            .map(|token| &token.value == kind)
            .unwrap_or(false)
    }

    /// Returns an `UnexpectedEof` `PhotonResult` Err with the source location of
    /// the previous `Token`'s span
    fn unexpected_eof(&self) -> PhotonError {
        PhotonErrorKind::error(PhotonErrorKind::UnexpectedEndOfFile, self.previous_span())
    }

    /// Consume the current token if it matches `expected_kind`
    /// Otherwise, throw a `ParseError`
    fn expect(&mut self, expected_kind: &TokenKind) -> PhotonResult<Token> {
        // Matches token.. consume and advance
        if self.check(&expected_kind) {
            self.advance()
        } else {
            // Doesnt match! return what we got with the error
            let found = self.peek_token().map(|token| token);

            // If there's no token then we've hit an unexpected EoF
            let Some(found_token) = found else {
                return Err(self.unexpected_eof());
            };

            // There's a token that we didnt expect.
            Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedToken {
                    expected: expected_kind.clone(),
                    found: found_token.value.clone(),
                },
                found_token.span,
            ))
        }
    }

    /// Conditionally advance if the current token matches `kind`.
    ///
    /// Returns Some(token) if it was consumed, None otherwise.
    fn match_token(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.check(&kind) {
            Some(
                self.advance()
                    .expect("check already checked for a token to exist here"),
            )
        } else {
            None
        }
    }

    /// Runs the `Parser` continuously over the source tokens
    /// until EOF is hit or an error occurs, returns all the
    /// the AST in a `Module`
    ///
    /// # Errors
    ///
    /// This may error in many ways!! See `PhotonError`.
    fn parse_module(&mut self) -> PhotonResult<Module> {
        let mut items = Vec::new();

        self.skip_newlines();

        while !self.is_at_end() {
            self.skip_newlines();
        }

        Ok(Module { items })
    }

    /// Repeatedly skips newline tokens until the first non-newline token.
    fn skip_newlines(&mut self) {
        while let Some(_) = self.match_token(&TokenKind::Newline) {}
    }
}
