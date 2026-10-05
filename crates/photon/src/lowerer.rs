//! lowerer.rs
//!
//! This is the lowering component of the `Photon3` sugar layer,
//! responsible for:
//!
//! - taking the full ast
//! - resolving it down into std.b3 source code using its macros
//! - resolving types (simply)
//! - resolving whether things produce values or not and handling void appropriately with drop

use std::collections::HashMap;

use crate::{
    ast::{
        Expression, FunctionDeclaration, Located, Module, ObjectDeclaration, Parameter,
        QualifiedName, SimpleStatement, SourceSpan, Statement, TopLevelItem, TypeName,
    },
    errors::{PhotonErrorKind, PhotonResult, TypeMismatchSource},
};

/// The output of the lowering phase,
/// this is the lowered version of a module as a textual version
/// of the std.b3 macro library with boson3 macros
#[derive(Debug, Clone)]
pub struct LoweredModule {
    pub contents: String,
}

/// One lowered value, containing the code required
/// to produce the value, the type of the value expected
///
/// `Void` expressions must explicitly leave no values on the stack!
/// everything else must produce the value desired by the type.
struct LoweredExpression {
    code: String,
    value_type: TypeName,
}

/// The context of a function,
/// this is essentially a mapping of locals currently
/// in the context of the function to their expected type.
#[derive(Debug)]
struct FunctionContext {
    locals: HashMap<String, TypeName>,

    // The return type, technically we could get this from the
    // symbol table and drill that, but its easier to just store it here
    ret_type: TypeName,
}

/// The full symbol table for a module, which defines
/// all the functions and objects that can be referred to
///
/// (capabilities are missing because they are expected
/// to be invoked using the escape boson3 statement hatch).
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    functions: HashMap<QualifiedName, FunctionSignature>,
    objects: HashMap<QualifiedName, ObjectSignature>,
    globals: HashMap<QualifiedName, TypeName>,
}

/// An output source location which can be accepted by the gluon3 linker
/// with line and col info.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

/// Converts byte offsets from lexer/parser spans back into line/col
/// positions that the tokens actually occured at.
#[derive(Debug, Clone)]
pub struct SourceMap {
    line_starts: Vec<usize>,
}

/// One defined function in the symbol table that can be referred to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub name: QualifiedName,
    pub parameters: Vec<TypeName>,
    pub return_type: TypeName,
}

/// One defined object in the symbol table that can be referred to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSignature {
    pub name: QualifiedName,
    pub fields: Vec<(String, TypeName)>,
}

/// The actual lowerer itself, this is responsible
/// for handling the lowering from the produced `ast` to
/// boson3 source code with std.b3 included.
pub struct Lowerer<'symbols, 'source_map, 'module> {
    /// All of the introduced names/elements that
    /// can be referred to by a symbol in the current context
    /// globally (see: `FunctionContext`)
    symbols: &'symbols SymbolTable,
    source_map: &'source_map SourceMap,
    module: &'module Module,
}

impl LoweredExpression {
    /// Creates a new lowered expression, this should be some
    /// code that produces a value of some type.
    ///
    /// A `Void` type expression is one which is assumed to produce no value
    /// on the stack.
    fn value(code: impl Into<String>, value_type: TypeName) -> Self {
        Self {
            code: code.into(),
            value_type,
        }
    }

    /// Returns a lowered expresion that produces no value, essentially
    /// an expression of the Void type.
    fn no_value(code: impl Into<String>) -> Self {
        Self::value(code, TypeName::Void)
    }

    /// Returns whether or not this lowered expression produces a value
    /// e.g it is of the `Void` type.
    fn produces_value(&self) -> bool {
        self.value_type == TypeName::Void
    }

    /// Validates that this lowered expression has some type `expected`
    /// otherwise returns an error in the type mismatch source of `source`
    fn expect_type(
        &self,
        span: SourceSpan,
        expected: TypeName,
        source: TypeMismatchSource,
    ) -> PhotonResult<()> {
        if self.value_type != expected {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::TypeMismatchSource {
                    expected,
                    found: self.value_type.clone(),
                    source,
                },
                span,
            ));
        }

        Ok(())
    }
}

impl SourceMap {
    /// Creates a new source map over the provided source,
    /// this will map byte offsets back to lines based on the
    /// contents of `source`.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];

        // Map all newline locations, so we get the byte offset of each newline (line starts)
        for (byte_offset, character) in source.char_indices() {
            if character == '\n' {
                line_starts.push(byte_offset + character.len_utf8());
            }
        }
        Self { line_starts }
    }

    /// Returns the source location of a byte offset into the
    /// provided `SourceMap`'s source
    pub fn location(&self, byte_offset: usize) -> SourceLocation {
        // Find the corresponding line that matches this byte offset
        let line_index = self
            .line_starts
            .partition_point(|line_start| *line_start <= byte_offset)
            .saturating_sub(1);

        let line_start = self.line_starts[line_index];

        // remaining offset is then offset into that line (col)
        SourceLocation {
            line: line_index + 1,
            column: byte_offset.saturating_sub(line_start),
        }
    }

    /// Turns a span into a source location based on the start of the span
    /// as the source location requested.
    pub fn span_start(&self, span: SourceSpan) -> SourceLocation {
        self.location(span.start)
    }

    /// Turns a span into a source location based on the end of the span
    /// as the source location requested.
    pub fn span_end(&self, span: SourceSpan) -> SourceLocation {
        self.location(span.end)
    }
}

impl ObjectSignature {
    /// Returns the type of a field based on it's name (if it exists),
    /// otherwise None
    pub fn field_type(&self, field_name: &str) -> Option<&TypeName> {
        self.fields
            .iter()
            .find_map(|(name, field_type)| (name == field_name).then_some(field_type))
    }
}

impl SymbolTable {
    /// Collects all of the symbols referenced by the set of provided modules out.
    ///
    /// This will insert into a new symbol table all of these symbols for all objects,
    /// functions and globals defined that can then be referred to in other files under
    /// their respective namespaces.
    ///
    /// # Errors
    ///
    /// Duplicate definitions under the same symbol name for a symbol type is not allowed.
    pub fn collect_all_symbols_from_modules(modules: &[Module]) -> PhotonResult<Self> {
        let mut table = Self::default();

        // Collect defs from each namespace
        for module in modules {
            let namespace = &module.namespace;

            // Collect all the top level defs into symbol table
            for item in &module.items {
                match &item.value {
                    TopLevelItem::Function(function) => {
                        table.insert_function(namespace, function, item.span)?;
                    }
                    TopLevelItem::Object(object) => {
                        table.insert_object(namespace, object, item.span)?;
                    }
                    TopLevelItem::Global(global) => {
                        // this global exists under this namespace
                        let name = QualifiedName::from_text(&global.name).qualify_from(namespace);
                        let declared_type = global.declared_type.canonicalise(namespace);

                        // duplicate global!
                        if table.globals.insert(name.clone(), declared_type).is_some() {
                            return Err(PhotonErrorKind::error(
                                PhotonErrorKind::DuplicateGlobal { name },
                                item.span,
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(table)
    }

    /// Attempts to resolve a function from it's qualfied name in the
    /// symbol table.
    pub fn function(&self, name: &QualifiedName) -> Option<&FunctionSignature> {
        self.functions.get(name)
    }

    /// Attempts to resolve an object from it's qualfied name in the
    /// symbol table.
    pub fn object(&self, name: &QualifiedName) -> Option<&ObjectSignature> {
        self.objects.get(name)
    }

    /// Attempts to resolve a global from it's qualfied name in the
    /// symbol table.
    pub fn global(&self, name: &QualifiedName) -> Option<&TypeName> {
        self.globals.get(name)
    }

    /// Attempts to insert a new function under some name and declaration
    /// into the global symbol table.
    ///
    /// # Errors
    ///
    /// Duplicate functions under the same qualified name are not permitted.
    fn insert_function(
        &mut self,
        namespace: &QualifiedName,
        function: &FunctionDeclaration,
        decl_span: SourceSpan,
    ) -> PhotonResult<()> {
        // The name of this function qualified under the current namespace
        let name = QualifiedName::from_text(&function.name).qualify_from(namespace);

        // This is the "signature" of the function which can be referred to elsewhere by its name
        let signature = FunctionSignature {
            name: name.clone(),
            parameters: function
                .parameters
                .iter()
                .map(|parameter| parameter.declared_type.canonicalise(namespace))
                .collect(),

            return_type: function.return_type.canonicalise(namespace),
        };

        // Functions cannot be duplicates.
        if self.functions.insert(name.clone(), signature).is_some() {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::DuplicateFunction { name },
                decl_span,
            ));
        }

        Ok(())
    }

    /// Attempts to insert a new object under some name and declaration
    /// into the global symbol table.
    ///
    /// # Errors
    ///
    /// Duplicate objects under the same qualified name are not permitted.
    fn insert_object(
        &mut self,
        namespace: &QualifiedName,
        object: &ObjectDeclaration,
        decl_span: SourceSpan,
    ) -> PhotonResult<()> {
        // The name of this object qualified under the current namespace
        let name = QualifiedName::from_text(&object.name).qualify_from(namespace);

        // This is the "signature" of the object which can be referred to elsewhere by its name
        let signature = ObjectSignature {
            name: name.clone(),
            fields: object
                .fields
                .iter()
                .map(|field| {
                    (
                        field.name.clone(),
                        field.declared_type.canonicalise(namespace),
                    )
                })
                .collect(),
        };

        // objects cannot be duplicates.
        if self.objects.insert(name.clone(), signature).is_some() {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::DuplicateObject { name },
                decl_span,
            ));
        }

        Ok(())
    }
}

impl<'symbols, 'source_map, 'module> Lowerer<'symbols, 'source_map, 'module> {
    /// Create a new lowerer over the original content `source` that will lower
    /// all of the items in `module` down into the associated string std.b3 form.
    ///
    /// The `namespace`
    pub fn new(
        symbols: &'symbols SymbolTable,
        source_map: &'source_map SourceMap,
        module: &'module Module,
    ) -> Self {
        Self {
            symbols,
            source_map,
            module,
        }
    }

    /// Fully outputs the lowered module based on this lowerer's input components
    ///
    /// # Errors
    ///
    /// If the lowering fails in any way, such as illegal or duplicate elements
    /// etc. then this will return that corresponding error!
    ///
    /// See `PhotonError` for more details.
    pub fn lower_to_string(&self) -> PhotonResult<LoweredModule> {
        let mut output_string = Vec::new();

        // lower each individual TLI with smp
        for item in &self.module.items {
            output_string.push(self.lower_top_level_item(item)?);
        }

        Ok(LoweredModule {
            contents: output_string.join("\n"),
        })
    }

    /// Lowers one top level item declaration down into its code/string variant
    fn lower_top_level_item(&self, item: &Located<TopLevelItem>) -> PhotonResult<String> {
        let code = match &item.value {
            // namespace guaranteed to be uniq alr due to parser
            TopLevelItem::Namespace(namespace) => format!("@namespace {namespace}"),
            TopLevelItem::Requires(namespace) => format!("@requires {namespace}"),
            TopLevelItem::Entry(function) => format!("@entry {function}"),
            TopLevelItem::Capability(capability) => {
                format!("@capability {} {}", capability.name, capability.number)
            }
            TopLevelItem::Global(global) => format!("@global {}", global.name),
            TopLevelItem::Object(object) => {
                let field_names = object
                    .fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("@object {} ({field_names})", object.name)
            }
            TopLevelItem::Function(function) => self.lower_function(function)?,
        };

        Ok(self.with_source_location(item.span, code))
    }

    /// Lowers one function down into it's string code version.
    fn lower_function(&self, function: &FunctionDeclaration) -> PhotonResult<String> {
        // Create a function context, this is mapping of all the locals to
        // their expected types and the ret type which we pass to all sub-statements
        // of this function for type validation

        // we begin with the parameters which are mapped in as the first few locals in lepton3

        let parameter_names = function
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let mut context = FunctionContext {
            locals: function
                .parameters
                .iter()
                .map(|parameter| {
                    (
                        parameter.name.clone(),
                        parameter.declared_type.canonicalise(&self.module.namespace),
                    )
                })
                .collect(),
            ret_type: function.return_type.canonicalise(&self.module.namespace),
        };

        // lower each sub-statement of the function in the current context.
        let mut output = vec![format!("@fn {} ({parameter_names})", function.name)];
        for statement in &function.body {
            output.push(self.lower_statement(statement, &mut context)?);
        }

        Ok(output.join("\n"))
    }

    /// Lowers one statement down into its source code, this statement is being ran
    /// in the context of the function `context`.
    fn lower_statement(
        &self,
        statement: &Located<Statement>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        let span = statement.span;

        // convert statement type to specific string code ver
        let code = match &statement.value {
            Statement::Simple(simple_statement) => todo!(),
            Statement::Return { value } => todo!(),
            Statement::TailCall { call } => todo!(),
            Statement::If {
                condition,
                then_body,
                else_body,
            } => todo!(),
            Statement::While { condition, body } => todo!(),
            Statement::DoWhile { body, condition } => todo!(),
            Statement::For {
                initializer,
                condition,
                step,
                body,
            } => todo!(),
            Statement::ForEach {
                binding,
                array,
                body,
            } => todo!(),
            Statement::Loop { body } => self.lower_loop_statement(body, context)?,
            Statement::Break => "!std::break".to_string(),
            Statement::Continue => "!std::continue".to_string(),
            Statement::Boson3 { source } => self.lower_boson3_statement(source, span)?,
        };

        Ok(self.with_source_location(span, code))
    }

    /// Lowers one loop statement down into source code
    fn lower_loop_statement(
        &self,
        body: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        let body = self.lower_statement_block(body, context)?;
        Ok(format!("!std::forever {}", block(&body)))
    }

    /// Lowers a statement block down into source code (a block is essentially just a Vec<Statement>)
    fn lower_statement_block(
        &self,
        block: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        block
            .iter()
            .map(|statement| self.lower_statement(statement, context))
            .collect::<Result<Vec<_>, _>>()
            .map(|statements| statements.join("\n"))
    }

    /// Lowers a Boson3 source statement into the underlying code
    ///
    /// This is really just a special function that adds @source_loc
    /// lines to each body line of the boson3 source statement, because
    /// else it'd be difficult to map back into where in the src code,
    /// even though its already textualised
    fn lower_boson3_statement(&self, body: &String, body_span: SourceSpan) -> PhotonResult<String> {
        // Total output of this boson3 lowering
        let mut output = Vec::new();

        // Split all lines to insert @source_loc in with the byte offset appended to the line
        let lines = body.lines().map(|line| {
            let offset = body.len() as usize - line.as_ptr() as usize;
            (line, offset)
        });

        for (line, offset) in lines {
            // no content
            if line.trim().is_empty() {
                output.push(line.to_string());
                continue;
            }

            // the total number of leading whitespace chars
            let leading_whitespace = line.len() - line.trim().as_ptr() as usize;

            // actual offset for the real line contents
            let source_offset = body_span
                .start
                .saturating_add(offset)
                .saturating_add(leading_whitespace);

            let location = self.source_map.location(source_offset);

            // rebuild line with loc
            output.push(format!("@source_loc {} {}", location.line, location.column));

            output.push(line.to_string());
        }

        Ok(output.join("\n"))
    }

    /// Lowers a foreach statement in the current function context,
    fn lower_foreach_statement(
        &self,
        span: SourceSpan,
        binding: &Parameter,
        array: &Located<Expression>,
        body: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // for elem in array
        let array = self.expect_lowered_expression_type(
            array,
            &TypeName::Array,
            context,
            TypeMismatchSource::ForEachArraySource,
        )?;

        // type of the local binding
        let binding_type = binding.declared_type.canonicalise(&self.module.namespace);
        self.declare_local(&binding.name, binding_type, span, context)?;

        // local binding is declared in this context
        let body = self.lower_statement_block(body, context)?;

        Ok(format!(
            "!std::foreach ( {} : {} ) {}",
            binding.name,
            block(&array.code),
            block(&body)
        ))
    }

    /// Lowers a for statement in the current function context,
    fn lower_for_statement(
        &self,
        initializer: &Option<Located<SimpleStatement>>,
        condition: &Located<Expression>,
        step: &Box<Option<Located<SimpleStatement>>>,
        body: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // initialiser
        let initializer = initializer
            .map(|simple_statement| self.lower_simple_statement(simple_statement, context))
            .transpose()?
            .unwrap_or_default();

        // condition should return a bool type
        let condition = self.expect_lowered_expression_type(
            condition,
            &TypeName::Bool,
            context,
            TypeMismatchSource::ForCondition,
        )?;

        // The step component of the statement
        let step = step
            .map(|simple_statement| self.lower_simple_statement(simple_statement, context))
            .transpose()?
            .unwrap_or_default();

        let body = self.lower_statement_block(body, context)?;

        Ok(format!(
            "!std::for ( {} ; {} ; {} ) {}",
            block(&initializer),
            block(&condition.code),
            block(&step),
            block(&body)
        ))
    }

    /// Lowers a dowhile statement in the current function context,
    fn lower_dowhile_statement(
        &self,
        body: &Vec<Located<Statement>>,
        condition: &Located<Expression>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // body
        let body = self.lower_statement_block(body, context)?;

        // condition should be a bool
        let condition = self.expect_lowered_expression_type(
            condition,
            &TypeName::Bool,
            context,
            TypeMismatchSource::DoWhileCondition,
        )?;

        Ok(format!(
            "!std::do {} while ( {} )",
            block(&body),
            block(&condition.code)
        ))
    }

    /// Lowers an expression with a certain type involved.
    ///
    /// Expects that the lowered expression returns this type,
    /// returning the corresponding typemismatch error if not matching.
    fn expect_lowered_expression_type(
        &self,
        expression: &Located<Expression>,
        expected_type: &TypeName,
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<LoweredExpression> {
    }

    /// Outputs one lowered string from some code and a span that
    /// contains the @source_loc decorative directive for the full source location
    /// from the original photon3 file to pass through to the final lepton3 binary
    fn with_source_location(&self, span: SourceSpan, code: String) -> String {
        let location = self.source_map.span_start(span);
        format!("@source_loc {} {}\n{code}", location.line, location.column)
    }
}

/// Converts a string of `contents` to
/// a boson3 block essentially
/// {contents}
fn block(contents: &str) -> String {
    format!("{{ {contents} }}")
}
