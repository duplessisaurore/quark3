//! parser.rs
//!
//! This is the parsing component of the `Photon3` sugar layer,
//! responsible for:
//!
//! - taking the full lexed token output
//! - turning it into the AST

use crate::{
    ast::*,
    errors::{
        PhotonError,
        PhotonErrorKind::{self, UnexpectedEndOfFile},
        PhotonResult,
    },
    lexer::{Token, TokenKind},
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
            items.push(self.parse_top_level_item()?);
            self.skip_newlines();
        }

        Ok(Module { items })
    }

    /// Repeatedly skips newline tokens until the first non-newline token.
    fn skip_newlines(&mut self) {
        while let Some(_) = self.match_token(&TokenKind::Newline) {}
    }

    /// Parses a single top level module item (TLI)
    fn parse_top_level_item(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        // We expect a TLI to be here.
        let Some(token) = self.peek_token() else {
            return Err(self.unexpected_eof());
        };

        // All directives match to some TLI
        match token.value {
            TokenKind::Directive(directive_name) => match directive_name.as_str() {
                "namespace" => self.parse_namespace(),
                "requires" => self.parse_requires(),
                "entry" => self.parse_entry(),
                "capability" => self.parse_capability(),
                "global" => self.parse_global(),
                "object" => self.parse_object(),
                "fn" => self.parse_function(),

                _ => Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnknownTLD {
                        name: directive_name.clone(),
                    },
                    self.current_span(),
                )),
            },

            other_token => Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedToken {
                    found: other_token,
                    expected: TokenKind::Directive("any valid top level directive".to_string()),
                },
                self.current_span(),
            )),

            _ => Err(self.unexpected_eof()),
        }
    }

    /// Parses one qualified name, this is a name in the for of
    /// one or more namespaces::item_name
    ///
    /// e.x std::queue::Queue
    fn parse_qualified_name(&mut self) -> PhotonResult<QualifiedName> {
        let mut parts = Vec::new();

        // Each part should be a valid identifier
        parts.push(self.expect_identifier()?);

        while let Some(_) = self.match_token(&TokenKind::DoubleColon) {
            parts.push(self.expect_identifier()?);
        }

        Ok(QualifiedName::new(parts))
    }

    /// Expects an identifier to exist at the current position, erroring
    /// otherwise
    ///
    /// Returns the underlying string the identifier occupies
    fn expect_identifier(&mut self) -> PhotonResult<String> {
        let token = self.advance()?;

        if let TokenKind::Identifier(text) = token.value {
            Ok(text)
        } else {
            Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedToken {
                    expected: TokenKind::Identifier(String::from("<identifier>")),
                    found: token.value,
                },
                token.span,
            ))
        }
    }

    /// Parse a type at the current position
    fn parse_type(&mut self) -> PhotonResult<TypeName> {
        let name = self.parse_qualified_name()?;

        Ok(TypeName::from_qualified_name(name))
    }

    /// Parse a parameter at the current position, this is some
    /// ident: type
    fn parse_parameter(&mut self) -> PhotonResult<Parameter> {
        // ident
        let name = self.expect_identifier()?;

        self.expect(&TokenKind::Colon)?;

        // type
        let declared_type = self.parse_type()?;

        Ok(Parameter {
            name,
            declared_type,
        })
    }

    /// Parses an entire list of parameters etc.
    ///
    /// (param, param, param)
    ///
    /// outputs it as a set of params in a Vec's
    fn parse_parameter_list(&mut self) -> PhotonResult<Vec<Parameter>> {
        // (
        self.expect(&TokenKind::LeftParen)?;

        let mut parameters = Vec::new();

        // No parameters since it ends with )
        if let Some(_) = self.match_token(&TokenKind::RightParen) {
            return Ok(parameters);
        }

        loop {
            // Not a direct end of right paren, parse params
            parameters.push(self.parse_parameter()?);

            // Another parameter
            if let Some(_) = self.match_token(&TokenKind::Comma) {
                if self.check(&TokenKind::RightParen) {
                    // Premature end
                    return Err(PhotonErrorKind::error(
                        PhotonErrorKind::UnexpectedEndOfParamsFollowingComma,
                        self.current_span(),
                    ));
                }

                continue;
            }

            break;
        }

        self.expect(&TokenKind::RightParen)?;

        Ok(parameters)
    }

    /// Parses an entire list of arguments etc.
    ///
    /// (expr, expr, expr)
    ///
    /// outputs it as a set of exprs in a Vec's
    fn parse_argument_list(
        &mut self,
    ) -> PhotonResult<Vec<Located<Expression>>> {
        // (
        self.expect(&TokenKind::LeftParen)?;

        let mut arguments = Vec::new();

        // No parameters since it ends with )
        if let Some(_) = self.match_token(&TokenKind::RightParen) {
            return Ok(arguments);
        }

        loop {
            arguments.push(self.parse_expression()?);

            // Another argument
            if let Some(_) = self.match_token(&TokenKind::Comma) {
                if self.check(&TokenKind::RightParen) {
                    // Premature end
                    return Err(PhotonErrorKind::error(
                        PhotonErrorKind::UnexpectedEndOfArgsFollowingComma,
                        self.current_span(),
                    ));
                }

                continue;
            }

            break;
        }

        self.expect(&TokenKind::RightParen)?;

        Ok(arguments)
    }
}
