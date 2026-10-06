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
        AssignmentOperator, Expression, FunctionDeclaration, Located, MethodName, Module,
        ObjectDeclaration, Parameter, QualifiedName, SimpleStatement, SourceSpan, Statement,
        StepOperator, TopLevelItem, TypeName,
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

/// Intrinsics, these lower directly as some function
/// call to a std.b3 construct.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Intrinsic {
    Clone,
    TypeOf,
    Drop,
    Assert,
    IntToFloat,
    UIntToFloat,
    FloatToInt,
    FloatToUInt,
    IntToUInt,
    UIntToInt,
}

/// The current "global/local/object" state of an assignment
#[derive(Debug, Clone)]
enum AssignmentTarget {
    Local,
    Global,
    Array {
        array_index: String,
    },
    Object {
        object_name: QualifiedName,
        field_name: String,
    },
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

    /// Validates that this lowered expression has some value,
    /// otherwise errors
    fn expect_value(&self, span: SourceSpan, source: TypeMismatchSource) -> PhotonResult<()> {
        if !self.produces_value() {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::ExpressionDidNotProduceValue { source },
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

        // Can't be in the object table already under the exact same name,
        // as else we have duplicate callables and call cant resolve which
        if self.objects.contains_key(&name) {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::DuplicateCallable { name },
                decl_span,
            ));
        }

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

        // Can't be in the function table already under the exact same name,
        // as else we have duplicate callables and call cant resolve which
        if self.functions.contains_key(&name) {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::DuplicateCallable { name },
                decl_span,
            ));
        }

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
            Statement::Simple(simple_statement) => {
                self.lower_simple_statement(simple_statement, span, context)?
            }
            Statement::Return { value } => self.lower_return_statement(value, span, context)?,
            Statement::If {
                condition,
                then_body,
                else_body,
            } => self.lower_if_statement(condition, then_body, else_body, context)?,
            Statement::While { condition, body } => {
                self.lower_while_statement(condition, body, context)?
            }
            Statement::DoWhile { body, condition } => {
                self.lower_dowhile_statement(body, condition, context)?
            }
            Statement::For {
                initializer,
                condition,
                step,
                body,
            } => self.lower_for_statement(initializer, condition, step, body, context)?,
            Statement::ForEach {
                binding,
                array,
                body,
            } => self.lower_foreach_statement(span, binding, array, body, context)?,
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
            .as_ref()
            .map(|simple_statement| {
                self.lower_simple_statement(&simple_statement.value, simple_statement.span, context)
            })
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
        let step = Option::as_ref(step)
            .map(|simple_statement| {
                self.lower_simple_statement(&simple_statement.value, simple_statement.span, context)
            })
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

    /// Lowers a while statement in the current function context,
    fn lower_while_statement(
        &self,
        condition: &Located<Expression>,
        body: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // condition should be of type bool
        let condition = self.expect_lowered_expression_type(
            condition,
            &TypeName::Bool,
            context,
            TypeMismatchSource::WhileCondition,
        )?;

        // body
        let body = self.lower_statement_block(body, context)?;

        Ok(format!(
            "!std::while ( {} ) {}",
            block(&condition.code),
            block(&body)
        ))
    }

    /// Lowers an expression as a statement in the current function context
    ///
    /// If the expression is non-void it's value is dropped, otherwise
    /// the void expression is assumed to drop its value.
    fn lower_expression_statement(
        &self,
        expression: &Located<Expression>,
        context: &FunctionContext,
    ) -> PhotonResult<String> {
        let expression = self.lower_expression(expression, context)?;

        // void should auto-drop per fn comment, and we dont want to stack underflow
        if expression.is_void() {
            Ok(expression.code)
        } else {
            Ok(format!("!std::drop {}", block(&expression.code)))
        }
    }

    //// Lowers a simple statement in the current function context,
    fn lower_simple_statement(
        &self,
        statement: &SimpleStatement,
        span: SourceSpan,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // get code out from statement type
        let code = match &statement {
            SimpleStatement::Let {
                name,
                type_annotation,
                initializer,
            } => self.lower_let_statement(name, type_annotation, initializer, span, context)?,
            SimpleStatement::Assignment {
                target,
                operator,
                value,
            } => self.lower_assignment(target, *operator, value, span, context)?,
            SimpleStatement::Step { name, operator } => {
                self.lower_step(name, *operator, span, context)?
            }
            SimpleStatement::Expression(expr) => self.lower_expression_statement(expr, context)?,
        };

        Ok(self.with_source_location(span, code))
    }

    //// Lowers an assignment statement in the current function context
    fn lower_assignment(
        &self,
        target: &Located<Expression>,
        operator: AssignmentOperator,
        value: &Located<Expression>,
        span: SourceSpan,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // The RHS value of the assignment, which we are assigning
        // to `target` (some expr).
        let value = self.expect_lowered_expression_value(
            value,
            context,
            TypeMismatchSource::AssignmentRHS,
        )?;

        match &target.value {
            // A direct name of which we assign to a local/global
            Expression::Name(name) if name.is_unqualified() => {
                // In the function context locals, assign to local.
                if let Some(local_type) = context.locals.get(name.last()) {
                    return assignment_macro(
                        &value.code,
                        &name.last().to_string(),
                        local_type,
                        AssignmentTarget::Local,
                        operator,
                        span,
                    );
                }

                // Not a local? try globals, else it doesn't exist
                let global_name = name.resolve(&self.module.namespace);
                let global_type = self.symbols.global(&global_name).ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::UnknownAssignmentTarget {
                            name: name.to_string(),
                        },
                        span,
                    )
                })?;

                return assignment_macro(
                    &value.code,
                    &global_name.to_string(),
                    global_type,
                    AssignmentTarget::Global,
                    operator,
                    span,
                );
            }
            // A qualified name/not quite direct, which we assume to be a global.
            Expression::Name(name) => {
                let global_name = name.resolve(&self.module.namespace);
                let global_type = self.symbols.global(&global_name).ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::UnknownGlobal {
                            name: global_name.to_string(),
                        },
                        span,
                    )
                })?;

                return assignment_macro(
                    &value.code,
                    &global_name.to_string(),
                    global_type,
                    AssignmentTarget::Global,
                    operator,
                    span,
                );
            }

            // field access, assignment to an object.
            Expression::FieldAccess { receiver, field } => {
                // resolve obj expr
                let receiver = self.expect_lowered_expression_value(
                    receiver,
                    context,
                    TypeMismatchSource::FieldAssignmentReciever,
                )?;

                // must be of an object type.
                let object_name = receiver.value_type.object_name().ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::FieldAssignmentToNonObjectType {
                            found: receiver.value_type.clone(),
                        },
                        span,
                    )
                })?;

                let object = self.symbols.object(object_name).ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::UnknownObject {
                            name: object_name.clone(),
                        },
                        span,
                    )
                })?;

                // Ensure this object actually has this field
                expect_object_field(object, field, target.span)?;

                return assignment_macro(
                    &value.code,
                    &receiver.code,
                    &receiver.value_type,
                    AssignmentTarget::Object {
                        object_name: object.name.clone(),
                        field_name: field.clone(),
                    },
                    operator,
                    span,
                );
            }

            // Assignment to an array at an index
            Expression::Index { array, index } => {
                let (array, index) = self.lower_array_index(
                    array,
                    index,
                    context,
                    TypeMismatchSource::ArrayIndexAssignmentReciever,
                )?;

                return assignment_macro(
                    &value.code,
                    &array.code,
                    &value.value_type,
                    AssignmentTarget::Array {
                        array_index: index.code,
                    },
                    operator,
                    span,
                );
            }
            _ => Err(PhotonErrorKind::error(
                PhotonErrorKind::InvalidAssignmentTarget,
                span,
            )),
        }
    }

    //// Lowers a step statement in the current function context
    fn lower_step(
        &self,
        name: &str,
        operator: StepOperator,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<String> {
        // Get the type of the local, as we have special intrinsic step to use based on it.
        let local_type = context.locals.get(name).ok_or_else(|| {
            PhotonErrorKind::error(
                PhotonErrorKind::UnknownLocal {
                    name: name.to_string(),
                },
                span,
            )
        })?;

        // which step macro to use
        let macro_name = match local_type {
            TypeName::Int => "int_step",
            TypeName::UInt => "uint_step",
            other => {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::StepOperatorTypeMismatch {
                        type_name: other.clone(),
                        operator,
                        local_name: name.to_string(),
                    },
                    span,
                ));
            }
        };

        Ok(format!("!std::{macro_name} {name} {operator}"))
    }

    //// Lowers a let statement in the current function context,
    /// essentially just a declaration
    fn lower_let_statement(
        &self,
        name: &str,
        type_annotation: &Option<TypeName>,
        initializer: &Located<Expression>,
        span: SourceSpan,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // The initialiser should return a value (which matches the annotation if req)
        let initializer = match type_annotation {
            Some(decl_type) => self.expect_lowered_expression_type(
                initializer,
                decl_type,
                context,
                TypeMismatchSource::LetLocalInitialiser,
            )?,
            None => self.expect_lowered_expression_value(
                initializer,
                context,
                TypeMismatchSource::LetLocalInitialiser,
            )?,
        };

        // The local type is from the initialiser
        let local_type = initializer.value_type.canonicalise(&self.module.namespace);

        self.declare_local(name, local_type, span, context)?;
        Ok(format!("!std::let {name} = {}", block(&initializer.code)))
    }

    /// Lowers an if statement in the current function context,
    fn lower_if_statement(
        &self,
        condition: &Located<Expression>,
        then_body: &Vec<Located<Statement>>,
        else_body: &Option<Vec<Located<Statement>>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // condition should be of type bool
        let condition = self.expect_lowered_expression_type(
            condition,
            &TypeName::Bool,
            context,
            TypeMismatchSource::IfCondition,
        )?;

        // body
        let then_body = self.lower_statement_block(then_body, context)?;

        // whether or not there's an else
        Ok(if let Some(else_body) = else_body {
            // body of else
            let else_body = self.lower_statement_block(else_body, context)?;
            format!(
                "!std::if_else ( {} ) {} else {}",
                block(&condition.code),
                block(&then_body),
                block(&else_body)
            )
        } else {
            format!(
                "!std::if ( {} ) {}",
                block(&condition.code),
                block(&then_body)
            )
        })
    }

    /// Lowers a return statement in the current function context,
    fn lower_return_statement(
        &self,
        value: &Option<Located<Expression>>,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<String> {
        // If the return statement does not produce an expression
        let Some(expression) = value else {
            // And we don't expect anything, return void!
            if context.ret_type == TypeName::Void {
                return Ok("!std::return_void".to_owned());
            }

            // Otherwise error since we did not produce any value for this non-void function
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::ReturnInVoidFn,
                span,
            ));
        };

        // Check if this is a tail call that we can lower, and return that
        if let Some(tail_call) = self.lower_tail_call(expression, &context.ret_type, context)? {
            return Ok(tail_call);
        }

        // If it wasn't a tail call then consider the normal return case for
        // this expression.
        if context.ret_type == TypeName::Void {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::NoReturnExpectedReturn {
                    expected: context.ret_type.clone(),
                },
                span,
            ));
        }

        // get the lowered expr, should match function ret type
        let lowered = self.expect_lowered_expression_type(
            expression,
            &context.ret_type,
            context,
            TypeMismatchSource::ReturnValue,
        )?;

        Ok(format!("!std::return {}", block(&lowered.code),))
    }

    /// Attempts to lower an expression in return position as a tail call.
    /// returns an option dictating whether or not it could (with the produced code)
    fn lower_tail_call(
        &self,
        expression: &Located<Expression>,
        expected_return_type: &TypeName,
        context: &FunctionContext,
    ) -> PhotonResult<Option<String>> {
        match &expression.value {
            Expression::Call { callee, arguments } => {
                let Expression::Name(function_name) = &callee.value else {
                    // we don't handle name producing expressions such as
                    // (function_name)(args), for simplicity. as this would be pretty annoying
                    // to manually consider here, lol just use proper calls.
                    return Ok(None);
                };

                // intrinsics which we cant directly call
                if resolve_intrinsic(function_name).is_some() {
                    return Ok(None);
                }

                // resolve to underlying function
                let resolved_name = function_name.resolve(&self.module.namespace);

                // if this is a call to an object constructor, then we cant tail call to it because
                // its not a proper tail callable thing (object.new vs call)
                if self.symbols.object(&resolved_name).is_some() {
                    return Ok(None);
                }

                // resolve the signature of this function
                let signature = self.symbols.function(&resolved_name).ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::UnknownFunction {
                            name: resolved_name.clone(),
                        },
                        expression.span,
                    )
                })?;

                // Tail-calling will bypass our normalisation of void/non-void with drops,
                // so make sure the types agree (e.g we don't drop a non-void thing).
                if !tail_return_shape_matches(expected_return_type, &signature.return_type) {
                    return Ok(None);
                }

                // ensure argument count matches else error
                expect_argument_count(
                    signature.parameters.len(),
                    arguments.len(),
                    expression.span,
                )?;

                // lower and tailcall
                let arguments = self.lower_arguments(
                    arguments,
                    context,
                    TypeMismatchSource::TailCallArguments,
                )?;
                Ok(Some(format!(
                    "!std::tailcall {resolved_name} ( {} )",
                    lowered_exprs_block(&arguments),
                )))
            }

            Expression::MethodCall {
                receiver,
                method,
                arguments,
            } => {
                // lower the reciever down into its actual producing code
                let receiver = self.expect_lowered_expression_value(
                    receiver,
                    context,
                    TypeMismatchSource::TailCallMethodReciever,
                )?;

                // Inferred Array methods are std.b3 array operations
                // these cannot be tail called as they are special macros intead
                // and i dont feel like changing it grrrrr
                if receiver.value_type == TypeName::Array
                    && matches!(method, MethodName::Inferred(_))
                {
                    return Ok(None);
                }

                // resolve the full method name from the type and method
                let method_name =
                    self.resolve_method_name(&receiver.value_type, method, expression.span)?;

                // get the signature of this method
                let signature = self.symbols.function(&method_name).ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::UnknownMethod {
                            name: method_name.clone(),
                            method_type: receiver.value_type,
                        },
                        expression.span,
                    )
                })?;

                // Tail-calling will bypass our normalisation of void/non-void with drops,
                // so make sure the types agree (e.g we don't drop a non-void thing).
                if !tail_return_shape_matches(expected_return_type, &signature.return_type) {
                    return Ok(None);
                }

                // The receiver is the first function argument, but it does not
                // appear in the surface argument list.
                let expected_argument_count = signature.parameters.len().saturating_sub(1);

                // ensure the surface arg count matches the arg counts
                expect_argument_count(expected_argument_count, arguments.len(), expression.span)?;

                // lower and tailmethod
                let arguments = self.lower_arguments(
                    arguments,
                    context,
                    TypeMismatchSource::TailCallMethodArguments,
                )?;

                Ok(Some(format!(
                    "!std::object_tailmethod {} -> {method_name} ( {} )",
                    block(&receiver.code),
                    lowered_exprs_block(&arguments),
                )))
            }

            // not any call, cant be a tail call for us.
            _ => Ok(None),
        }
    }

    /// Lowers a set of argument expressions in a function context.
    ///
    /// This essentially lowers each argument expression, expecting it to contain
    /// a value.
    fn lower_arguments(
        &self,
        arguments: &[Located<Expression>],
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<Vec<LoweredExpression>> {
        arguments
            .iter()
            .map(|argument| self.expect_lowered_expression_value(argument, context, source.clone()))
            .collect()
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
        let lowered = self.lower_expression(expression, context)?;
        lowered.expect_type(expression.span, expected_type, source)?;
        Ok(lowered)
    }

    /// Lowers an expression where the expression must return a value (non-void).
    fn expect_lowered_expression_value(
        &self,
        expression: &Located<Expression>,
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<LoweredExpression> {
        let lowered = self.lower_expression(expression, context)?;
        lowered.expect_value(expression.span, source)?;
        Ok(lowered)
    }

    /// Resolves a to-be method on a reciever of some type down into the real
    /// function name in its associated namespace.
    ///
    /// The span should be where this resolution is being attempted
    ///
    /// We assume that say for some std::queue::Queue method peek(),
    /// it exists under std::queue::peek()
    fn resolve_method_name(
        &self,
        receiver_type: &TypeName,
        method: &MethodName,
        span: SourceSpan,
    ) -> PhotonResult<QualifiedName> {
        match method {
            // A fully qualified name has no resolution to be done.
            MethodName::Qualified(name) => Ok(name.resolve(&self.module.namespace)),

            // Infer from method name (under its namespace)
            MethodName::Inferred(method_name) => {
                // must be an object
                let object_name = receiver_type.object_name().ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::InferringMethodOnNonObjectType {
                            method_name: method_name.to_string(),
                            reciever_type: receiver_type.clone(),
                        },
                        span,
                    )
                })?;

                // method name exists under this namespace
                let namespace: QualifiedName = object_name.namespace().ok_or_else(|| {
                    PhotonErrorKind::error(
                        PhotonErrorKind::InferringMethodOnObjectWithoutNamespace {
                            object_name: object_name.clone(),
                        },
                        span,
                    )
                })?;

                // rebuild with method name
                let mut segments = namespace.segments;
                segments.push(method_name.clone());
                Ok(QualifiedName::new(segments))
            }
        }
    }

    /// Lowers a should-be array and should-be index expression
    /// down into a pair of the lowered array and index expressions.
    fn lower_array_index(
        &self,
        array: &Located<Expression>,
        index: &Located<Expression>,
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<(LoweredExpression, LoweredExpression)> {
        // array
        let array =
            self.expect_lowered_expression_type(array, &TypeName::Array, context, source.clone())?;

        // index
        let index = self.expect_lowered_expression_type(
            index,
            &TypeName::UInt,
            context,
            TypeMismatchSource::ArrayIndex {
                source: Box::new(source),
            },
        )?;

        Ok((array, index))
    }

    /// Declares a local of some `name` in the current `context` of the function,
    /// this local is expected to have the type of `local_type`.
    ///
    /// The span is the location of which this declaration occurs at for invalid
    /// redeclaration to another type.
    fn declare_local(
        &self,
        name: &str,
        local_type: TypeName,
        span: SourceSpan,
        context: &mut FunctionContext,
    ) -> PhotonResult<()> {
        // check if previous declaration exists with some type
        // for type mismatch
        if let Some(previous_type) = context.locals.get(name) {
            if previous_type != &local_type {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::LocalRedeclarationWithDifferentType {
                        original: previous_type.clone(),
                        new: local_type,
                        name: name.to_string(),
                    },
                    span,
                ));
            }
            return Ok(());
        }

        context.locals.insert(name.to_owned(), local_type);
        Ok(())
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

/// Converts a set of `LoweredExpression`'s to
/// a block.
fn lowered_exprs_block(exprs: &[LoweredExpression]) -> String {
    if exprs.is_empty() {
        return "{}".to_owned();
    }

    let contents = exprs
        .iter()
        .map(|value| value.code.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    block(&contents)
}

/// Tries to resolve a qualified name as a call to some `Intrinsic`,
/// returns Some(Intrinsic) if it can be resolved.
///
/// Allowed intrinsics have no namespace or are under std::
fn resolve_intrinsic(name: &QualifiedName) -> Option<Intrinsic> {
    // Name must be unqualified (no namespace)
    // and a direct call for intrinsic to map.
    let name = if name.is_unqualified() {
        name.last()
    } else if name
        .segments
        .first()
        .is_some_and(|segment| segment == "std")
    {
        // std is also allowed
        name.last()
    } else {
        return None;
    };

    // map to actual intrinsic underlying call
    match name {
        "clone" => Some(Intrinsic::Clone),
        "type" => Some(Intrinsic::TypeOf),
        "drop" => Some(Intrinsic::Drop),
        "assert" => Some(Intrinsic::Assert),
        "int_to_float" => Some(Intrinsic::IntToFloat),
        "uint_to_float" => Some(Intrinsic::UIntToFloat),
        "float_to_int" => Some(Intrinsic::FloatToInt),
        "float_to_uint" => Some(Intrinsic::FloatToUInt),
        "int_to_uint" => Some(Intrinsic::IntToUInt),
        "uint_to_int" => Some(Intrinsic::UIntToInt),
        _ => None,
    }
}

/// Returns whether or not the shape (that is whether or not
/// we actually expect a value) matches from this function.
fn tail_return_shape_matches(caller_return_type: &TypeName, callee_return_type: &TypeName) -> bool {
    let caller_is_void = caller_return_type == &TypeName::Void;
    let callee_is_void = callee_return_type == &TypeName::Void;

    caller_is_void == callee_is_void
}

/// Ensures that the expected number of arguments
/// matches the actual nubmer of arguments, erroring otherwise.
fn expect_argument_count(expected: usize, actual: usize, span: SourceSpan) -> PhotonResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(PhotonErrorKind::error(
            PhotonErrorKind::NumArgumentCallMismatch {
                expected,
                found: actual,
            },
            span,
        ))
    }
}

/// Returns the string for the assignment macro to use
/// for the corresponding type `value_type`, what kind of `target` this is,
/// and what `operator` we are assigning with.
///
/// The `span` should compromise of this full assignment
///
/// The `code` should be the direct code belonging to the rhs, which will be block'd
///
/// The `lhs` should be the thing we are assigning to.
/// It is assumed to be valid.
fn assignment_macro(
    code: &String,
    lhs: &String,
    value_type: &TypeName,
    target: AssignmentTarget,
    operator: AssignmentOperator,
    span: SourceSpan,
) -> PhotonResult<String> {
    // non-compound assignment
    if operator == AssignmentOperator::Assign {
        return Ok(match target {
            AssignmentTarget::Object {
                object_name,
                field_name,
            } => format!(
                "!std::object_set {} -> {}.{} = {}",
                block(lhs),
                object_name,
                field_name,
                block(code)
            ),
            AssignmentTarget::Local => format!("!std::set {lhs} = {}", block(code)),
            AssignmentTarget::Global => format!("!std::global_set {lhs} = {}", block(code)),
            AssignmentTarget::Array { array_index } => format!(
                "!std::array_set {} [ {} ] = {}",
                block(lhs),
                array_index,
                block(code)
            ),
        });
    }

    // Get the underlying macro for the base "set" for
    // the underlying compound assignment.
    let macro_name = match (value_type, target) {
        (TypeName::Int, AssignmentTarget::Local) => format!("int_set {lhs}"),
        (TypeName::UInt, AssignmentTarget::Local) => format!("uint_set {lhs}"),
        (TypeName::Float, AssignmentTarget::Local) => format!("float_set {lhs}"),
        (TypeName::Int, AssignmentTarget::Global) => format!("int_global_set {lhs}"),
        (TypeName::UInt, AssignmentTarget::Global) => format!("uint_global_set {lhs}"),
        (TypeName::Float, AssignmentTarget::Global) => format!("float_global_set {lhs}"),
        (
            TypeName::Int,
            AssignmentTarget::Object {
                object_name,
                field_name,
            },
        ) => format!(
            "!std::object_int_set {} -> {}.{}",
            block(lhs),
            object_name,
            field_name,
        ),
        (
            TypeName::UInt,
            AssignmentTarget::Object {
                object_name,
                field_name,
            },
        ) => format!(
            "!std::object_uint_set {} -> {}.{}",
            block(lhs),
            object_name,
            field_name,
        ),
        (
            TypeName::Float,
            AssignmentTarget::Object {
                object_name,
                field_name,
            },
        ) => format!(
            "!std::object_float_set {} -> {}.{}",
            block(lhs),
            object_name,
            field_name,
        ),
        (TypeName::Int, AssignmentTarget::Array { array_index }) => format!(
            "!std::!std::array_int_set {} [ {} ]",
            block(lhs),
            array_index,
        ),
        (TypeName::UInt, AssignmentTarget::Array { array_index }) => format!(
            "!std::!std::array_uint_set {} [ {} ]",
            block(lhs),
            array_index,
        ),
        (TypeName::Float, AssignmentTarget::Array { array_index }) => format!(
            "!std::!std::array_float_set {} [ {} ]",
            block(lhs),
            array_index,
        ),

        // no valid ops for this type
        (_, _) => {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::CompoundAssignmentOperatorTypeMismatch {
                    type_name: value_type.clone(),
                    operator,
                },
                span,
            ));
        }
    };

    // these operators are defined for the int types but not float.
    if value_type == &TypeName::Float
        && matches!(
            operator,
            AssignmentOperator::ShiftLeftAssign
                | AssignmentOperator::ShiftRightAssign
                | AssignmentOperator::BitwiseAndAssign
                | AssignmentOperator::BitwiseOrAssign
                | AssignmentOperator::BitwiseXorAssign
        )
    {
        return Err(PhotonErrorKind::error(
            PhotonErrorKind::CompoundAssignmentOperatorTypeMismatch {
                type_name: value_type.clone(),
                operator,
            },
            span,
        ));
    }

    Ok(format!("!std::{macro_name} {operator} {}", block(code)))
}

/// Ensures an object, with the signature `object` contains
/// a field with the name `field`, otherwise erroring
fn expect_object_field<'a>(
    object: &'a ObjectSignature,
    field: &str,
    span: SourceSpan,
) -> PhotonResult<&'a TypeName> {
    object.field_type(field).ok_or_else(|| {
        PhotonErrorKind::error(
            PhotonErrorKind::UnknownObjectField {
                name: object.name.clone(),
                field: field.to_string(),
            },
            span,
        )
    })
}
