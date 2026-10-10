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
        PhotonErrorKind::{self},
        PhotonResult,
    },
    lexer::{BosonBodyToken, Token, TokenKind},
};

/// The actual parser class itself
pub struct Parser<'tokens> {
    /// Original source file name
    source_file_name: String,

    /// All of the tokens we are parsing
    tokens: &'tokens [Token],

    /// Our current position in the token stream
    ///
    /// We do not use Peekable or something similar since
    /// we may want to do backtracking or other things
    cursor: usize,

    /// The type parameters that are currently in scope.
    ///
    /// This is used for matching against type names during type resolution,
    /// as a direct match of a type name like T to a type param in scope
    /// should be that generic parameter instead of Object(T)
    type_parameters_in_scope: Vec<String>,
}

impl<'tokens> Parser<'tokens> {
    /// Create a new parser over `tokens` that will parse all of the
    /// tokens into a singular `Module` for this file
    pub fn new(source_file_name: String, tokens: &'tokens [Token]) -> Self {
        Self {
            source_file_name,
            tokens,
            cursor: 0,
            type_parameters_in_scope: Vec::new(),
        }
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

    /// Check if the token `offset` ahead matches `kind`.
    fn check_nth(&self, offset: usize, kind: &TokenKind) -> bool {
        self.peek_token_nth(offset)
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
        if self.check(expected_kind) {
            self.advance()
        } else {
            // Doesnt match! return what we got with the error
            let found = self.peek_token();

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
        if self.check(kind) {
            Some(
                self.advance()
                    .expect("check already checked for a token to exist here"),
            )
        } else {
            None
        }
    }

    /// Conditionally advance if the current token matches `kind`.
    ///
    /// Returns true if it was consumed, else false
    fn eat(&mut self, kind: &TokenKind) -> bool {
        self.match_token(kind).is_some()
    }

    /// Runs the `Parser` continuously over the source tokens
    /// until EOF is hit or an error occurs, returns all the
    /// the AST in a `Module`
    ///
    /// # Errors
    ///
    /// This may error in many ways!! See `PhotonError`, generally
    /// if tokens dont match up to the actual grammatical constructs
    pub fn parse_module(&mut self) -> PhotonResult<Module> {
        let mut items = Vec::new();

        self.skip_newlines();

        // Validate namespace declaration for this module.
        let mut namespace_declared: Option<_> = None;

        while !self.is_at_end() {
            let item = self.parse_top_level_item()?;

            // Check item for potential conflicts/duplicates
            match &item {
                Located {
                    value: TopLevelItem::Namespace(qualified_namespace),
                    ..
                } => {
                    // duplicate namespace
                    if namespace_declared.is_some() {
                        return Err(PhotonErrorKind::error(
                            PhotonErrorKind::DuplicateNamespace,
                            self.previous_span(),
                        ));
                    }

                    namespace_declared = Some(qualified_namespace.clone());
                }
                _ => {}
            };

            items.push(item);
            self.skip_newlines();
        }

        // extract namespace decl
        let Some(module_namespace) = namespace_declared else {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::NoNamespace,
                self.previous_span(),
            ));
        };

        Ok(Module {
            items,
            namespace: module_namespace,
            source_file: self.source_file_name.clone(),
        })
    }

    /// Repeatedly skips newline tokens until the first non-newline token.
    fn skip_newlines(&mut self) {
        while self.eat(&TokenKind::Newline) {}
    }

    /// Parses a single top level module item (TLI)
    fn parse_top_level_item(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        // We expect a TLI to be here.
        let Some(token) = self.peek_token() else {
            return Err(self.unexpected_eof());
        };

        // All directives match to some TLI
        match &token.value {
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
                    found: other_token.clone(),
                    expected: TokenKind::Directive("any valid top level directive".to_string()),
                },
                self.current_span(),
            )),
        }
    }

    /// Expects a directive with a certain `expected` directive type in this current
    /// position
    ///
    /// Returns the span that this directive component is at
    fn expect_directive(&mut self, expected: &str) -> PhotonResult<SourceSpan> {
        match self.advance() {
            // Directive match
            Ok(Token {
                value: TokenKind::Directive(name),
                span,
            }) if name == expected => Ok(span),

            // Is directive, no match
            Ok(Token {
                value: TokenKind::Directive(name),
                span,
            }) => Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedDirective {
                    found: name,
                    expected: expected.to_string(),
                },
                span,
            )),

            // Is not directive, no match
            Ok(Token {
                value: actual,
                span,
            }) => Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedToken {
                    found: actual,
                    expected: TokenKind::Directive(expected.to_string()),
                },
                span,
            )),

            Err(error) => Err(error),
        }
    }

    /// Parses one namespace delcaration at the current position
    fn parse_namespace(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("namespace")?.start;

        // Name of the modules namespace
        let name = self.parse_qualified_name()?;
        let end = self.previous_span().end;

        let item = Located::new(TopLevelItem::Namespace(name), (start..end).into());

        // Newline must follow TLI
        self.require_newline()?;

        Ok(item)
    }

    /// Parse one requires declaration at the current possition
    fn parse_requires(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("requires")?.start;

        // Name of the module we require
        let name = self.parse_qualified_name()?;
        let end = self.previous_span().end;

        let item = Located::new(TopLevelItem::Requires(name), (start..end).into());

        self.require_newline()?;

        Ok(item)
    }

    /// Parse one entry declaration at the current possition
    fn parse_entry(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("entry")?.start;

        // Name of the entry function
        let name = self.expect_identifier()?;
        let end = self.previous_span().end;

        let item = Located::new(TopLevelItem::Entry(name), (start..end).into());

        self.require_newline()?;

        Ok(item)
    }

    /// Parse one capability declaration at the current possition
    fn parse_capability(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("capability")?.start;

        // Name of the capability binding in the local module
        let name = self.expect_identifier()?;

        // The number following, this should be the actual value
        let number = match self.advance() {
            Ok(Token {
                value: TokenKind::IntLiteral(value),
                ..
            }) if value >= 0 => value as u64,

            Ok(Token {
                value: TokenKind::UIntLiteral(value),
                ..
            }) => value,

            Ok(Token {
                value: actual,
                span,
            }) => {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::InvalidCapabilityNumber { found: actual },
                    span,
                ));
            }

            Err(error) => Err(error)?,
        };

        let end = self.previous_span().end;

        let item = Located::new(
            TopLevelItem::Capability(CapabilityDeclaration { name, number }),
            (start..end).into(),
        );

        self.require_newline()?;

        Ok(item)
    }

    /// Parse one global declaration at the current position
    fn parse_global(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("global")?.start;

        // optional constant declaration
        let is_const = self.eat(&TokenKind::Const);

        // global name
        let name = self.expect_identifier()?;

        self.expect(&TokenKind::Colon)?;

        // global type
        let declared_type = self.parse_type()?;

        // optional binding
        let initialiser = if self.eat(&TokenKind::Assign) {
            Some(self.parse_expression()?)
        } else {
            None
        };

        let end = self.previous_span().end;

        let item = Located::new(
            TopLevelItem::Global(GlobalDeclaration {
                name,
                declared_type,
                is_const,
                initialiser,
            }),
            (start..end).into(),
        );

        self.require_newline()?;

        Ok(item)
    }

    /// Parse one object declaration at the current position
    fn parse_object(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("object")?.start;

        // object name
        let name = self.expect_identifier()?;

        // optional type parameters
        let type_parameters = self.parse_type_parameter_list()?;

        // all the parmaeters of the object, these can refer to the type parameters
        let fields =
            self.with_type_parameters(&type_parameters, |parser| parser.parse_parameter_list())?;

        let end = self.previous_span().end;

        let item = Located::new(
            TopLevelItem::Object(ObjectDeclaration {
                name,
                type_parameters,
                fields,
            }),
            (start..end).into(),
        );

        self.require_newline()?;

        Ok(item)
    }

    /// Parse one function declaration at the current position
    fn parse_function(&mut self) -> PhotonResult<Located<TopLevelItem>> {
        let start = self.expect_directive("fn")?.start;

        // function name
        let name = self.expect_identifier()?;
        // optional type parameters to the function
        let type_parameters = self.parse_type_parameter_list()?;

        // parameters, return type, signature etc. can refer to type params
        let (parameters, return_type, signature_end, body) =
            self.with_type_parameters(&type_parameters, |parser| {
                // function params
                let parameters = parser.parse_parameter_list()?;

                // function ret type
                parser.expect(&TokenKind::Arrow)?;
                let return_type = parser.parse_type()?;

                let signature_end = parser.previous_span().end;

                parser.require_newline()?;

                let mut body = Vec::new();

                // body of the function
                // essentially we parse it as statements without considering directive lines
                while !parser.is_at_end()
                    && !matches!(
                        parser.peek_token(),
                        Some(Token {
                            value: TokenKind::Directive(_),
                            ..
                        })
                    )
                {
                    body.push(parser.parse_statement()?);
                }

                Ok((parameters, return_type, signature_end, body))
            })?;

        // After this function declaration
        let end = body
            .last()
            .map(|statement| statement.span.end)
            .unwrap_or(signature_end);

        Ok(Located::new(
            TopLevelItem::Function(FunctionDeclaration {
                name,
                type_parameters,
                parameters,
                return_type,
                body,
            }),
            (start..end).into(),
        ))
    }

    /// Forces a newline to exist at this position and advances until the next line with content
    fn require_newline(&mut self) -> PhotonResult<()> {
        let next_token = self.advance()?;

        // Next token should be a newline
        match next_token {
            Token {
                value: TokenKind::Newline,
                ..
            } => {}
            Token { value, span } => Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedToken {
                    found: value,
                    expected: TokenKind::Newline,
                },
                span,
            ))?,
        }

        self.skip_newlines();

        Ok(())
    }

    /// Parses one qualified name, this is a name in the for of
    /// one or more namespaces::item_name
    ///
    /// e.x std::queue::Queue
    fn parse_qualified_name(&mut self) -> PhotonResult<QualifiedName> {
        let mut parts = Vec::new();

        // Each part should be a valid identifier
        parts.push(self.expect_identifier()?);

        // `::` followed by `[` is the start of explicit type arguments
        // instead of another segment
        while self.check(&TokenKind::DoubleColon) && !self.check_nth(1, &TokenKind::LeftBracket) {
            self.advance()?;
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

    /// Parses a comma separated list of at least one item
    ///
    /// Each item is parsed using the `parse_item` function, which should
    /// return a result of that type T.
    ///
    /// The closing delimiter of the list is `close`, and the error to provide
    /// if ther eis a trailing comma, etc blah, blah, `close` is `trailing_comma_error`
    fn parse_delimited_list<T>(
        &mut self,
        close: &TokenKind,
        trailing_comma_error: PhotonErrorKind,
        mut parse_item: impl FnMut(&mut Self) -> PhotonResult<T>,
    ) -> PhotonResult<Vec<T>> {
        // Get all items
        let mut items = Vec::new();

        loop {
            items.push(parse_item(self)?);

            // another item
            if self.eat(&TokenKind::Comma) {
                if self.check(close) {
                    // premature end
                    return Err(PhotonErrorKind::error(
                        trailing_comma_error,
                        self.current_span(),
                    ));
                }

                continue;
            }

            break;
        }

        // must be followed by the close
        self.expect(close)?;

        Ok(items)
    }

    /// Runs the following functions in `parse` with a parser that has all of the supplied
    /// `type_paramaters` in its scope.
    ///
    /// Only the internal scope `parse` has the type parameters in scope, outside of this `parse`,
    /// all the type parameters are returned.
    fn with_type_parameters<T>(
        &mut self,
        type_parameters: &[String],
        parse: impl FnOnce(&mut Self) -> PhotonResult<T>,
    ) -> PhotonResult<T> {
        let outer_scope =
            std::mem::replace(&mut self.type_parameters_in_scope, type_parameters.to_vec());

        let result = parse(self);

        self.type_parameters_in_scope = outer_scope;

        result
    }

    /// Parses an optional list of type parameters on a declaration
    ///
    /// This is the parameters the declaration takes rather than the ones it uses.
    fn parse_type_parameter_list(&mut self) -> PhotonResult<Vec<String>> {
        // [
        if !self.eat(&TokenKind::LeftBracket) {
            return Ok(Vec::new());
        }

        // T, U, V]
        let parsed = self.parse_delimited_list(
            &TokenKind::RightBracket,
            PhotonErrorKind::UnexpectedEndOfTypeParamsFollowingComma,
            |parser| {
                let span = parser.current_span();
                let name = parser.expect_identifier()?;

                Ok((name, span))
            },
        )?;

        // disallow duplicate type params, as else it doesnt make sense to bind func[T, T] etc.
        // as it would make things a lot more complex
        let mut names: Vec<String> = Vec::with_capacity(parsed.len());
        for (name, span) in parsed {
            if names.contains(&name) {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::DuplicateTypeParameter { name },
                    span,
                ));
            }

            names.push(name);
        }

        Ok(names)
    }

    /// Parses the arguments of a type application after the opening `[`
    fn parse_type_arguments_after_open(&mut self) -> PhotonResult<Vec<TypeName>> {
        self.parse_delimited_list(
            &TokenKind::RightBracket,
            PhotonErrorKind::UnexpectedEndOfTypeArgsFollowingComma,
            |parser| parser.parse_type(),
        )
    }

    /// Parses the explicit type arguments of a call if there are any
    fn parse_optional_call_type_arguments(&mut self) -> PhotonResult<Option<Vec<TypeName>>> {
        // Must be followed by a :: and a [ for it to be an optional call type arg
        if !(self.check(&TokenKind::DoubleColon) && self.check_nth(1, &TokenKind::LeftBracket)) {
            return Ok(None);
        }

        // If it is ::[, then we know it must be type arguments
        self.expect(&TokenKind::DoubleColon)?;
        self.expect(&TokenKind::LeftBracket)?;

        Ok(Some(self.parse_type_arguments_after_open()?))
    }

    /// Turns a qualified name in a type position into a type
    fn type_from_qualified_name(&self, name: QualifiedName) -> TypeName {
        // if name is unqualified (no namespacing) then it can
        // refer to one of the type parameters currently in scope
        // and if it is then it should be a TypeName::GenericParameter instead of
        // the actual qualified name ver
        if name.is_unqualified()
            && self
                .type_parameters_in_scope
                .iter()
                .any(|parameter| parameter == name.last())
        {
            TypeName::GenericParameter(name.last().to_owned())
        } else {
            TypeName::from_qualified_name(name)
        }
    }

    /// Parse a type at the current position
    fn parse_type(&mut self) -> PhotonResult<TypeName> {
        let name = self.parse_qualified_name()?;

        let parsed = self.type_from_qualified_name(name);

        // type is followed by [ so it has type arguments
        if self.eat(&TokenKind::LeftBracket) {
            let arguments = self.parse_type_arguments_after_open()?;

            return Ok(parsed.applied(arguments));
        }

        Ok(parsed)
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

        // No parameters since it ends with )
        if self.eat(&TokenKind::RightParen) {
            return Ok(Vec::new());
        }

        self.parse_delimited_list(
            &TokenKind::RightParen,
            PhotonErrorKind::UnexpectedEndOfParamsFollowingComma,
            |parser| parser.parse_parameter(),
        )
    }

    /// Parses an entire list of arguments etc.
    ///
    /// (expr, expr, expr)
    ///
    /// outputs it as a set of exprs in a Vec's
    fn parse_argument_list(&mut self) -> PhotonResult<Vec<Located<Expression>>> {
        // (
        self.expect(&TokenKind::LeftParen)?;

        // No parameters since it ends with )
        if self.eat(&TokenKind::RightParen) {
            return Ok(Vec::new());
        }

        self.parse_delimited_list(
            &TokenKind::RightParen,
            PhotonErrorKind::UnexpectedEndOfArgsFollowingComma,
            |parser| parser.parse_expression(),
        )
    }

    /// Parses one statement at the current position
    fn parse_statement(&mut self) -> PhotonResult<Located<Statement>> {
        match self.peek_token().map(|tok| &tok.value) {
            Some(TokenKind::Return) => self.parse_return_statement(),
            Some(TokenKind::If) => self.parse_if_statement(),
            Some(TokenKind::While) => self.parse_while_statement(),
            Some(TokenKind::Do) => self.parse_do_statement(),
            Some(TokenKind::For) => self.parse_for_statement(),
            Some(TokenKind::Foreach) => self.parse_foreach_statement(),
            Some(TokenKind::Loop) => self.parse_loop_statement(),
            Some(TokenKind::Break) => self.parse_break_statement(),
            Some(TokenKind::Continue) => self.parse_continue_statement(),
            Some(TokenKind::Boson3(_)) => self.parse_boson3_statement(),
            _ => self.parse_simple_statement_as_statement(),
        }
    }

    fn at_statement_terminator(&self) -> bool {
        self.is_at_end()
            || self.check(&TokenKind::Newline)
            || self.check(&TokenKind::RightBrace)
            || self.check(&TokenKind::Semicolon)
    }

    /// Parses a return statement (return something blahhh or not)
    /// at the current position
    fn parse_return_statement(&mut self) -> PhotonResult<Located<Statement>> {
        // return
        let start = self.expect(&TokenKind::Return)?.span.start;

        // Check if we have a value or not that we are returning
        let value = if self.at_statement_terminator() {
            None
        } else {
            Some(self.parse_expression()?)
        };

        let end = value
            .as_ref()
            .map(|value| value.span.end)
            .unwrap_or_else(|| self.previous_span().end);

        // build final ret statement
        let statement = Located::new(Statement::Return { value }, (start..end).into());

        self.require_statement_terminator()?;

        Ok(statement)
    }

    /// break.
    fn parse_break_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let break_token = self.expect(&TokenKind::Break)?;

        let statement = Located::new(Statement::Break, break_token.span);

        self.require_statement_terminator()?;

        Ok(statement)
    }

    /// continue.
    fn parse_continue_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let continue_token = self.expect(&TokenKind::Continue)?;

        let statement = Located::new(Statement::Continue, continue_token.span);

        self.require_statement_terminator()?;

        Ok(statement)
    }

    /// A legacy boson3 block statement, this is just
    /// forwarded to boson3 so nothing really special.
    fn parse_boson3_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let statement = match self.advance() {
            Ok(Token {
                value: TokenKind::Boson3(body),
                ..
            }) => Located::new(Statement::Boson3 { source: body.body }, body.body_span),

            Ok(Token { value, span }) => {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnexpectedNonB3Statement { found: value },
                    span,
                ));
            }

            Err(error) => Err(error)?,
        };

        self.require_statement_terminator()?;
        Ok(statement)
    }

    /// An if statement pertaining to some condition (and potential else body)
    fn parse_if_statement(&mut self) -> PhotonResult<Located<Statement>> {
        // start position
        let start = self.expect(&TokenKind::If)?.span.start;

        // must be followed by a condition and a then
        let condition = self.parse_expression()?;
        let then_body = self.parse_block()?;

        let mut end = self.previous_span().end;

        // check if there is an else body
        let else_body = if self.eat(&TokenKind::Else) {
            let body = self.parse_block()?;
            end = self.previous_span().end;
            Some(body)
        } else {
            None
        };

        let statement = Located::new(
            Statement::If {
                condition,
                then_body,
                else_body,
            },
            (start..end).into(),
        );

        Ok(statement)
    }

    /// Parses one while loop statement
    fn parse_while_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let start = self.expect(&TokenKind::While)?.span.start;

        // condition then body
        let condition = self.parse_expression()?;

        let body = self.parse_block()?;
        let end = self.previous_span().end;

        let statement = Located::new(Statement::While { condition, body }, (start..end).into());

        Ok(statement)
    }

    /// Parse one do <body> while <condition> statement
    fn parse_do_statement(&mut self) -> PhotonResult<Located<Statement>> {
        // The do statement
        let start = self.expect(&TokenKind::Do)?.span.start;

        let body = self.parse_block()?;

        let _while_tok = self.expect(&TokenKind::While)?;

        // Condition following the block/while
        let condition = self.parse_expression()?;
        let end = condition.span.end;

        let statement = Located::new(Statement::DoWhile { body, condition }, (start..end).into());

        self.require_statement_terminator()?;
        Ok(statement)
    }

    /// For statement, with an initialiser, condition and step.
    ///
    /// for (<initialiser>;<condition>;<step>);
    fn parse_for_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let start = self.expect(&TokenKind::For)?.span.start;

        // (<initialiser
        self.expect(&TokenKind::LeftParen)?;
        let initialiser = if self.check(&TokenKind::Semicolon) {
            None
        } else {
            Some(self.parse_simple_statement()?)
        };

        // ;<condition>;
        self.expect(&TokenKind::Semicolon)?;
        let condition = self.parse_expression()?;
        self.expect(&TokenKind::Semicolon)?;

        // <step>)
        let step = if self.check(&TokenKind::RightParen) {
            None
        } else {
            Some(self.parse_simple_statement()?)
        };

        self.expect(&TokenKind::RightParen)?;

        // body of the for loop
        let body = self.parse_block()?;
        let end = self.previous_span().end;

        let statement = Located::new(
            Statement::For {
                initialiser,
                condition,
                step: Box::new(step),
                body,
            },
            (start..end).into(),
        );

        Ok(statement)
    }

    /// For element in array loop.
    ///
    /// This is for (<binding> in <array>)
    fn parse_foreach_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let start = self.expect(&TokenKind::Foreach)?.span.start;

        // (<binding> in <array>)
        self.expect(&TokenKind::LeftParen)?;
        let binding = self.parse_parameter()?;

        self.expect(&TokenKind::In)?;

        let array = self.parse_expression()?;
        self.expect(&TokenKind::RightParen)?;

        // body
        let body = self.parse_block()?;
        let end = self.previous_span().end;

        let statement = Located::new(
            Statement::ForEach {
                binding,
                array,
                body,
            },
            (start..end).into(),
        );

        Ok(statement)
    }

    /// A forever loop. This is essentially while(1)
    fn parse_loop_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let start = self.expect(&TokenKind::Loop)?.span.start;

        // loop <body>
        let body = self.parse_block()?;
        let end = self.previous_span().end;

        let statement = Located::new(Statement::Loop { body }, (start..end).into());

        Ok(statement)
    }

    /// Parses one block of further statements
    fn parse_block(&mut self) -> PhotonResult<Vec<Located<Statement>>> {
        // opening `{`
        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        // inside statements
        let mut statements = Vec::new();

        // find the closing `}`
        while !self.check(&TokenKind::RightBrace) {
            if self.is_at_end() {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnclosedBlockFound,
                    self.current_span(),
                ));
            }

            // haven't found it yet, parse next statement.
            statements.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;
        self.skip_newlines();
        Ok(statements)
    }

    /// Checks the parser follows with a valid statement
    /// terminator in the current position, otherwise erroring
    fn require_statement_terminator(&mut self) -> PhotonResult<()> {
        if self.check(&TokenKind::Newline) {
            self.skip_newlines();
            return Ok(());
        }

        if self.check(&TokenKind::RightBrace)
            || self.is_at_end()
            || self.check(&TokenKind::Semicolon)
        {
            return Ok(());
        }

        // cant be at the end anyway
        let unexpected_tok = self.advance()?;
        Err(PhotonErrorKind::error(
            PhotonErrorKind::UnexpectedNonEndOfStatement {
                found: unexpected_tok.value,
            },
            unexpected_tok.span,
        ))
    }

    /// Parses a simple statement in the current position
    /// as a normal statement useable for its effect
    fn parse_simple_statement_as_statement(&mut self) -> PhotonResult<Located<Statement>> {
        let simple = self.parse_simple_statement()?;

        // Must be followed by a terminator (else we r gluin)
        self.require_statement_terminator()?;

        Ok(Located::new(Statement::Simple(simple.value), simple.span))
    }

    /// Parses a simple statement at the current position
    /// This includes let, step and assignment/exprs
    fn parse_simple_statement(&mut self) -> PhotonResult<Located<SimpleStatement>> {
        // Starts with Let = let statement
        if self.check(&TokenKind::Let) {
            return self.parse_let_simple_statement();
        }

        // Looks like a step statement, must be one
        if self.looks_like_step_simple_statement() {
            return self.parse_step_simple_statement();
        }

        // Else default to an expression/assignment in this position
        self.parse_assignment_or_expression_simple_statement()
    }

    /// Parses a `let` statement at the current position
    fn parse_let_simple_statement(&mut self) -> PhotonResult<Located<SimpleStatement>> {
        // let <ident>
        let start = self.expect(&TokenKind::Let)?.span.start;
        let name = self.expect_identifier()?;

        // Optional type annotation on this let binding
        let type_annotation = if self.eat(&TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // must be followed by an assignment for let, as
        // combined assignemnts use the base value (potentially unassigned)
        self.expect(&TokenKind::Assign)?;

        // the initialiser for the name
        let initialiser = self.parse_expression()?;
        let end = initialiser.span.end;

        Ok(Located::new(
            SimpleStatement::Let {
                name,
                type_annotation,
                initialiser,
            },
            (start..end).into(),
        ))
    }

    /// Parses a step statement at the position
    fn parse_step_simple_statement(&mut self) -> PhotonResult<Located<SimpleStatement>> {
        let start = self.current_span().start;

        // <name><step op>
        let name = self.expect_identifier()?;

        // i++
        let operator = if self.eat(&TokenKind::PlusPlus) {
            StepOperator::Increment
        }
        // i--
        else if self.eat(&TokenKind::MinusMinus) {
            StepOperator::Decrement
        } else {
            let found_element = self.advance()?;
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedNonStepOperator {
                    found: found_element.value,
                },
                found_element.span,
            ))?;
        };

        let end = self.previous_span().end;

        Ok(Located::new(
            SimpleStatement::Step { name, operator },
            (start..end).into(),
        ))
    }

    /// Parses either an assignment or expression simple statement
    ///
    /// This should be used as the top-level entry to expression parsing where
    /// assignment is to be considered, otherwise only parse_expression.
    fn parse_assignment_or_expression_simple_statement(
        &mut self,
    ) -> PhotonResult<Located<SimpleStatement>> {
        // Left side/what we are assigning
        let left = self.parse_expression()?;
        let start = left.span.start;

        // Whether or not this is an actual assignment expression
        if let Some(operator) = self.parse_assignment_operator() {
            // rhs assignment
            let right = self.parse_expression()?;
            let end = right.span.end;

            Ok(Located::new(
                SimpleStatement::Assignment {
                    target: left,
                    operator,
                    value: right,
                },
                (start..end).into(),
            ))
        } else {
            // Otherwise this is just an expression
            let span = left.span;

            Ok(Located::new(SimpleStatement::Expression(left), span))
        }
    }

    /// Parses the operator for an assignment expression in the form of
    ///
    /// <ident> <assignment_op> <value>
    ///
    /// This includes compound assignment.
    fn parse_assignment_operator(&mut self) -> Option<AssignmentOperator> {
        let operator = match self.peek_token().map(|tok| &tok.value)? {
            TokenKind::Assign => AssignmentOperator::Assign,
            TokenKind::PlusAssign => AssignmentOperator::AddAssign,
            TokenKind::MinusAssign => AssignmentOperator::SubtractAssign,
            TokenKind::StarAssign => AssignmentOperator::MultiplyAssign,
            TokenKind::SlashAssign => AssignmentOperator::DivideAssign,
            TokenKind::PercentAssign => AssignmentOperator::RemainderAssign,
            TokenKind::ShiftLeftAssign => AssignmentOperator::ShiftLeftAssign,
            TokenKind::ShiftRightAssign => AssignmentOperator::ShiftRightAssign,
            TokenKind::BitwiseAndAssign => AssignmentOperator::BitwiseAndAssign,
            TokenKind::BitwiseOrAssign => AssignmentOperator::BitwiseOrAssign,
            TokenKind::BitwiseXorAssign => AssignmentOperator::BitwiseXorAssign,
            _ => return None,
        };

        // advance past this token
        let _ = self.advance();
        Some(operator)
    }

    /// Returns whether or not the sequence of tokens
    /// from the current position can resemble a step statement
    ///
    /// A step statement is as follows:
    ///
    /// <identifier><step><end> such as i++, this is because
    /// array concat conflicts with the operator so we need a special case
    fn looks_like_step_simple_statement(&self) -> bool {
        // no identifier
        if !matches!(
            self.peek_token(),
            Some(Token {
                value: TokenKind::Identifier(_),
                ..
            })
        ) {
            return false;
        }

        // no step op
        if !matches!(
            self.peek_token_nth(1),
            Some(Token {
                value: TokenKind::PlusPlus | TokenKind::MinusMinus,
                ..
            })
        ) {
            return false;
        }

        // no valid end/maybe its valid but we cant be sure
        matches!(
            self.peek_token_nth(2),
            None | Some(Token {
                value: TokenKind::Semicolon
                    | TokenKind::RightParen
                    | TokenKind::Newline
                    | TokenKind::RightBrace,
                ..
            })
        )
    }

    /// Parses a full expression chain, this does not consider assignment!
    fn parse_expression(&mut self) -> PhotonResult<Located<Expression>> {
        self.parse_conditional()
    }

    /// Parses a conditional binary operator (condition ? true_expr : false_expr)
    fn parse_conditional(&mut self) -> PhotonResult<Located<Expression>> {
        // condition
        let condition = self.parse_logical_or()?;

        // ?
        if !self.eat(&TokenKind::Question) {
            return Ok(condition);
        }

        let start = condition.span.start;

        // true_expr
        let when_true = self.parse_expression()?;
        self.expect(&TokenKind::Colon)?;

        // false_expr
        let when_false = self.parse_expression()?;
        let end = when_false.span.end;

        Ok(Located::new(
            Expression::Conditional {
                condition: Box::new(condition),
                when_true: Box::new(when_true),
                when_false: Box::new(when_false),
            },
            (start..end).into(),
        ))
    }

    /// Parses a logical or binary operator
    fn parse_logical_or(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_logical_and()?;

        // potential right side
        while self.eat(&TokenKind::LogicalOr) {
            let right = self.parse_logical_and()?;

            left = make_binary(left, BinaryOperator::LogicalOr, right);
        }

        Ok(left)
    }

    /// Parses a bitwise and binary operator
    fn parse_logical_and(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_bitwise_or()?;

        // potential right side
        while self.eat(&TokenKind::LogicalAnd) {
            let right = self.parse_bitwise_or()?;

            left = make_binary(left, BinaryOperator::LogicalAnd, right);
        }

        Ok(left)
    }

    /// Parses a bitwise or binary operator
    fn parse_bitwise_or(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_bitwise_xor()?;

        // potential right side
        while self.eat(&TokenKind::BitwiseOr) {
            let right = self.parse_bitwise_xor()?;

            left = make_binary(left, BinaryOperator::BitwiseOr, right);
        }

        Ok(left)
    }

    /// Parses a bitwise xor binary operator
    fn parse_bitwise_xor(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_bitwise_and()?;

        // potential right side
        while self.eat(&TokenKind::BitwiseXor) {
            let right = self.parse_bitwise_and()?;

            left = make_binary(left, BinaryOperator::BitwiseXor, right);
        }

        Ok(left)
    }

    /// Parses a bitwise and binary operator
    fn parse_bitwise_and(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_equality()?;

        // potential right
        while self.eat(&TokenKind::BitwiseAnd) {
            let right = self.parse_equality()?;

            left = make_binary(left, BinaryOperator::BitwiseAnd, right);
        }

        Ok(left)
    }

    /// Parses a equality binary operator
    fn parse_equality(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_comparison()?;

        // potential right side
        loop {
            // ==
            let operator = if self.eat(&TokenKind::EqualEqual) {
                BinaryOperator::Equal
            }
            // !=
            else if self.eat(&TokenKind::NotEqual) {
                BinaryOperator::NotEqual
            } else {
                break;
            };

            // must be followed by right side
            let right = self.parse_comparison()?;

            left = make_binary(left, operator, right);
        }

        Ok(left)
    }

    /// Parses a comparison binary operator
    fn parse_comparison(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_shift()?;

        // potentai lright
        loop {
            // <
            let operator = if self.eat(&TokenKind::Less) {
                BinaryOperator::Less
            }
            // <=
            else if self.eat(&TokenKind::LessEqual) {
                BinaryOperator::LessEqual
            // >
            } else if self.eat(&TokenKind::Greater) {
                BinaryOperator::Greater
            // >=
            } else if self.eat(&TokenKind::GreaterEqual) {
                BinaryOperator::GreaterEqual
            } else {
                break;
            };

            // must be followed by right side
            let right = self.parse_shift()?;

            left = make_binary(left, operator, right);
        }

        Ok(left)
    }

    /// Parses a shifting binary operator
    fn parse_shift(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_additive()?;

        // potential right side
        loop {
            // <<
            let operator = if self.eat(&TokenKind::ShiftLeft) {
                BinaryOperator::ShiftLeft
            }
            // >>
            else if self.eat(&TokenKind::ShiftRight) {
                BinaryOperator::ShiftRight
            } else {
                break;
            };

            // must be a following right expression applied to
            let right = self.parse_additive()?;

            left = make_binary(left, operator, right);
        }

        Ok(left)
    }

    /// Parses an additive binary expression
    fn parse_additive(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_multiplicative()?;

        // potential right side
        loop {
            // +
            let operator = if self.eat(&TokenKind::Plus) {
                BinaryOperator::Add
            }
            // -
            else if self.eat(&TokenKind::Minus) {
                BinaryOperator::Subtract
            }
            // array concat ++
            else if self.eat(&TokenKind::PlusPlus) {
                BinaryOperator::ArrayAppend
            } else {
                break;
            };

            // must be a following right expression applied to
            let right = self.parse_multiplicative()?;

            left = make_binary(left, operator, right);
        }

        Ok(left)
    }

    /// Parses a multiplicative binary expression
    fn parse_multiplicative(&mut self) -> PhotonResult<Located<Expression>> {
        // left side
        let mut left = self.parse_cast()?;

        // potential right side
        loop {
            // *
            let operator = if self.eat(&TokenKind::Star) {
                BinaryOperator::Multiply
            }
            // /
            else if self.eat(&TokenKind::Slash) {
                BinaryOperator::Divide
            }
            // %
            else if self.eat(&TokenKind::Percent) {
                BinaryOperator::Remainder
            } else {
                break;
            };

            // must be a following right expression applied to
            let right = self.parse_cast()?;

            left = make_binary(left, operator, right);
        }

        Ok(left)
    }

    /// Parses type reinterpretation expressions which bypass the compiler
    /// and force things to be certain types regardless
    fn parse_cast(&mut self) -> PhotonResult<Located<Expression>> {
        // left hand side
        let mut expression = self.parse_unary()?;

        // see if it is an as with potential type rhs
        while self.eat(&TokenKind::As) {
            let start = expression.span.start;

            // the target type to cast to
            let target_type = self.parse_type()?;
            let end = self.previous_span().end;

            expression = Located::new(
                Expression::Cast {
                    expression: Box::new(expression),
                    target_type,
                },
                (start..end).into(),
            );
        }

        Ok(expression)
    }

    /// Parses a unary expression, (unary op)some
    fn parse_unary(&mut self) -> PhotonResult<Located<Expression>> {
        // The start of the current unary expression
        let start = self.current_span().start;

        // - negative
        if self.eat(&TokenKind::Minus) {
            let operand = self.parse_unary()?;
            let end = operand.span.end;

            return Ok(Located::new(
                Expression::Unary {
                    operator: UnaryOperator::Negate,
                    operand: Box::new(operand),
                },
                (start..end).into(),
            ));
        }

        // ! logical not
        if self.eat(&TokenKind::LogicalNot) {
            let operand = self.parse_unary()?;
            let end = operand.span.end;

            return Ok(Located::new(
                Expression::Unary {
                    operator: UnaryOperator::LogicalNot,
                    operand: Box::new(operand),
                },
                (start..end).into(),
            ));
        }

        // ~ bitwise not
        if self.eat(&TokenKind::BitwiseNot) {
            let operand = self.parse_unary()?;
            let end = operand.span.end;

            return Ok(Located::new(
                Expression::Unary {
                    operator: UnaryOperator::BitwiseNot,
                    operand: Box::new(operand),
                },
                (start..end).into(),
            ));
        }

        self.parse_postfix()
    }

    /// Parses a postfix expression, some(postfix op)
    fn parse_postfix(&mut self) -> PhotonResult<Located<Expression>> {
        // some
        let mut expression = self.parse_primary()?;
        let start = expression.span.start;

        // (postfix op)
        loop {
            // foo(...)
            // this is a function call.

            // explicit type arguments that may be for this function call
            let type_arguments = self.parse_optional_call_type_arguments()?;

            if type_arguments.is_some() || self.check(&TokenKind::LeftParen) {
                // parse all arguments to foo/some
                let arguments = self.parse_argument_list()?;
                let end = self.previous_span().end;

                expression = Located::new(
                    Expression::Call {
                        callee: Box::new(expression),
                        type_arguments: type_arguments.unwrap_or_default(),
                        arguments,
                    },
                    (start..end).into(),
                );

                continue;
            }

            // array[index]
            if self.eat(&TokenKind::LeftBracket) {
                // underlying index
                let index = self.parse_expression()?;

                let right_bracket = self.expect(&TokenKind::RightBracket)?;

                let end = right_bracket.span.end;

                expression = Located::new(
                    Expression::Index {
                        array: Box::new(expression),
                        index: Box::new(index),
                    },
                    (start..end).into(),
                );

                continue;
            }

            // object.field / object.method(...)
            if self.eat(&TokenKind::Dot) {
                let name = self.expect_identifier()?;

                // explicit type arguments, that may be for this method call
                let type_arguments = self.parse_optional_call_type_arguments()?;

                // check if this is an object method call
                if type_arguments.is_some() || self.check(&TokenKind::LeftParen) {
                    let arguments = self.parse_argument_list()?;
                    let end = self.previous_span().end;

                    expression = Located::new(
                        Expression::MethodCall {
                            receiver: Box::new(expression),
                            method: MethodName::Inferred(name),
                            type_arguments: type_arguments.unwrap_or_default(),
                            arguments,
                        },
                        (start..end).into(),
                    );
                } else {
                    // normal object field access
                    let start = expression.span.start;
                    let end = self.previous_span().end;

                    expression = Located::new(
                        Expression::FieldAccess {
                            receiver: Box::new(expression),
                            field: name,
                        },
                        (start..end).into(),
                    );
                }

                continue;
            }

            // qualified method call, as opposed to inferred from type.
            // object->foo::bar(...)
            if self.eat(&TokenKind::Arrow) {
                let name = self.parse_qualified_name()?;

                // optional type arguments to this method call
                let type_arguments = self
                    .parse_optional_call_type_arguments()?
                    .unwrap_or_default();

                let arguments = self.parse_argument_list()?;

                let end = self.previous_span().end;

                expression = Located::new(
                    Expression::MethodCall {
                        receiver: Box::new(expression),
                        method: MethodName::Qualified(name),
                        type_arguments,
                        arguments,
                    },
                    (start..end).into(),
                );

                continue;
            }

            break;
        }

        Ok(expression)
    }

    /// Parses a primary expression, these are direct bottom
    /// non-operator literals etc. that produce a value directly
    fn parse_primary(&mut self) -> PhotonResult<Located<Expression>> {
        // Identifier we want to parse specially using parse_qualified_name
        // so test that first
        if matches!(
            self.peek_token(),
            Some(Token {
                value: TokenKind::Identifier(_),
                ..
            })
        ) {
            let start = self.current_span().start;

            // Parse the full identifier name..,,
            let name = self.parse_qualified_name()?;
            let end = self.previous_span().end;

            return Ok(Located::new(Expression::Name(name), (start..end).into()));
        }

        // there must be a primary in this position
        let next_tok = self.advance()?;
        let (token, span) = (next_tok.value, next_tok.span);

        match token {
            // literals => produce value directly in expr
            TokenKind::IntLiteral(value) => Ok(Located::new(Expression::IntLiteral(value), span)),

            TokenKind::UIntLiteral(value) => Ok(Located::new(Expression::UIntLiteral(value), span)),

            TokenKind::FloatLiteral(value) => {
                Ok(Located::new(Expression::FloatLiteral(value), span))
            }

            TokenKind::BoolLiteral(value) => Ok(Located::new(Expression::BoolLiteral(value), span)),

            // sub-expression in paren
            TokenKind::LeftParen => {
                let start = span.start;

                // (expr)
                let mut expression = self.parse_expression()?;
                let right_paren = self.expect(&TokenKind::RightParen)?;

                expression.span = (start..right_paren.span.end).into();
                Ok(expression)
            }

            // array literal `[`
            TokenKind::LeftBracket => self.parse_array_literal_after_open(span.start),

            // legacy boson element b3<type>
            TokenKind::Boson3(BosonBodyToken {
                declared_type,
                body,
                body_span,
            }) => Ok(Located::new(
                Expression::Boson3 {
                    declared_type: self
                        .type_from_qualified_name(QualifiedName::from_text(&declared_type)),
                    body,
                    body_span,
                },
                span,
            )),

            // non-valid primary expressions
            actual => Err(PhotonErrorKind::error(
                PhotonErrorKind::UnexpectedNonExpression { found: actual },
                span,
            ))?,
        }
    }

    /// Parses the remaining elements of an array literal
    /// after the opening `[`
    ///
    /// The start should be the location where the `[` is.
    fn parse_array_literal_after_open(
        &mut self,
        start: usize,
    ) -> PhotonResult<Located<Expression>> {
         // parse the array elements set
        let elements = if self.eat(&TokenKind::RightBracket) {
            Vec::new()
        } else {
            self.parse_delimited_list(
                &TokenKind::RightBracket,
                PhotonErrorKind::UnexpectedEndOfArrayElemsFollowingComma,
                |parser| parser.parse_expression(),
            )?
        };

        // Both paths have consumed the closing ].
        let end = self.previous_span().end;

        Ok(Located::new(
            Expression::ArrayLiteral(elements),
            (start..end).into(),
        ))
    }
}

/// Make a binary expression from a
///
/// left    <operator>    right
///
/// expression.
fn make_binary(
    left: Located<Expression>,
    operator: BinaryOperator,
    right: Located<Expression>,
) -> Located<Expression> {
    let span: SourceSpan = (left.span.start..right.span.end).into();

    Located::new(
        Expression::Binary {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        },
        span,
    )
}
