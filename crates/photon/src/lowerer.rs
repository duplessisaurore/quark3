//! lowerer.rs
//!
//! This is the lowering component of the `Photon3` sugar layer,
//! responsible for:
//!
//! - taking the full ast
//! - resolving it down into std.b3 source code using its macros
//! - resolving types (simply)
//! - resolving whether things produce values or not and handling void appropriately with drop

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

use crate::{
    ast::{
        AssignmentOperator, BinaryOperator, Expression, FunctionDeclaration, Located, MethodName,
        Module, ObjectDeclaration, Parameter, QualifiedName, SimpleStatement, SourceSpan,
        Statement, StepOperator, TopLevelItem, TypeName, TypeSubstitutionMap, UnaryOperator,
    },
    errors::{PhotonError, PhotonErrorKind, PhotonResult, TypeMismatchSource},
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
pub struct LoweredExpression {
    code: String,
    value_type: TypeName,
}

/// Intrinsics, these lower directly as some function
/// call to a std.b3 construct.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Intrinsic {
    Clone,
    TypeOf,
    Drop,
    Assert,
    ToFloat,
    ToInt,
    ToUInt,
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

    // Concrete types for the generic parameters of this function instance.
    //
    // This is used for the monomorphisation of the functions similar to cpp
    substitution: TypeSubstitutionMap,

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

    // Type parameters and normal parameters
    pub type_parameters: Vec<String>,
    pub parameters: Vec<TypeName>,
    pub return_type: TypeName,
}

/// One defined object in the symbol table that can be referred to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSignature {
    pub name: QualifiedName,
    pub type_parameters: Vec<String>,
    pub fields: Vec<(String, TypeName)>,
}

/// The actual lowerer itself, this is responsible
/// for handling the lowering from the produced `ast` to
/// boson3 source code with std.b3 included.
pub struct Lowerer<'symbols, 'source_map, 'module, 'monomorphs> {
    /// All of the introduced names/elements that
    /// can be referred to by a symbol in the current context
    /// globally (see: `FunctionContext`)
    symbols: &'symbols SymbolTable,
    source_map: &'source_map SourceMap,
    module: &'module Module,

    // The monomorphisations currently available to the lowerer
    //
    // Because we may discover new monomorphisations to make we need to hold
    // a mutable reference to this
    monomorphisations: &'monomorphs mut Monomorphisations,
}

/// One registered function instance in the work list of the lowerer coordinator.
///
/// The first pass of all modules puts all these specialisations used in a work-list
/// which then we lower in a second pass for the real function bodies.
#[derive(Debug, Clone)]
struct Specialisation {
    /// Index of the defining module in the coordinator's inputs that
    /// we can map back onto to get the module this was declalred with
    module_index: usize,

    /// Index of the generic function declaration in that module's top level items
    item_index: usize,

    /// the full qualified name to refer to the specific instance/specialisation of this
    /// function with e.g blah::add_mono0
    name: QualifiedName,

    /// Resolved types in the original function's type-parameter order for this specialisation
    type_arguments: Vec<TypeName>,
}

/// Registry for all monomorphisations and instances used
/// for all of the modules being lowered.
///
/// We need to share it so modules can call instanced functions in other modules.
#[derive(Debug, Clone)]
struct Monomorphisations {
    /// original generic function name to where it is in the input modules and in the top
    /// level items of that module
    templates: HashMap<QualifiedName, (usize, usize)>,

    /// original function name and concrete types mapping to concrete qualified name for that instance
    ///
    /// basically a cache for lookup so we can map back to the same instance for same type args
    names: HashMap<(QualifiedName, Vec<TypeName>), QualifiedName>,

    /// names already occupied by functions, objects, globals, capabilities,
    /// or generated instances etc., we dont want our generated monomorph instances to conflict! KABOOOOOMYMMM if they do
    /// and head many scratch :(
    reserved_names: HashSet<QualifiedName>,

    /// work list of all registered instances of a function specialisation which produces
    /// a monomorph instance
    ///
    /// we push these during first pass, resolve in second pass
    pending: Vec<Specialisation>,

    /// next numeric suffix to try when generating a `__mono_` name for a specific
    /// monomorp instance of a function
    next_name: usize,
}

impl Monomorphisations {
    /// Collect generic definitions for all monomorphisation templates
    ///
    /// `symbols` supplies function, object, and global names that we want to avoid kabooming with
    fn new(symbols: &SymbolTable, inputs: &[(&Module, &SourceMap)]) -> Self {
        // make a new empty registry with reserved objects, gllobals and function
        // names so we dont accidentally conflict with them.
        //
        // capabilities are added in the later pass
        let mut state = Self {
            templates: HashMap::new(),
            names: HashMap::new(),
            reserved_names: symbols
                .functions
                .keys()
                .chain(symbols.objects.keys())
                .chain(symbols.globals.keys())
                .cloned()
                .collect(),
            pending: Vec::new(),
            next_name: 0,
        };

        // go through each module
        for (module_index, (module, _)) in inputs.iter().enumerate() {
            for (item_index, item) in module.items.iter().enumerate() {
                // add capabilities to reserved names too, and store the function template for this generic function
                match &item.value {
                    TopLevelItem::Function(function) if !function.type_parameters.is_empty() => {
                        let name = QualifiedName::from_text(&function.name)
                            .qualify_from(&module.namespace);

                        state.templates.insert(name, (module_index, item_index));
                    }
                    TopLevelItem::Capability(capability) => {
                        state.reserved_names.insert(
                            QualifiedName::from_text(&capability.name)
                                .qualify_from(&module.namespace),
                        );
                    }
                    _ => {}
                }
            }
        }
        state
    }

    /// Return the emitted name for a generic call
    ///
    /// This will automatically handle mappings to the same instance generated
    /// if the type arguments and function called r the same, otherwise generating
    /// a new specialisation of the function.
    fn register(
        &mut self,
        function: &QualifiedName,
        type_arguments: Vec<TypeName>,
        span: SourceSpan,
    ) -> PhotonResult<QualifiedName> {
        // Find existing instance if it exists
        let key = (function.clone(), type_arguments.clone());
        if let Some(name) = self.names.get(&key) {
            return Ok(name.clone());
        }

        // Try find the template for this function which we are generating with
        let &(module_index, item_index) = self.templates.get(function).ok_or_else(|| {
            PhotonErrorKind::error(
                PhotonErrorKind::MissingFunctionTemplate {
                    name: function.clone(),
                },
                span,
            )
        })?;

        // ensure all of the types are concretised for this specific instance we are registsering,
        // else kaboom, (aka basically ensrue no generic parameters are left)
        let mut types: Vec<&TypeName> = type_arguments.iter().collect();
        while let Some(current_check_type) = types.pop() {
            match current_check_type {
                // unresolved generic parameter of this specliasation
                TypeName::GenericParameter(parameter) => {
                    return Err(PhotonErrorKind::error(
                        PhotonErrorKind::UnresolvedGenericParameterDuringMonomorph {
                            parameter: parameter.clone(),
                            function_name: function.clone(),
                        },
                        span,
                    ));
                }
                // maybe will contain an unresolved sub-param
                TypeName::Applied {
                    constructor,
                    arguments,
                } => {
                    types.push(constructor.as_ref());
                    types.extend(arguments);
                }
                _ => {}
            }
        }

        // Generate new monomorph instance name for this function
        //
        // we essentially loop around this name __mono__<next_name> until
        // we hit a non-reserved name.
        let name = loop {
            let mut name = function.clone();
            let new_name = format!("{}__mono_{}", function.last(), self.next_name);

            // update name with new_name
            *name
                .segments
                .last_mut()
                .expect("function has a name, should be handled by the parser") = new_name;

            self.next_name += 1;

            // if its a reserved name try the next index
            if self.reserved_names.insert(name.clone()) {
                break name;
            }
        };

        // Register before lowering the body so recursive calls reuse this name.
        self.names.insert(key, name.clone());

        // push specialisation for second pass to resolve
        self.pending.push(Specialisation {
            module_index,
            item_index,
            name: name.clone(),
            type_arguments,
        });

        Ok(name)
    }
}

/// Lower all Photon3 modules together
///
/// We need to lower them all together because we need to consider monomorphisations
/// across modules.
///
/// # Errors
///
/// This will error in many ways, for example if lowering fails on one module, or
/// if monomorphisations have an issue etc.
///
/// See `PhotonError` for specific lowering errors.
///
/// The error return type is which specific input module (usize index into that) produced
/// this ereror, and the actual error for that module.
pub fn lower_modules(
    symbols: &SymbolTable,
    inputs: &[(&Module, &SourceMap)],
) -> Result<Vec<LoweredModule>, (usize, PhotonError)> {
    // Create a new monomorph regisitry for all of these modules.
    let mut monomorphisations = Monomorphisations::new(symbols, inputs);

    // And full output for each module
    let mut outputs = Vec::with_capacity(inputs.len());

    // Lower each module initially
    //
    // this will basically register eacah monomorphisation specialisation into
    // `pending` in `monomorphisations`
    for (index, &(module, source_map)) in inputs.iter().enumerate() {
        let mut lowerer = Lowerer::new(symbols, source_map, module, &mut monomorphisations);
        outputs.push(
            lowerer
                .lower_module_items()
                .map_err(|error| (index, error))?,
        );
    }

    // recursively keep resolving all of the specialisations in monomorphisations until
    // we are done with all monomorph instance generation
    loop {
        // get next instance to resolve, otherwise we are done
        let instance = monomorphisations.pending.pop();
        let Some(instance) = instance else { 
            break 
        };

        // get the specific module and source map this specialisation belongs to
        let (module, source_map) = inputs[instance.module_index];

        // get the actual function
        let item = &module.items[instance.item_index];
        let TopLevelItem::Function(function) = &item.value else {
            unreachable!("only function declarations are registered as templates by `register` and during lowering bleh");
        };

        // the specific type arguments to use during this specialisation for all the type parameters
        let substitution = function
            .type_parameters
            .iter()
            .cloned()
            .zip(instance.type_arguments)
            .collect();


        // lower this specific function with these specific type arguments
        let mut lowerer = Lowerer::new(symbols, source_map, module, &mut monomorphisations);
        let code = lowerer
            .lower_function(function, instance.name.last(), substitution)
            .map_err(|error| (instance.module_index, error))?;

        // add to the output for this specific module
        let output = &mut outputs[instance.module_index].contents;
        output.push('\n');
        output.push_str(&lowerer.with_source_location(item.span, code));
    }

    Ok(outputs)
}

impl LoweredExpression {
    /// Creates a new lowered expression, this should be some
    /// code that produces a value of some type.
    ///
    /// A `Void` type expression is one which is assumed to produce no value
    /// on the stack.
    pub fn value(code: impl Into<String>, value_type: TypeName) -> Self {
        Self {
            code: code.into(),
            value_type,
        }
    }

    /// Returns a lowered expresion that produces no value, essentially
    /// an expression of the Void type.
    pub fn no_value(code: impl Into<String>) -> Self {
        Self::value(code, TypeName::Void)
    }

    /// Returns whether or not this lowered expression produces a value
    /// e.g it is of the `Void` type.
    fn produces_value(&self) -> bool {
        self.value_type != TypeName::Void
    }

    /// Validates that this lowered expression has some type `expected`
    /// otherwise returns an error in the type mismatch source of `source`
    fn expect_type(
        &self,
        span: SourceSpan,
        expected: &TypeName,
        source: TypeMismatchSource,
    ) -> PhotonResult<()> {
        expect_type_accepts(expected, &self.value_type, source, span)
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
}

impl ObjectSignature {
    /// Returns the type of a field based on it's name (if it exists),
    /// otherwise None
    pub fn field_type(&self, field_name: &str) -> Option<&TypeName> {
        self.fields
            .iter()
            .find_map(|(name, field_type)| (name == field_name).then_some(field_type))
    }

    /// Returns a subsitution map which represents all of the substitutions done
    /// to create this instance of this applied object for this object signature type.
    fn substitution_for_instance(
        &self,
        instance_type: &TypeName,
        span: SourceSpan,
    ) -> PhotonResult<TypeSubstitutionMap> {
        // If there are no type parameters to this object signature then we have an empty map
        if self.type_parameters.is_empty() {
            if instance_type.object_name() == Some(&self.name) {
                return Ok(TypeSubstitutionMap::new());
            }
        }

        // We have type parameters so this instance must be an Applied {}
        if let TypeName::Applied {
            constructor,
            arguments,
        } = instance_type
        {
            // Number of instance parameters must the number of expected
            // type parameters for this object type/sig
            if constructor.object_name() == Some(&self.name)
                && arguments.len() == self.type_parameters.len()
            {
                return Ok(self
                    .type_parameters
                    .iter()
                    .cloned()
                    .zip(arguments.iter().cloned())
                    .collect());
            }
        }

        Err(PhotonErrorKind::error(
            PhotonErrorKind::InvalidGenericInstance {
                instance_type: instance_type.clone(),
                signature: self.name.clone(),
            },
            span,
        ))
    }

    /// Returns the type of a field on this instance based on
    /// the name of the field.
    ///
    /// # Errors
    ///
    /// The instance must be a valid generic instance of this object.
    fn field_type_for_instance(
        &self,
        instance_type: &TypeName,
        field_name: &str,
        span: SourceSpan,
    ) -> PhotonResult<TypeName> {
        // get the subsitution map to resolve string -> type
        let substitution = self.substitution_for_instance(instance_type, span)?;

        let field_type = self.field_type(field_name).ok_or_else(|| {
            PhotonErrorKind::error(
                PhotonErrorKind::UnknownObjectField {
                    name: self.name.clone(),
                    field: field_name.to_string(),
                },
                span,
            )
        })?;

        Ok(field_type.substitute(&substitution))
    }

    /// Instantiates a concrete instance of this object from the signature
    /// using the arguments to the object's fields and types.
    ///
    /// This validates the actual arguments matches the instantiated types.
    ///
    /// (as in the type of the object in the concrete instance).
    fn instantiate_object_type(
        &self,
        explicit_type_arguments: &[TypeName],
        actual_arguments: &[LoweredExpression],
        span: SourceSpan,
    ) -> PhotonResult<TypeName> {
        // Make sure we have enough arguments to the fields
        expect_argument_count(self.fields.len(), actual_arguments.len(), span)?;

        // More explicit type args than type params are also not allowed
        // as it can be difficutl to understand why things arent working
        if explicit_type_arguments.len() > self.type_parameters.len() {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::TooManyExplicitTypeParams {
                    name: self.name.clone(),
                    max: self.type_parameters.len(),
                    found: explicit_type_arguments.len(),
                },
                span,
            ));
        }

        // We build a subsitution map from `explicit_type_params` now
        let mut substitution = TypeSubstitutionMap::new();
        for (parameter, argument) in self.type_parameters.iter().zip(explicit_type_arguments) {
            substitution.insert(parameter.clone(), argument.clone());
        }

        // Infer any remaining type parameters from the actual arguments now, as explicit types
        // is non-inferred
        //
        // e.g Queue::<blah>(meow) explicit blah, and Queue(meow) infer from meow
        for ((field, expected), actual) in self.fields.iter().zip(actual_arguments) {
            infer_type_substitution_map(
                expected,
                &actual.value_type,
                &mut substitution,
                span,
                TypeMismatchSource::ObjectTypeInferrence {
                    field: field.to_string(),
                },
            )?;
        }

        // Ensure all type parameters could actually be inferred based on explicit/inferred args
        for parameter in &self.type_parameters {
            if !substitution.contains_key(parameter) {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnknownObjectTypeParam {
                        parameter: parameter.to_string(),
                        object_type: self.name.clone(),
                    },
                    span,
                ));
            }
        }

        // Substitute all the fields of the object into concrete types now
        let fields = self
            .fields
            .iter()
            .map(|(name, field_type)| (name.clone(), field_type.substitute(&substitution)))
            .collect::<Vec<_>>();

        // Ensure arguments to the fields of the object actually match their concrete types
        for ((field_name, expected), actual) in fields.iter().zip(actual_arguments) {
            expect_type_accepts(
                expected,
                &actual.value_type,
                TypeMismatchSource::ObjectConstructorField {
                    name: self.name.clone(),
                    field: field_name.clone(),
                },
                span,
            )?;
        }

        // If there were no actual type parameters, then this is just a bare Object
        let instance_type = if self.type_parameters.is_empty() {
            TypeName::Object(self.name.clone())
        } else {
            // Otherwise make the actual applied type with these type parameters substituted
            let arguments = self
                .type_parameters
                .iter()
                .map(|parameter| substitution[parameter].clone())
                .collect();

            TypeName::Object(self.name.clone()).applied(arguments)
        };

        Ok(instance_type)
    }
}

impl FunctionSignature {
    /// Check a call and return its concrete return type and type arguments.
    ///
    /// Type arguments are ordered by the function's declared type parameters.
    /// This essentially resolves all the type params, args and return type with that in mind.
    fn check_call_and_resolve_types(
        &self,
        explicit_type_arguments: &[TypeName],
        provided_types: &[TypeName],
        span: SourceSpan,
    ) -> PhotonResult<(TypeName, Vec<TypeName>)> {
        // Ensure we have enough concrete parameters for their actual types
        expect_argument_count(self.parameters.len(), provided_types.len(), span)?;

        // More explicit type args than type params are also not allowed
        // as it can be difficutl to understand why things arent working
        if explicit_type_arguments.len() > self.type_parameters.len() {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::TooManyExplicitTypeParams {
                    name: self.name.clone(),
                    max: self.type_parameters.len(),
                    found: explicit_type_arguments.len(),
                },
                span,
            ));
        }

        // We build a subsitution map from `explicit_type_params` now
        let mut substitution = TypeSubstitutionMap::new();
        for (parameter, argument) in self.type_parameters.iter().zip(explicit_type_arguments) {
            substitution.insert(parameter.clone(), argument.clone());
        }

        // Infer any remaining type parameters from the actual arguments now, as explicit types
        // is non-inferred
        //
        // e.g func<T>(blah: T) and call func(int) should infer T = int
        for (argn, (expected, actual)) in self.parameters.iter().zip(provided_types).enumerate() {
            infer_type_substitution_map(
                expected,
                actual,
                &mut substitution,
                span,
                TypeMismatchSource::FunctionTypeInferrence { argn },
            )?;
        }

        // Ensure all type parameters could actually be inferred based on explicit/inferred args
        for parameter in &self.type_parameters {
            if !substitution.contains_key(parameter) {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnknownFunctionTypeParam {
                        parameter: parameter.to_string(),
                        function: self.name.clone(),
                    },
                    span,
                ));
            }
        }

        // Substitute all the params of the function into concrete types now
        let params = self
            .parameters
            .iter()
            .map(|param_type| param_type.substitute(&substitution))
            .collect::<Vec<_>>();

        // Ensure arguments to the parameters of the function actually match their concrete types
        for (argn, (expected, actual)) in params.iter().zip(provided_types).enumerate() {
            expect_type_accepts(
                expected,
                actual,
                TypeMismatchSource::FunctionCallArgument {
                    name: self.name.clone(),
                    argn,
                },
                span,
            )?;
        }

        // Resolve all concrete type arguments for the type params
        let type_arguments = self
            .type_parameters
            .iter()
            .map(|parameter| substitution[parameter].clone())
            .collect();

        Ok((self.return_type.substitute(&substitution), type_arguments))
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
            type_parameters: function.type_parameters.clone(),
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
            type_parameters: object.type_parameters.clone(),
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

impl<'symbols, 'source_map, 'module, 'monomorphs>
    Lowerer<'symbols, 'source_map, 'module, 'monomorphs>
{
    /// Create a new lowerer over the original content `source` that will lower
    /// all of the items in `module` down into the associated string std.b3 form.
    ///
    /// The `namespace`
    fn new(
        symbols: &'symbols SymbolTable,
        source_map: &'source_map SourceMap,
        module: &'module Module,
        monomorphisations: &'monomorphs mut Monomorphisations,
    ) -> Self {
        Self {
            symbols,
            source_map,
            module,
            monomorphisations,
        }
    }

    /// Lower all the top items of a module to their stringified `LoweredModule` form.
    ///
    /// This should only be called with monomorphisations available
    fn lower_module_items(&mut self) -> PhotonResult<LoweredModule> {
        let mut output_string = Vec::new();

        // insert original source file location for module
        output_string.push(format!("@original_source {}", self.module.source_file));

        // lower each individual TLI with smp
        for item in &self.module.items {
            output_string.push(self.lower_top_level_item(item)?);
        }

        Ok(LoweredModule {
            contents: output_string.join("\n"),
        })
    }

    /// Lowers one top level item declaration down into its code/string variant
    fn lower_top_level_item(&mut self, item: &Located<TopLevelItem>) -> PhotonResult<String> {
        let code = match &item.value {
            // namespace guaranteed to be uniq alr due to parser
            TopLevelItem::Namespace(namespace) => format!("@namespace {namespace}"),
            TopLevelItem::Requires(namespace) => format!("@requires {namespace}"),
            TopLevelItem::Entry(function) => {
                // Make sure the entry function is not a genereic function/has no type parameters.
                // else bad things will happen.
                let name = QualifiedName::from_text(function).resolve(&self.module.namespace);
                if self
                    .symbols
                    .function(&name)
                    .is_some_and(|signature| !signature.type_parameters.is_empty())
                {
                    return Err(PhotonErrorKind::error(
                        PhotonErrorKind::EntryGenericFunction { name },
                        item.span,
                    ));
                }

                format!("@entry {function}")
            }
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
            TopLevelItem::Function(function) => {
                if !function.type_parameters.is_empty() {
                    // We only emit concrete functions for
                    // generic functions on use for that specific instance
                    return Ok(String::new());
                }

                self.lower_function(function, &function.name, TypeSubstitutionMap::new())?
            }
        };

        Ok(self.with_source_location(item.span, code))
    }

    /// Lowers one function down into it's string code version.
    ///
    /// `emitted_name` should be the actual concrete emitted name of
    /// this function's instance, and `substitution` should be a substitution map
    /// used by this function to produce the real underlying concrete types
    /// of this function's instance
    fn lower_function(
        &mut self,
        function: &FunctionDeclaration,
        emitted_name: &str,
        substitution: TypeSubstitutionMap,
    ) -> PhotonResult<String> {
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
                        parameter
                            .declared_type
                            .canonicalise(&self.module.namespace)
                            .substitute(&substitution),
                    )
                })
                .collect(),
            ret_type: function
                .return_type
                .canonicalise(&self.module.namespace)
                .substitute(&substitution),
            substitution,
        };

        // lower each sub-statement of the function in the current context.
        let mut output = vec![format!("@fn {emitted_name} ({parameter_names})")];
        for statement in &function.body {
            output.push(self.lower_statement(statement, &mut context)?);
        }

        Ok(output.join("\n"))
    }

    /// Lowers one statement down into its source code, this statement is being ran
    /// in the context of the function `context`.
    fn lower_statement(
        &mut self,
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
                initialiser,
                condition,
                step,
                body,
            } => self.lower_for_statement(initialiser, condition, step, body, context)?,
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
        &mut self,
        body: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        let body = self.lower_statement_block(body, context)?;
        Ok(format!("!std::forever {}", block(&body)))
    }

    /// Lowers a statement block down into source code (a block is essentially just a Vec<Statement>)
    fn lower_statement_block(
        &mut self,
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
        let mut line_offset = 0usize;

        // Go over each linee
        for raw_line in body.split_inclusive('\n') {
            let line_with_no_newline = raw_line.strip_suffix('\n').unwrap_or(raw_line);
            let line = line_with_no_newline
                .strip_suffix('\r')
                .unwrap_or(line_with_no_newline);

            if !line.trim().is_empty() {
                // the total number of leading whitespace chars
                let leading_whitespace = line.len() - line.trim_start().len();

                // actual offset for the real line contents
                let source_offset = body_span.start + line_offset + leading_whitespace;

                let location = self.source_map.location(source_offset);

                // rebuild line with loc
                output.push(format!("@source_loc {} {}", location.line, location.column));
            }

            output.push(line.to_string());
            line_offset += raw_line.len();
        }

        Ok(output.join("\n"))
    }

    /// Lowers a foreach statement in the current function context,
    fn lower_foreach_statement(
        &mut self,
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
        let binding_type = self.resolve_type(&binding.declared_type, context);

        // ensure binding type matches array type if it has some
        if let Some(element_type) = array.value_type.array_element_type() {
            expect_type_accepts(
                element_type,
                &binding_type,
                TypeMismatchSource::ForEachBinding,
                span,
            )?;
        }

        // local binding is declared in this context for the body
        self.declare_local(&binding.name, binding_type, span, context)?;
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
        &mut self,
        initialiser: &Option<Located<SimpleStatement>>,
        condition: &Located<Expression>,
        step: &Box<Option<Located<SimpleStatement>>>,
        body: &Vec<Located<Statement>>,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // initialiser
        let initialiser = initialiser
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
            block(&initialiser),
            block(&condition.code),
            block(&step),
            block(&body)
        ))
    }

    /// Lowers a dowhile statement in the current function context,
    fn lower_dowhile_statement(
        &mut self,
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
        &mut self,
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
        &mut self,
        expression: &Located<Expression>,
        context: &FunctionContext,
    ) -> PhotonResult<String> {
        let expression = self.lower_expression(expression, context)?;

        // void should auto-drop per fn comment, and we dont want to stack underflow
        if !expression.produces_value() {
            Ok(expression.code)
        } else {
            Ok(format!("!std::drop {}", block(&expression.code)))
        }
    }

    //// Lowers a simple statement in the current function context,
    fn lower_simple_statement(
        &mut self,
        statement: &SimpleStatement,
        span: SourceSpan,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // get code out from statement type
        let code = match &statement {
            SimpleStatement::Let {
                name,
                type_annotation,
                initialiser,
            } => self.lower_let_statement(name, type_annotation, initialiser, span, context)?,
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

        Ok(code)
    }

    //// Lowers an assignment statement in the current function context
    fn lower_assignment(
        &mut self,
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
                    // validate assignment matches existing type
                    value.expect_type(span, local_type, TypeMismatchSource::AssignmentRHS)?;

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

                value.expect_type(span, global_type, TypeMismatchSource::AssignmentRHS)?;

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

                // Ensure this object actually has this field and get its type
                let field_type =
                    object.field_type_for_instance(&receiver.value_type, field, span)?;

                // ensure value matches field type
                value.expect_type(span, &field_type, TypeMismatchSource::AssignmentRHS)?;

                return assignment_macro(
                    &value.code,
                    &receiver.code,
                    &field_type,
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

                let element_type = array.value_type.array_element_type();

                // If the array has a known element type, normal assignment
                // must match that type.
                if let Some(element_type) = element_type {
                    value.expect_type(span, element_type, TypeMismatchSource::AssignmentRHS)?;
                }

                // Compound assignment requires we know the underlying array type,
                // otherwise we can't really trust it
                if operator != AssignmentOperator::Assign && element_type.is_none() {
                    return Err(PhotonErrorKind::error(
                        PhotonErrorKind::CompoundAssignmentOperatorTypeMismatch {
                            type_name: TypeName::Array,
                            operator,
                        },
                        span,
                    ));
                }

                // Get the type of the element for compound assignment cases,
                // which should be what we are using for the assignment macro.
                //
                // if there is no element type (such as in assignment), default
                // to value type because we overwrite so it doesn't matter to much
                let assignment_type = element_type
                    .cloned()
                    .unwrap_or_else(|| value.value_type.clone());

                return assignment_macro(
                    &value.code,
                    &array.code,
                    &assignment_type,
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
        &mut self,
        name: &str,
        type_annotation: &Option<TypeName>,
        initialiser: &Located<Expression>,
        span: SourceSpan,
        context: &mut FunctionContext,
    ) -> PhotonResult<String> {
        // The initialiser should return a value (which matches the annotation if req)
        let initialiser = match type_annotation {
            Some(decl_type) => self.expect_lowered_expression_type(
                initialiser,
                decl_type,
                context,
                TypeMismatchSource::LetLocalInitialiser,
            )?,
            None => self.expect_lowered_expression_value(
                initialiser,
                context,
                TypeMismatchSource::LetLocalInitialiser,
            )?,
        };

        // The local type is from the initialiser
        let local_type = initialiser.value_type.canonicalise(&self.module.namespace);

        self.declare_local(name, local_type, span, context)?;
        Ok(format!("!std::let {name} = {}", block(&initialiser.code)))
    }

    /// Lowers an if statement in the current function context,
    fn lower_if_statement(
        &mut self,
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
        &mut self,
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
        &mut self,
        expression: &Located<Expression>,
        expected_return_type: &TypeName,
        context: &FunctionContext,
    ) -> PhotonResult<Option<String>> {
        let span = expression.span;

        match &expression.value {
            Expression::Call {
                callee,
                type_arguments,
                arguments,
            } => {
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

                // ensure argument count matches else error
                expect_argument_count(
                    signature.parameters.len(),
                    arguments.len(),
                    expression.span,
                )?;

                // lower arguments
                let arguments = self.lower_arguments(
                    arguments,
                    context,
                    TypeMismatchSource::TailCallArguments,
                )?;

                // resolve type arguments
                let explicit_type_arguments = type_arguments
                    .iter()
                    .map(|explicit_type| self.resolve_type(explicit_type, context))
                    .collect::<Vec<_>>();

                // types of arguments
                let actual_types = arguments
                    .iter()
                    .map(|argument| argument.value_type.clone())
                    .collect::<Vec<_>>();

                // validate all argument types/explicit types against signature and get return type
                let (resolved_name, resolved_return_type) = self.resolve_function_call(
                    signature,
                    &explicit_type_arguments,
                    &actual_types,
                    span,
                )?;

                // Tail-calling will bypass our normalisation of void/non-void with drops,
                // so make sure the types agree (e.g we don't drop a non-void thing).
                if !tail_call_return_type_matches(expected_return_type, &resolved_return_type) {
                    return Ok(None);
                }

                Ok(Some(format!(
                    "!std::tailcall {resolved_name} ( {} )",
                    lowered_exprs_block(&arguments),
                )))
            }

            Expression::MethodCall {
                receiver,
                method,
                type_arguments,
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
                if receiver.value_type.is_array() && matches!(method, MethodName::Inferred(_)) {
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
                            method_type: receiver.value_type.clone(),
                        },
                        expression.span,
                    )
                })?;

                // The receiver is the first function argument, but it does not
                // appear in the surface argument list.
                let expected_argument_count = signature.parameters.len().saturating_sub(1);

                // ensure the surface arg count matches the arg counts
                expect_argument_count(expected_argument_count, arguments.len(), expression.span)?;

                // lower arguments
                let arguments = self.lower_arguments(
                    arguments,
                    context,
                    TypeMismatchSource::TailCallMethodArguments,
                )?;

                // resolve type arguments
                let explicit_type_arguments = type_arguments
                    .iter()
                    .map(|explicit_type| self.resolve_type(explicit_type, context))
                    .collect::<Vec<_>>();

                // types of arguments
                let mut actual_types = Vec::with_capacity(arguments.len() + 1);
                actual_types.push(receiver.value_type.clone());
                actual_types.extend(arguments.iter().map(|argument| argument.value_type.clone()));

                // validate all argument types/explicit types against signature and get return type
                let (method_name, resolved_return_type) = self.resolve_function_call(
                    signature,
                    &explicit_type_arguments,
                    &actual_types,
                    span,
                )?;

                // Tail-calling will bypass our normalisation of void/non-void with drops,
                // so make sure the types agree (e.g we don't drop a non-void thing).
                if !tail_call_return_type_matches(expected_return_type, &resolved_return_type) {
                    return Ok(None);
                }

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

    /// Resolve a source type in the definition's namespace, then substitute the
    /// concrete arguments of the current function instance.
    fn resolve_type(&self, ty: &TypeName, context: &FunctionContext) -> TypeName {
        ty.canonicalise(&self.module.namespace)
            .substitute(&context.substitution)
    }

    /// Check a call and select the emitted function
    ///
    /// This will find the explicit monomorphised call to use
    /// for these type arguments
    fn resolve_function_call(
        &mut self,
        signature: &FunctionSignature,
        explicit_type_arguments: &[TypeName],
        actual_types: &[TypeName],
        span: SourceSpan,
    ) -> PhotonResult<(QualifiedName, TypeName)> {
        // Get the return type nad type arguments to use for this fucntion.
        let (return_type, type_arguments) =
            signature.check_call_and_resolve_types(explicit_type_arguments, actual_types, span)?;

        // If we have no type arguments, just use the normal function
        let name = if type_arguments.is_empty() {
            signature.name.clone()
        } else {
            // Otherwise, register a new monomorphisation or use an existing one
            // for these specific type arguments
            self.monomorphisations
                .register(&signature.name, type_arguments, span)?
        };

        Ok((name, return_type))
    }

    /// Lowers a set of argument expressions in a function context.
    ///
    /// This essentially lowers each argument expression, expecting it to contain
    /// a value.
    fn lower_arguments(
        &mut self,
        arguments: &[Located<Expression>],
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<Vec<LoweredExpression>> {
        arguments
            .iter()
            .map(|argument| self.expect_lowered_expression_value(argument, context, source.clone()))
            .collect()
    }

    /// Lowers exactly one argument expression in a function context.
    ///
    /// This essentially lowers the set, expecting only one argument expression back.
    fn expect_lower_single_argument(
        &mut self,
        arguments: &[Located<Expression>],
        span: SourceSpan,
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<LoweredExpression> {
        let lowered_args = self.lower_arguments(arguments, context, source)?;
        expect_argument_count(1, lowered_args.len(), span)?;

        // We know at least one exists safely
        Ok(lowered_args.into_iter().next().unwrap())
    }

    /// Lowers an expression with a certain type involved.
    ///
    /// Expects that the lowered expression returns this type,
    /// returning the corresponding typemismatch error if not matching.
    fn expect_lowered_expression_type(
        &mut self,
        expression: &Located<Expression>,
        expected_type: &TypeName,
        context: &FunctionContext,
        source: TypeMismatchSource,
    ) -> PhotonResult<LoweredExpression> {
        let expected_type = self.resolve_type(expected_type, context);

        // Array literals can use the surrounding annotation or return type,
        // including when there are no elements from which to infer a type.
        let lowered = match &expression.value {
            Expression::ArrayLiteral(elements) => {
                self.lower_array_lit_expr(elements, context, expected_type.array_element_type())?
            }
            _ => self.lower_expression(expression, context)?,
        };

        lowered.expect_type(expression.span, &expected_type, source)?;
        Ok(lowered)
    }

    /// Lowers an expression where the expression must return a value (non-void).
    fn expect_lowered_expression_value(
        &mut self,
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
        &mut self,
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

    /// Lowers a Photon3 expression into the LoweredExpression holding the code
    /// used to produce the expression's value and the type of this value.
    fn lower_expression(
        &mut self,
        expression: &Located<Expression>,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        let span = expression.span;

        let lowered_expr = match &expression.value {
            // literals, easy mapping
            Expression::IntLiteral(value) => {
                LoweredExpression::value(format!("!std::int {value}"), TypeName::Int)
            }
            Expression::UIntLiteral(value) => {
                LoweredExpression::value(format!("!std::uint {value}"), TypeName::UInt)
            }
            Expression::FloatLiteral(value) => {
                LoweredExpression::value(format!("!std::float {value:?}"), TypeName::Float)
            }
            Expression::BoolLiteral(value) => {
                LoweredExpression::value(format!("!std::bool {value}"), TypeName::Bool)
            }
            Expression::Name(name) => self.lower_name_expr(name, span, context)?,
            Expression::ArrayLiteral(elements) => {
                self.lower_array_lit_expr(elements, context, None)?
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
            } => self.lower_call_expr(callee, type_arguments, arguments, span, context)?,
            Expression::FieldAccess { receiver, field } => {
                self.lower_field_access(receiver, field, span, context)?
            }
            Expression::MethodCall {
                receiver,
                method,
                type_arguments,
                arguments,
            } => {
                self.lower_method_call(receiver, method, type_arguments, arguments, span, context)?
            }
            Expression::Index { array, index } => {
                self.lower_array_index_expr(array, index, context)?
            }
            Expression::Unary { operator, operand } => {
                self.lower_unary_expr(*operator, operand, span, context)?
            }
            Expression::Binary {
                left,
                operator,
                right,
            } => self.lower_binary_expr(left, *operator, right, span, context)?,
            Expression::Conditional {
                condition,
                when_true,
                when_false,
            } => self.lower_conditional_expr(condition, when_true, when_false, context)?,
            Expression::Cast {
                expression,
                target_type,
            } => self.lower_cast_expr(expression, target_type, span, context)?,
            Expression::Boson3 {
                declared_type,
                body,
                body_span,
            } => {
                // map into source and return as expr of type, we dont normalise bcz it should do what it says.
                let boson3_source = self.lower_boson3_statement(body, *body_span)?;
                let expr_type = self.resolve_type(declared_type, context);
                LoweredExpression::value(boson3_source, expr_type)
            }
        };

        Ok(lowered_expr)
    }

    /// Lowers one name in the position of an expression in the current `context`
    /// of the function.
    ///
    /// This essentially tries to just resolve the name.
    fn lower_name_expr(
        &self,
        name: &QualifiedName,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        if name.is_unqualified() {
            // simple unit
            if name.last() == "unit" {
                return Ok(LoweredExpression::value("!std::unit", TypeName::Unit));
            }

            // otherwise maybe a local of this function
            if let Some(local_type) = context.locals.get(name.last()) {
                return Ok(LoweredExpression::value(
                    format!("!std::get {}", name.last()),
                    local_type.clone(),
                ));
            }
        }

        // isnt a local or unit, try map global.
        let global_name = name.resolve(&self.module.namespace);
        if let Some(global_type) = self.symbols.global(&global_name) {
            return Ok(LoweredExpression::value(
                format!("!std::global_get {global_name}"),
                global_type.clone(),
            ));
        }

        // Couldn't be found
        Err(PhotonErrorKind::error(
            PhotonErrorKind::UnknownName {
                name: name.to_string(),
            },
            span,
        ))
    }

    /// Lowers one expression reinterpretation/cast down in the current context of a function
    fn lower_cast_expr(
        &mut self,
        expression: &Located<Expression>,
        target_type: &TypeName,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // target value we are casting
        let value = self.expect_lowered_expression_value(
            expression,
            context,
            TypeMismatchSource::CastTarget,
        )?;

        // Target type we are casting to with this new expression
        let target_type = self.resolve_type(target_type, context);

        // Void casts are illegal, as thats just drop bruh
        if target_type == TypeName::Void {
            return Err(PhotonErrorKind::error(PhotonErrorKind::VoidCast, span));
        }

        Ok(LoweredExpression {
            code: value.code,
            value_type: target_type,
        })
    }

    /// Lowers one array index access expression in the current context of a function
    fn lower_array_index_expr(
        &mut self,
        array: &Box<Located<Expression>>,
        index: &Box<Located<Expression>>,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // lower array and index down
        let (array, index) =
            self.lower_array_index(array, index, context, TypeMismatchSource::ArrayIndexExpr)?;

        let element_type = array
            .value_type
            .array_element_type()
            .cloned()
            .unwrap_or(TypeName::Any);

        // simple get at index
        Ok(LoweredExpression::value(
            format!(
                "!std::array_get {} [ {} ]",
                block(&array.code),
                block(&index.code)
            ),
            element_type,
        ))
    }

    /// Lowers one conditional  expression in the current context of a function
    fn lower_conditional_expr(
        &mut self,
        condition: &Box<Located<Expression>>,
        when_true: &Box<Located<Expression>>,
        when_false: &Box<Located<Expression>>,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // condition
        let condition = self.expect_lowered_expression_type(
            condition,
            &TypeName::Bool,
            context,
            TypeMismatchSource::ConditionExpressionOfTheConditionalExpression,
        )?;

        // branches when true
        let when_true = self.expect_lowered_expression_value(
            when_true,
            context,
            TypeMismatchSource::TrueBranchCondExpr,
        )?;

        let when_false = self.expect_lowered_expression_value(
            when_false,
            context,
            TypeMismatchSource::FalseBranchCondExpr,
        )?;

        // resulting type can only be known if both types match (else its either)
        let result_type = if when_true.value_type == when_false.value_type {
            when_true.value_type.clone()
        } else {
            TypeName::Any
        };

        Ok(LoweredExpression::value(
            format!(
                "!std::choose {} ? {} : {}",
                block(&condition.code),
                block(&when_true.code),
                block(&when_false.code)
            ),
            result_type,
        ))
    }

    /// Lowers one unary expression in the current context of a function
    fn lower_unary_expr(
        &mut self,
        operator: UnaryOperator,
        operand: &Box<Located<Expression>>,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // Get the operand we are applying the unary expression to
        let operand = self.expect_lowered_expression_value(
            operand,
            context,
            TypeMismatchSource::UnaryOperand,
        )?;

        let operand_type = &operand.value_type;

        // Get the associated macro name and text for the operator along with the resulting type
        let (macro_name, operator_text, result_type) = match (operator, operand_type) {
            (UnaryOperator::Negate, TypeName::Int) => ("int_unary", "-", TypeName::Int),
            (UnaryOperator::Negate, TypeName::Float) => ("float_unary", "-", TypeName::Float),
            (UnaryOperator::LogicalNot, TypeName::Bool) => ("bool_unary", "!", TypeName::Bool),
            (UnaryOperator::BitwiseNot, TypeName::Int) => ("int_unary", "~", TypeName::Int),
            (UnaryOperator::BitwiseNot, TypeName::UInt) => ("uint_unary", "~", TypeName::UInt),
            _ => {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::UnaryOperatorTypeMismatch {
                        type_name: operand_type.clone(),
                        operator,
                    },
                    span,
                ));
            }
        };

        Ok(LoweredExpression::value(
            format!(
                "!std::{macro_name} {operator_text} {}",
                block(&operand.code)
            ),
            result_type,
        ))
    }

    /// Lowers one array literal expression in the current context of a function
    fn lower_array_lit_expr(
        &mut self,
        elements: &Vec<Located<Expression>>,
        context: &FunctionContext,
        expected_element_type: Option<&TypeName>,
    ) -> PhotonResult<LoweredExpression> {
        // Propagate a known element type into nested literals of this literal if
        // we have a known type, as it should match.
        let values = if let Some(expected) = expected_element_type {
            elements
                .iter()
                .map(|element| {
                    let value = self.expect_lowered_expression_type(
                        element,
                        expected,
                        context,
                        TypeMismatchSource::ArrayLiteralExpression,
                    )?;
                    value.expect_value(element.span, TypeMismatchSource::ArrayLiteralExpression)?;
                    Ok(value)
                })
                .collect::<PhotonResult<Vec<_>>>()?
        } else {
            self.lower_arguments(
                elements,
                context,
                TypeMismatchSource::ArrayLiteralExpression,
            )?
        };

        // get the type of all the elements for the overall type of the array
        // (ALL if non heterogenous)
        let element_type = match (expected_element_type, values.first()) {
            (Some(expected), _) => expected.clone(),
            (None, None) => TypeName::Any,

            // All types must be the same
            (None, Some(first))
                if values
                    .iter()
                    .all(|value| value.value_type == first.value_type) =>
            {
                first.value_type.clone()
            }
            (None, Some(_)) => TypeName::Any,
        };

        Ok(LoweredExpression::value(
            format!(
                "!std::array {} ( {} )",
                values.len(),
                lowered_exprs_block(&values)
            ),
            TypeName::array_of(element_type),
        ))
    }

    /// Lowers one call expression in the current context of a function
    fn lower_call_expr(
        &mut self,
        callee: &Located<Expression>,
        type_arguments: &[TypeName],
        arguments: &[Located<Expression>],
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // Calls are only permitted to straight up name callees (e.g) my_func()
        let Expression::Name(callee_name) = &callee.value else {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::InvalidNonNameCallTarget,
                span,
            ));
        };

        // Validate if the call is an intrinsic or not, and lower it if it is.
        if let Some(value) = self.lower_intrinsic_call(callee_name, arguments, span, context)? {
            return Ok(value);
        }

        // Resolve it's name, so we can find out if its a function or object call.
        let resolved_name = callee_name.resolve(&self.module.namespace);

        // Get type arguments for resolving
        let explicit_type_arguments = type_arguments
            .iter()
            .map(|explicit_type| self.resolve_type(explicit_type, context))
            .collect::<Vec<_>>();

        // Object constructor
        if let Some(object) = self.symbols.object(&resolved_name) {
            expect_argument_count(object.fields.len(), arguments.len(), span)?;

            // Lower all the arguments down to their expressions
            let arguments = self.lower_arguments(
                arguments,
                context,
                TypeMismatchSource::ObjectConstructorArguments {
                    name: resolved_name.clone(),
                },
            )?;

            // type check all arguments and get actual real type
            let constructed_object_type =
                object.instantiate_object_type(&explicit_type_arguments, &arguments, span)?;

            // actual constructor
            return Ok(LoweredExpression::value(
                format!(
                    "!std::object_new {resolved_name} ( {} )",
                    lowered_exprs_block(&arguments)
                ),
                constructed_object_type,
            ));
        }

        // Not an object constructor, try see if its a function.
        let signature = self.symbols.function(&resolved_name).ok_or_else(|| {
            PhotonErrorKind::error(
                PhotonErrorKind::UnknownCallable {
                    name: resolved_name.clone(),
                },
                span,
            )
        })?;

        // must match the argument count.
        expect_argument_count(signature.parameters.len(), arguments.len(), span)?;

        // lower all arguments down to their expressions
        let arguments = self.lower_arguments(
            arguments,
            context,
            TypeMismatchSource::FunctionCallArguments {
                name: resolved_name.clone(),
            },
        )?;

        // get types of all the arguments
        let actual_types = arguments
            .iter()
            .map(|argument| argument.value_type.clone())
            .collect::<Vec<_>>();

        // check types of arguments and explicit type params and get
        // our actual return type and the actual name of the monomorphised function
        let (resolved_name, resolved_return_type) =
            self.resolve_function_call(signature, &explicit_type_arguments, &actual_types, span)?;

        let call_code = format!(
            "!std::call {resolved_name} ( {} )",
            lowered_exprs_block(&arguments)
        );

        Ok(resolved_return_type.normalise_to_drop(&call_code))
    }

    /// Lowers a potential intrinsic call in the current context of a function
    fn lower_intrinsic_call(
        &mut self,
        name: &QualifiedName,
        arguments: &[Located<Expression>],
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<Option<LoweredExpression>> {
        // Check and get the underlying intrinsic if this exists
        let Some(intrinsic) = resolve_intrinsic(name) else {
            return Ok(None);
        };

        // Argument to the intrinsic
        let value = self.expect_lower_single_argument(
            arguments,
            span,
            context,
            TypeMismatchSource::IntrinsicArgument { intrinsic },
        )?;

        Ok(Some(match intrinsic {
            // These dont have to check types
            Intrinsic::Clone => LoweredExpression::value(
                format!("!std::clone {}", block(&value.code)),
                value.value_type,
            ),
            Intrinsic::TypeOf => LoweredExpression::value(
                format!("!std::type {}", block(&value.code)),
                TypeName::Tag,
            ),
            Intrinsic::Drop => {
                LoweredExpression::no_value(format!("!std::drop {}", block(&value.code)))
            }

            // These do
            Intrinsic::Assert => {
                value.expect_type(span, &TypeName::Bool, TypeMismatchSource::AssertCondition)?;
                LoweredExpression::no_value(format!("!std::assert ( {} )", block(&value.code)))
            }
            Intrinsic::ToFloat => {
                // Get specific to_float macro from lhs type
                let specific_macro = match value.value_type {
                    TypeName::Int => "int_to_float",
                    TypeName::UInt => "uint_to_float",
                    _ => {
                        return Err(PhotonErrorKind::error(
                            PhotonErrorKind::InvalidTypeForConversionIntrinsic {
                                intrinsic,
                                found: value.value_type.clone(),
                            },
                            span,
                        ));
                    }
                };

                LoweredExpression::value(
                    format!("!std::{specific_macro} {}", block(&value.code)),
                    TypeName::Float,
                )
            }
            Intrinsic::ToInt => {
                // Get specific to_int macro from lhs type
                let specific_macro = match value.value_type {
                    TypeName::Float => "float_to_int",
                    TypeName::UInt => "uint_to_int",
                    _ => {
                        return Err(PhotonErrorKind::error(
                            PhotonErrorKind::InvalidTypeForConversionIntrinsic {
                                intrinsic,
                                found: value.value_type.clone(),
                            },
                            span,
                        ));
                    }
                };

                LoweredExpression::value(
                    format!("!std::{specific_macro} {}", block(&value.code)),
                    TypeName::Int,
                )
            }
            Intrinsic::ToUInt => {
                // Get specific to_int macro from lhs type
                let specific_macro = match value.value_type {
                    TypeName::Float => "float_to_uint",
                    TypeName::Int => "int_to_uint",
                    _ => {
                        return Err(PhotonErrorKind::error(
                            PhotonErrorKind::InvalidTypeForConversionIntrinsic {
                                intrinsic,
                                found: value.value_type.clone(),
                            },
                            span,
                        ));
                    }
                };

                LoweredExpression::value(
                    format!("!std::{specific_macro} {}", block(&value.code)),
                    TypeName::UInt,
                )
            }
        }))
    }

    /// Lowers a method call in the current context of a function
    fn lower_method_call(
        &mut self,
        receiver: &Located<Expression>,
        method: &MethodName,
        type_arguments: &[TypeName],
        arguments: &[Located<Expression>],
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // LHS of the method
        let receiver = self.expect_lowered_expression_value(
            receiver,
            context,
            TypeMismatchSource::MethodCallReceiver,
        )?;

        // Check if it's a special array method, which uses internal methods
        // rather than calling a real method function
        if matches!(method, MethodName::Inferred(_)) {
            if let Some(array_method) =
                self.lower_array_method(&receiver, method, arguments, span, context)?
            {
                return Ok(array_method);
            }
        }

        // Resolve the method name down to find the function signature
        let method_name = self.resolve_method_name(&receiver.value_type, method, span)?;
        let signature = self.symbols.function(&method_name).ok_or_else(|| {
            PhotonErrorKind::error(
                PhotonErrorKind::UnknownMethod {
                    name: method_name.clone(),
                    method_type: receiver.value_type.clone(),
                },
                span,
            )
        })?;

        // Make sure the argument count matches and lower the arguments to exprs
        expect_argument_count(
            signature.parameters.len().saturating_sub(1),
            arguments.len(),
            span,
        )?;

        let arguments = self.lower_arguments(
            arguments,
            context,
            TypeMismatchSource::FunctionCallArguments {
                name: method_name.clone(),
            },
        )?;

        // get explicit type arguments to this method call
        let explicit_type_arguments = type_arguments
            .iter()
            .map(|explicit_type| self.resolve_type(explicit_type, context))
            .collect::<Vec<_>>();

        // actual types of all the method params
        let mut actual_types = Vec::with_capacity(arguments.len() + 1);
        actual_types.push(receiver.value_type.clone());
        actual_types.extend(arguments.iter().map(|argument| argument.value_type.clone()));

        // check types of arguments and explicit type params and get
        // our actual return type and concrete monomorphised name of the method
        let (method_name, resolved_return_type) =
            self.resolve_function_call(signature, &explicit_type_arguments, &actual_types, span)?;

        let call_code = format!(
            "!std::object_method {} -> {method_name} ( {} )",
            block(&receiver.code),
            lowered_exprs_block(&arguments)
        );

        Ok(resolved_return_type.normalise_to_drop(&call_code))
    }

    /// Lowers a potential array method call in the current context of a function
    fn lower_array_method(
        &mut self,
        receiver: &LoweredExpression,
        method: &MethodName,
        arguments: &[Located<Expression>],
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<Option<LoweredExpression>> {
        // Reciever type must be an array
        if !receiver.value_type.is_array() {
            return Ok(None);
        }

        // And it must be a inferred method (array is under no namespace)
        let MethodName::Inferred(method_name) = method else {
            return Ok(None);
        };

        // Get the specific element type of the array element, for returning
        // more known types rather than any
        let element_type = receiver
            .value_type
            .array_element_type()
            .cloned()
            .unwrap_or(TypeName::Any);

        // match specific macro to inferred call
        let result = match method_name.as_str() {
            "length" => {
                expect_argument_count(0, arguments.len(), span)?;
                LoweredExpression::value(
                    format!("!std::array_length {}", block(&receiver.code)),
                    TypeName::UInt,
                )
            }
            "head" => {
                expect_argument_count(0, arguments.len(), span)?;
                LoweredExpression::value(
                    format!("!std::array_head {}", block(&receiver.code)),
                    element_type,
                )
            }
            "tail" => {
                expect_argument_count(0, arguments.len(), span)?;
                LoweredExpression::value(
                    format!("!std::array_tail {}", block(&receiver.code)),
                    receiver.value_type.clone(),
                )
            }
            "append" => {
                expect_argument_count(1, arguments.len(), span)?;

                // Should have another array we are appending to us with
                let other = self.expect_lowered_expression_type(
                    &arguments[0],
                    &TypeName::Array,
                    context,
                    TypeMismatchSource::ArrayAppendArgument,
                )?;

                // Get the resulting type out
                let Some(result_type) =
                    array_append_result_type(&receiver.value_type, &other.value_type)
                else {
                    return Err(PhotonErrorKind::error(
                        PhotonErrorKind::BinaryOpMismatch {
                            operator: BinaryOperator::ArrayAppend,
                            lhs: receiver.value_type.clone(),
                            rhs: other.value_type,
                        },
                        span,
                    ));
                };

                LoweredExpression::value(
                    format!(
                        "!std::array_append {} {}",
                        block(&receiver.code),
                        block(&other.code)
                    ),
                    result_type,
                )
            }
            "prepend" => {
                let value = self.expect_lower_single_argument(
                    arguments,
                    span,
                    context,
                    TypeMismatchSource::ArrayPrependArgument,
                )?;

                expect_type_accepts(
                    &element_type,
                    &value.value_type,
                    TypeMismatchSource::ArrayPrependArgument,
                    span,
                )?;

                LoweredExpression::value(
                    format!(
                        "!std::array_prepend {} {}",
                        block(&receiver.code),
                        block(&value.code)
                    ),
                    receiver.value_type.clone(),
                )
            }
            _ => return Ok(None),
        };

        Ok(Some(result))
    }

    /// Lowers an object field access in the current context of a function
    fn lower_field_access(
        &mut self,
        receiver: &Located<Expression>,
        field: &str,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // object we are accessing
        let receiver = self.expect_lowered_expression_value(
            receiver,
            context,
            TypeMismatchSource::FieldAccessReciever {
                field: field.to_string(),
            },
        )?;

        // name and signature of the object
        let object_name = receiver.value_type.object_name().ok_or_else(|| {
            PhotonErrorKind::error(
                PhotonErrorKind::FieldAccessToNonObjectType {
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

        // Ensure this object field actually exists
        let field_type = object.field_type_for_instance(&receiver.value_type, field, span)?;

        Ok(LoweredExpression::value(
            format!(
                "!std::object_get {} -> {}.{}",
                block(&receiver.code),
                object.name,
                field
            ),
            field_type,
        ))
    }

    /// Lowers a binary operation expression in the current context of a function
    fn lower_binary_expr(
        &mut self,
        left: &Located<Expression>,
        operator: BinaryOperator,
        right: &Located<Expression>,
        span: SourceSpan,
        context: &FunctionContext,
    ) -> PhotonResult<LoweredExpression> {
        // Turn operands into actual lowered expressions first to use.
        let left =
            self.expect_lowered_expression_value(left, context, TypeMismatchSource::BinOpLHS)?;
        let right =
            self.expect_lowered_expression_value(right, context, TypeMismatchSource::BinOpRHS)?;

        // The array append operator, applies to array types
        if operator == BinaryOperator::ArrayAppend {
            // Get the resulting type out
            let Some(result_type) = array_append_result_type(&left.value_type, &right.value_type)
            else {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::BinaryOpMismatch {
                        operator: BinaryOperator::ArrayAppend,
                        lhs: left.value_type.clone(),
                        rhs: right.value_type.clone(),
                    },
                    span,
                ));
            };

            return Ok(LoweredExpression::value(
                format!(
                    "!std::array_append {} {}",
                    block(&left.code),
                    block(&right.code)
                ),
                result_type,
            ));
        }

        // Non-matching binop types, not permitted!
        if left.value_type != right.value_type {
            return Err(PhotonErrorKind::error(
                PhotonErrorKind::BinaryOpMismatch {
                    operator,
                    lhs: left.value_type,
                    rhs: right.value_type,
                },
                span,
            ));
        }

        // get the underlying expr macro to use and the result of this operation
        let (macro_name, result_type) = match left.value_type {
            TypeName::Int if operator.is_numeric() => (
                "int_expr",
                // comp 2 ints vs produce new int
                if operator.is_comparison() {
                    TypeName::Bool
                } else {
                    TypeName::Int
                },
            ),
            TypeName::UInt if operator.is_numeric() => (
                "uint_expr",
                if operator.is_comparison() {
                    TypeName::Bool
                } else {
                    TypeName::UInt
                },
            ),
            TypeName::Float if operator.is_float() => (
                "float_expr",
                if operator.is_comparison() {
                    TypeName::Bool
                } else {
                    TypeName::Float
                },
            ),
            // Booleans have OR/AND
            TypeName::Bool
                if matches!(
                    operator,
                    BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
                ) =>
            {
                ("bool_expr", TypeName::Bool)
            }
            // Tag equality
            TypeName::Tag
                if matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual) =>
            {
                ("tag_expr", TypeName::Bool)
            }
            _ => {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::BinaryOperatorUndefinedForType {
                        operator,
                        operand_type: left.value_type.clone(),
                    },
                    span,
                ));
            }
        };

        Ok(LoweredExpression::value(
            format!(
                "!std::{macro_name} {} {operator} {}",
                block(&left.code),
                block(&right.code)
            ),
            result_type,
        ))
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
pub fn block(contents: &str) -> String {
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
        "typeof" => Some(Intrinsic::TypeOf),
        "drop" => Some(Intrinsic::Drop),
        "assert" => Some(Intrinsic::Assert),
        "to_float" => Some(Intrinsic::ToFloat),
        "to_int" => Some(Intrinsic::ToInt),
        "to_uint" => Some(Intrinsic::ToUInt),
        _ => None,
    }
}

/// Returns whether or not the shape (that is whether or not
/// we actually expect a value) matches from this function.
///
/// The shape that matches is if expect & actual is Void or
/// if actual isnt void and expected accepts actual
fn tail_call_return_type_matches(expected: &TypeName, actual: &TypeName) -> bool {
    if expected == &TypeName::Void {
        actual == &TypeName::Void
    } else {
        actual != &TypeName::Void && type_accepts(expected, actual)
    }
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
            "object_int_set {} -> {}.{}",
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
            "object_uint_set {} -> {}.{}",
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
            "object_float_set {} -> {}.{}",
            block(lhs),
            object_name,
            field_name,
        ),
        (TypeName::Int, AssignmentTarget::Array { array_index }) => {
            format!("array_int_set {} [ {} ]", block(lhs), array_index,)
        }
        (TypeName::UInt, AssignmentTarget::Array { array_index }) => {
            format!("array_uint_set {} [ {} ]", block(lhs), array_index,)
        }
        (TypeName::Float, AssignmentTarget::Array { array_index }) => {
            format!("array_float_set {} [ {} ]", block(lhs), array_index,)
        }

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

impl Display for Intrinsic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let intrinsic_name = match self {
            Intrinsic::Clone => "clone",
            Intrinsic::TypeOf => "typeof",
            Intrinsic::Drop => "drop",
            Intrinsic::Assert => "assert",
            Intrinsic::ToFloat => "to_float",
            Intrinsic::ToInt => "to_int",
            Intrinsic::ToUInt => "to_uint",
        };

        write!(f, "`{intrinsic_name}`")
    }
}

/// Infer types into a type substitution map based on the supplied type for some declared type we
/// are substituting into
///
/// Essentially based on the `declared_type` and the `supplied_type` of that
/// declared type, we either see if it's in the map, and verify the type is legal (otherwise erroring)
///
/// or if its not currently in the map we set it as the first of that kind of parameter in the map.
fn infer_type_substitution_map(
    declared_type: &TypeName,
    supplied_type: &TypeName,
    map: &mut TypeSubstitutionMap,
    span: SourceSpan,
    source: TypeMismatchSource,
) -> PhotonResult<()> {
    match declared_type {
        // A simple generic parameter, say Queue<T> where declared_type is the T and
        // supplied_type is provided in the position of T
        TypeName::GenericParameter(parameter) => {
            // See if the type exists already in the map (else we insert).
            if let Some(previous_type) = map.get(parameter) {
                if type_accepts(previous_type, supplied_type)
                    && type_accepts(supplied_type, previous_type)
                {
                    // Exists in map, type accepts
                    Ok(())
                } else {
                    // Type mismatch!
                    Err(PhotonErrorKind::error(
                        PhotonErrorKind::InvalidSuppliedTypeForGenericParam {
                            expected: previous_type.clone(),
                            found: supplied_type.clone(),
                            generic_parameter: parameter.clone(),
                        },
                        span,
                    ))
                }
            } else {
                // Not in the map yet, insert for this parameter as the first time.
                map.insert(parameter.clone(), supplied_type.clone());
                Ok(())
            }
        }

        // Applied so for e.x Queue<T>,
        // if putting in a Queue<Int> should fill T as Int
        TypeName::Applied {
            constructor: declared_constructor,
            arguments: declared_arguments,
        } => {
            // If we are supplying Array for some Array<T>, this is valid
            // as this is an "any" array
            if declared_constructor.as_ref() == &TypeName::Array
                && supplied_type == &TypeName::Array
            {
                return Ok(());
            }

            // Extract the inner arguments to match against declared_type
            // in supplied_type.
            let TypeName::Applied {
                constructor: supplied_constructor,
                arguments: supplied_arguments,
            } = supplied_type
            else {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::NonAppliedSuppliedTypeForDeclaredSuppliedType {
                        supplied: supplied_type.clone(),
                    },
                    span,
                ));
            };

            // The total number of arguments must match, doesn't make much
            // sense to match C<T, A> with C<Int, Int, Int>
            if declared_arguments.len() != supplied_arguments.len() {
                return Err(PhotonErrorKind::error(
                    PhotonErrorKind::AppliedTypeInferenceArgNMismatch {
                        declared_argn: declared_arguments.len(),
                        supplied_argn: supplied_arguments.len(),
                    },
                    span,
                ));
            }

            // Match against generic constructor..
            infer_type_substitution_map(
                declared_constructor,
                supplied_constructor,
                map,
                span,
                source.clone(),
            )?;

            // Now infer against the sub things, so for C<T, A> and C<B, A> we infer T = B and A = A
            for (declared_argument, supplied_argument) in
                declared_arguments.iter().zip(supplied_arguments)
            {
                infer_type_substitution_map(
                    declared_argument,
                    supplied_argument,
                    map,
                    span,
                    source.clone(),
                )?;
            }

            Ok(())
        }

        // We are allowed to supply any non-void type to some Any
        TypeName::Any if supplied_type != &TypeName::Void => Ok(()),

        // And concrete types must accept eachother
        concrete if type_accepts(concrete, supplied_type) => Ok(()),

        // Otherwise, type mismatch!
        _ => Err(PhotonErrorKind::error(
            PhotonErrorKind::TypeMismatchSource {
                expected: declared_type.clone(),
                found: supplied_type.clone(),
                source,
            },
            span,
        )),
    }
}

/// Whether or not this type expects another type (that
/// is sthat passing this type to another type is allowed)
fn type_accepts(expected: &TypeName, actual: &TypeName) -> bool {
    match (expected, actual) {
        (TypeName::Any, actual) => actual != &TypeName::Void,

        // Arrays accept other arrays if there is no parameterised types
        // else has to be exact
        (TypeName::Array, actual) => actual.is_array(),

        // Objects accept eachother if its just a bare object name vs actual concrete type
        (TypeName::Object(expected_name), actual) => actual.object_name() == Some(expected_name),

        _ => expected == actual,
    }
}

/// Excepts that the expected type accepts the actual type, otherwise
/// errors with a TypeMismatch
fn expect_type_accepts(
    expected: &TypeName,
    actual: &TypeName,
    source: TypeMismatchSource,
    span: SourceSpan,
) -> PhotonResult<()> {
    if !type_accepts(expected, actual) {
        return Err(PhotonErrorKind::error(
            PhotonErrorKind::TypeMismatchSource {
                expected: expected.clone(),
                found: actual.clone(),
                source: source,
            },
            span,
        ));
    }

    Ok(())
}

/// Resulting type of the array append operation if these
/// are the two array types.
///
/// Returns None if the resultant type is not an Array
/// otherwise Some() of the type.
fn array_append_result_type(left: &TypeName, right: &TypeName) -> Option<TypeName> {
    if !left.is_array() || !right.is_array() {
        return None;
    }

    // A bare Array means the element type cannot be known (non-homogenous)
    // so we have to revert to that if either are array
    if left == &TypeName::Array || right == &TypeName::Array {
        return Some(TypeName::Array);
    }

    // And if they arent array, then we need to ensure their types match
    if left == right {
        return Some(left.clone());
    }

    None
}
