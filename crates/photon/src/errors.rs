//! All errors that can occur during the `Photon3`
//! sugar pass

use std::fmt::Display;

use crate::{
    ast::{
        AssignmentOperator, BinaryOperator, Located, QualifiedName, SourceSpan, StepOperator,
        TypeName, UnaryOperator,
    },
    lexer::TokenKind,
    lowerer::Intrinsic,
};

/// All possible error kinds that can occur during
/// the `Photon3` sugar pass.
#[derive(Debug)]
pub enum PhotonErrorKind {
    /// An unterminated legacy boson3 block was found.
    UnterminatedLegacyBoson3Block,

    /// A numeric literal was malformed and could not be succesfully
    /// converted to an actual Token
    ///
    /// The reason for this happening is stored in `reason`.
    MalformedNumber { reason: String },

    /// A character was not expected to be here!
    UnexpectedCharacter { character: char },

    /// A missing directive name was found here
    MissingDirectiveName,

    /// Unexpected end of file here, we should
    /// see more tokens
    UnexpectedEndOfFile,

    /// A token was found to be here that we did
    /// not expect
    UnexpectedToken {
        found: TokenKind,
        expected: TokenKind,
    },

    /// An unknown top level directive was found
    /// regarding name-wise
    UnknownTLD { name: String },

    /// Expected another parameter follow since a comma
    /// was found here
    UnexpectedEndOfParamsFollowingComma,

    /// Expected another type parameter to follow since
    /// a comma was found here
    UnexpectedEndOfTypeParamsFollowingComma,

    /// Expected another argument follow since a comma
    /// was found here
    UnexpectedEndOfArgsFollowingComma,

    /// Expected another type argument follow since a comma
    /// was found here
    UnexpectedEndOfTypeArgsFollowingComma,

    /// Expected another array element follow since a comma
    /// was found here
    UnexpectedEndOfArrayElemsFollowingComma,

    /// Global nodes when ordering in the lowerer formed a 
    /// cycle in the graph of globals
    GlobalInitialisationCycle { cycle: Vec<QualifiedName> },

    /// Unexpected directive in the current position
    UnexpectedDirective { found: String, expected: String },

    /// Invalid numeric literal for the @capability directive
    InvalidCapabilityNumber { found: TokenKind },

    /// A non expression token was found in the place of an
    /// expression
    UnexpectedNonExpression { found: TokenKind },

    /// Unexpectedly found multiple @entry declarations across
    /// modules!
    UnexpectedMultipleEntry { first: QualifiedName, second: QualifiedName },

    /// A non-step operator was found to be in the place of
    /// a step operator
    UnexpectedNonStepOperator { found: TokenKind },

    /// An unexpected non-end of statement occured here,
    /// as in we did not see a valid terminator!
    UnexpectedNonEndOfStatement { found: TokenKind },

    /// An unexpected non-boson3 statement was found here
    UnexpectedNonB3Statement { found: TokenKind },

    /// An unclosed/unterminated block was found
    UnclosedBlockFound,

    /// A duplicate namespace was declared in this module!
    DuplicateNamespace,

    /// No namespace was delcared in the module!
    NoNamespace,

    /// A duplicate global was declared with this name!
    DuplicateGlobal { name: QualifiedName },

    /// A duplicate type parameter was declared with this name!
    DuplicateTypeParameter { name: String },

    /// A duplicate function was declared with this name!
    DuplicateFunction { name: QualifiedName },

    /// A duplicate object was declared with this name!
    DuplicateObject { name: QualifiedName },

    /// An overlapping callable, either an object or a function
    /// was obth declared with the same name!
    DuplicateCallable { name: QualifiedName },

    /// An unknown callable, either an object or a function.
    /// These were not found during the call lookup
    UnknownCallable { name: QualifiedName },

    /// A type mismatch error occured during the lowering phase.
    TypeMismatchSource {
        expected: TypeName,
        found: TypeName,
        source: TypeMismatchSource,
    },

    /// A local was re-declared with a seperate type to the previous
    /// declaration, this is not allowed!
    LocalRedeclarationWithDifferentType {
        original: TypeName,
        new: TypeName,
        name: String,
    },

    /// void fn, void return expr
    NoReturnExpectedReturn { expected: TypeName },

    /// void fn, any nonvoid return expr
    ReturnInVoidFn,

    /// Attempted to call a function with an unknown name
    UnknownFunction { name: QualifiedName },

    /// Attempted to reference an object with an unknown name
    UnknownObject { name: QualifiedName },

    /// Attempted to call a method with an unknown name
    UnknownMethod {
        name: QualifiedName,
        method_type: TypeName,
    },

    /// Attempted to reference a field which does not exist
    UnknownObjectField { name: QualifiedName, field: String },

    /// Attempted to call a function with an invalid nubmer
    /// of arguments!
    ///
    /// (the vm does handle this but comptime errors are better)
    NumArgumentCallMismatch { expected: usize, found: usize },

    /// Inferring methods on non object types are not permitted
    /// because they do not belong to a namespace
    InferringMethodOnNonObjectType {
        method_name: String,
        reciever_type: TypeName,
    },

    /// No namespace for an object, which means we cant figure
    /// out what namespace to look for, for methods!
    InferringMethodOnObjectWithoutNamespace { object_name: QualifiedName },

    /// An expression in a position which is expected to produce
    /// a value did not in fact produce a value
    ExpressionDidNotProduceValue { source: TypeMismatchSource },

    /// A local was looked up but it could not be found!
    UnknownLocal { name: String },

    /// A name was attempted to be assigned to but it could not be found!
    UnknownAssignmentTarget { name: String },

    /// Attempted to assign to a constant global!
    ConstantGlobalAssignment { name: QualifiedName },

    /// A name was attempted to be an expression to but it could not be found!
    UnknownName { name: String },

    /// A global was looked up but it could not be found!
    UnknownGlobal { name: String },

    /// A type mismatch for a step operator! It isn't defined
    /// for this type which is used for this local
    StepOperatorTypeMismatch {
        type_name: TypeName,
        operator: StepOperator,
        local_name: String,
    },

    /// A type mismatch for a unar operator! It isn't defined
    /// for this type which is used for this local
    UnaryOperatorTypeMismatch {
        type_name: TypeName,
        operator: UnaryOperator,
    },

    /// A type mismatch for a compound operator! It isn't defined
    /// for this type
    CompoundAssignmentOperatorTypeMismatch {
        type_name: TypeName,
        operator: AssignmentOperator,
    },

    /// Found the usage of a field assignment to a non-object type when
    /// trying to assign using object-field assignment syntax
    FieldAssignmentToNonObjectType { found: TypeName },

    /// Found the usage of a field access to a non-object type when
    /// trying to access using object-field access syntax
    FieldAccessToNonObjectType { found: TypeName },

    /// Invalid assignment target, non-array, object, local or global
    InvalidAssignmentTarget,

    /// Invalid call target, it is a non-name
    InvalidNonNameCallTarget,

    /// Invalid conversion type for specific intrinsic
    InvalidTypeForConversionIntrinsic {
        intrinsic: Intrinsic,
        found: TypeName,
    },

    /// A cast of a value to Void...
    VoidCast,

    /// Array append operator can only be applied to two arrays
    ArrayAppendToNonBothArrayOperands { lhs: TypeName, rhs: TypeName },

    /// Binary operator can only be applied to two of the same types
    BinaryOpMismatch {
        operator: BinaryOperator,
        lhs: TypeName,
        rhs: TypeName,
    },

    /// Using binary operator on a type where it is not defined
    BinaryOperatorUndefinedForType {
        operator: BinaryOperator,
        operand_type: TypeName,
    },

    /// An invalid generic instance was found, this is means
    /// that an instance attempted to match against the signature
    /// and was invalid for the signature/instance
    InvalidGenericInstance {
        instance_type: TypeName,
        signature: QualifiedName,
    },

    /// An invalid supplied type mismatch for this generic parameter
    InvalidSuppliedTypeForGenericParam {
        generic_parameter: String,
        expected: TypeName,
        found: TypeName,
    },

    /// When trying to infer types against some declared applied type
    /// the supplied type is not an applied type, but rather some other
    /// concrete type.
    ///
    /// E.g for Queue<T> we are supplying Int which doesnt fit
    NonAppliedSuppliedTypeForDeclaredSuppliedType { supplied: TypeName },

    /// When trying to infer types against some declared applied type
    /// the supplied types generic parameters mismatch in the number of arguments
    /// against the declared type
    AppliedTypeInferenceArgNMismatch {
        declared_argn: usize,
        supplied_argn: usize,
    },

    /// Unable to infer/find this type parameter after explicit and inferred type substitution
    /// during object type substiution
    UnknownObjectTypeParam {
        parameter: String,
        object_type: QualifiedName,
    },

    /// Unable to infer/find this type parameter after explicit and inferred type substitution
    /// during function type substiution
    UnknownFunctionTypeParam {
        parameter: String,
        function: QualifiedName,
    },

    /// Too many type parameters were passed for this generic resolution,
    TooManyExplicitTypeParams {
        name: QualifiedName,
        max: usize,
        found: usize,
    },

    /// Missing the function template for this function which we are
    /// attempting to monomorphise and create a specialisation for
    MissingFunctionTemplate { name: QualifiedName },

    /// When registering a specialisation of a function for monomorphisation
    /// a type argument still had an unresolved generic parameter
    UnresolvedGenericParameterDuringMonomorph {
        parameter: String,
        function_name: QualifiedName,
    },

    /// Entry referred to a generic function! That like... doesn't make sense
    /// what do you expect?
    EntryGenericFunction { name: QualifiedName },
}

/// Located version of `PhotonErrorKind` w source span info
pub type PhotonError = Box<Located<PhotonErrorKind>>;

/// Result type for photon functions
pub type PhotonResult<T> = Result<T, PhotonError>;

impl PhotonErrorKind {
    /// Returns a new located `PhotonErrorKind` which represents
    /// some error into the source.
    pub fn error(kind: Self, span: SourceSpan) -> PhotonError {
        Box::new(Located::new(kind, span))
    }
}

impl Display for PhotonErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnterminatedLegacyBoson3Block => {
                write!(
                    f,
                    "expected to find a closing `}}`, instead found unterminated `b3<Type> {{ ... }}` block"
                )
            }
            Self::MalformedNumber { reason } => {
                write!(
                    f,
                    "expected to find a valid numeric literal here, instead the literal was malformed: `{reason}`"
                )
            }
            Self::UnexpectedCharacter { character } => {
                write!(
                    f,
                    "expected a valid symbol character, instead found unexpected character `{character}`"
                )
            }
            Self::MissingDirectiveName => {
                write!(f, "expected a directive name after `@`")
            }
            Self::UnexpectedEndOfFile => {
                write!(
                    f,
                    "unexpected end of file at the current position, there should be more tokens!"
                )
            }
            Self::UnexpectedToken { found, expected } => {
                write!(
                    f,
                    "unexpected `{found}` at the current position, there should have been a `{expected}`!"
                )
            }
            Self::UnknownTLD { name } => {
                write!(f, "unknown top-level directive `@{name}`")
            }
            Self::UnexpectedEndOfParamsFollowingComma => {
                write!(
                    f,
                    "unexpected right `)` here when more params should have followed after `,`"
                )
            }
            Self::UnexpectedEndOfTypeParamsFollowingComma => {
                write!(
                    f,
                    "unexpected right `]` here when more type params should have followed after `,`"
                )
            }
            Self::UnexpectedEndOfArgsFollowingComma => {
                write!(
                    f,
                    "unexpected right `)` here when more arguments should have followed after `,`"
                )
            }
            Self::UnexpectedEndOfTypeArgsFollowingComma => {
                write!(
                    f,
                    "unexpected right `]` here when more type arguments should have followed after `,`"
                )
            }
            Self::UnexpectedDirective { found, expected } => {
                write!(
                    f,
                    "unexpected `@{found}` directive at the current position, there should have been an `@{expected}`!"
                )
            }
            Self::InvalidCapabilityNumber { found } => {
                write!(
                    f,
                    "expected non-negative integer capability number, found {found}"
                )
            }
            Self::UnexpectedEndOfArrayElemsFollowingComma => {
                write!(
                    f,
                    "unexpected right `]` here when more elements should have followed after `,`"
                )
            }
            Self::UnexpectedNonExpression { found } => {
                write!(
                    f,
                    "unexpectedly found non-expression element `{found}` in expression position!"
                )
            }
            Self::UnexpectedNonStepOperator { found } => {
                write!(
                    f,
                    "unexpectedly found non-step operator `{found}` in step operator position, which must be `++` or `--`!"
                )
            }
            Self::UnexpectedNonEndOfStatement { found } => {
                write!(
                    f,
                    "unexpectedly found non-end of statement element `{found}` in end of statement position!"
                )
            }
            Self::UnexpectedNonB3Statement { found } => {
                write!(
                    f,
                    "unexpectedly found non-b3 statement `{found}` in b3 statement position!"
                )
            }
            Self::UnclosedBlockFound => {
                write!(
                    f,
                    "could not find a matching `}}` for a open block! unterminated block's are not valid"
                )
            }
            Self::DuplicateNamespace => {
                write!(f, "unexpectedly found more than one @namespace declaration")
            }
            Self::NoNamespace => {
                write!(f, "unexpectedly found no namespace delcarations in file")
            }
            Self::DuplicateGlobal { name } => {
                write!(
                    f,
                    "multiple definitions of the global with the qualified name `{name}` were found!"
                )
            }
            Self::DuplicateTypeParameter { name } => {
                write!(
                    f,
                    "multiple type parameters with the same name `{name}` were found!"
                )
            }
            Self::DuplicateFunction { name } => {
                write!(
                    f,
                    "multiple definitions of the function with the qualified name `{name}` were found!"
                )
            }
            Self::DuplicateObject { name } => {
                write!(
                    f,
                    "multiple definitions of the object with the qualified name `{name}` were found!"
                )
            }
            Self::TypeMismatchSource {
                expected,
                found,
                source,
            } => {
                write!(
                    f,
                    "expected to recieve type of `{expected}` in {source}, received `{found}`"
                )
            }
            Self::LocalRedeclarationWithDifferentType {
                original,
                new,
                name,
            } => {
                write!(
                    f,
                    "local `{name}` was already declared as `{original}` and cannot be redeclared as `{new}`"
                )
            }
            Self::NoReturnExpectedReturn { expected } => {
                write!(
                    f,
                    "function has declared return type of `{expected}`, so `return` must return said type value"
                )
            }
            Self::ReturnInVoidFn => {
                write!(
                    f,
                    "function has declared return type of `Void`, so `return` must NOT return any value`"
                )
            }
            Self::DuplicateCallable { name } => {
                write!(
                    f,
                    "Attempted to declare a function or a object under the name `{name}`, however an existing function/object already exists under the same name! because the object constructor is a call it's impossible to resolve this, so this callable case is illegal!"
                )
            }
            Self::UnknownFunction { name } => {
                write!(f, "found reference to unknown function `{name}`")
            }
            Self::UnknownObject { name } => {
                write!(f, "found reference to unknown object `{name}`")
            }
            Self::UnknownCallable { name } => {
                write!(
                    f,
                    "found reference to unknown function or a object under the name `{name}`!"
                )
            }
            Self::NumArgumentCallMismatch { expected, found } => {
                write!(f, "expected {expected} arguments, received {found}")
            }
            Self::UnknownMethod { name, method_type } => {
                write!(
                    f,
                    "found reference to unknown method `{name}` on type `{method_type}`"
                )
            }
            Self::InferringMethodOnNonObjectType {
                method_name,
                reciever_type,
            } => {
                write!(
                    f,
                    "cannot infer method `{method_name}` because receiver type is `{reciever_type}` and is not a valid object-based type!"
                )
            }
            Self::InferringMethodOnObjectWithoutNamespace { object_name } => {
                write!(
                    f,
                    "cannot infer method name on object type `{object_name}` because it has no namespace for method lookup"
                )
            }
            Self::ExpressionDidNotProduceValue { source } => {
                write!(
                    f,
                    "expected expression in position of `{source}` to produce a value, but it did not!"
                )
            }
            Self::StepOperatorTypeMismatch {
                type_name,
                operator,
                local_name,
            } => {
                write!(
                    f,
                    "attempted to use step operator `{operator}` on local `{local_name}` with type `{type_name}`, however the operator is not defined for this type!"
                )
            }
            Self::UnaryOperatorTypeMismatch {
                type_name,
                operator,
            } => {
                write!(
                    f,
                    "attempted to use unary operator `{operator}` on type `{type_name}`, however the operator is not defined for this type!"
                )
            }
            Self::CompoundAssignmentOperatorTypeMismatch {
                type_name,
                operator,
            } => {
                write!(
                    f,
                    "attempted to use compound assignment operator `{operator}` with value of type `{type_name}`, however the operator is not defined for this type!"
                )
            }
            Self::UnknownLocal { name } => {
                write!(f, "found reference to unknown local `{name}`")
            }
            Self::ConstantGlobalAssignment { name } => {
                write!(f, "found assignment to a constant global variable `{name}`. This is not allowed!")
            }
            Self::UnknownAssignmentTarget { name } => {
                write!(
                    f,
                    "found reference to unknown assignment target `{name}`, this could not be resolved to a local or a global"
                )
            }
            Self::UnknownName { name } => {
                write!(
                    f,
                    "found reference to unknown `{name}`, this could not be resolved to a local or a global"
                )
            }
            Self::UnknownGlobal { name } => {
                write!(f, "found reference to unknown global `{name}`")
            }
            Self::FieldAssignmentToNonObjectType { found } => {
                write!(
                    f,
                    "unexpected illegal field assignment to non object type `{found}`"
                )
            }
            Self::FieldAccessToNonObjectType { found } => {
                write!(
                    f,
                    "unexpected illegal field access to non object type `{found}`"
                )
            }
            Self::UnknownObjectField { name, field } => {
                write!(
                    f,
                    "found reference to unknown field `{field}` on object with type `{name}`"
                )
            }
            Self::InvalidAssignmentTarget => {
                write!(
                    f,
                    "invalid assignment target, assignment target must be a local, global, object field, or array element"
                )
            }
            Self::InvalidNonNameCallTarget => {
                write!(
                    f,
                    "invalid call target, call targets must be a direct name (function/object constructor)."
                )
            }
            Self::InvalidTypeForConversionIntrinsic { intrinsic, found } => {
                write!(
                    f,
                    "invalid type lhs for conversion intrinsic `{intrinsic}`, got lhs type of `{found}`"
                )
            }
            Self::VoidCast => {
                write!(f, "attempted invalid cast of value to type of Void")
            }
            Self::ArrayAppendToNonBothArrayOperands { lhs, rhs } => {
                write!(
                    f,
                    "attemped array append `++` to non-array operands! got lhs of `{lhs}` and rhs of `{rhs}`"
                )
            }
            Self::BinaryOpMismatch { operator, lhs, rhs } => {
                write!(
                    f,
                    "attemped operator usage `{operator}` to mismatching typed operands! got lhs of `{lhs}` and rhs of `{rhs}`"
                )
            }
            Self::BinaryOperatorUndefinedForType {
                operator,
                operand_type,
            } => {
                write!(
                    f,
                    "attempted usage of operator `{operator}` on type operands of type `{operand_type}` where it is not defined"
                )
            }
            Self::InvalidGenericInstance {
                instance_type,
                signature,
            } => {
                write!(
                    f,
                    "found invalid concrete instance `{instance_type}` for actual signature type `{signature}`"
                )
            }
            Self::InvalidSuppliedTypeForGenericParam {
                generic_parameter,
                expected,
                found,
            } => {
                write!(
                    f,
                    "for generic parameter of type `{generic_parameter}` recieved type `{found}` mismatches expected concrete type `{expected}`"
                )
            }
            Self::NonAppliedSuppliedTypeForDeclaredSuppliedType { supplied } => {
                write!(
                    f,
                    "expected some applied type Applied<Args> to be provided for type inference, instead found `{supplied}`"
                )
            }
            Self::AppliedTypeInferenceArgNMismatch {
                declared_argn,
                supplied_argn,
            } => {
                write!(
                    f,
                    "expected supplied type argument count `{supplied_argn}` to match declared type argument count `{declared_argn}` during type inference"
                )
            }
            Self::UnknownObjectTypeParam {
                parameter,
                object_type,
            } => {
                write!(
                    f,
                    "cannot infer type parameter `{parameter}` for object of type `{object_type}`"
                )
            }
            Self::UnknownFunctionTypeParam {
                parameter,
                function,
            } => {
                write!(
                    f,
                    "cannot infer type parameter `{parameter}` for function `{function}`"
                )
            }
            Self::TooManyExplicitTypeParams { name, max, found } => {
                write!(
                    f,
                    "recieved too many explicit parameters during generic resolution for `{name}`, expected at most `{max}`, got `{found}`"
                )
            }
            Self::EntryGenericFunction { name } => {
                write!(
                    f,
                    "entry referred to a generic function `{name}`, this is not allowed.. like what type args would it even get?"
                )
            }
            Self::MissingFunctionTemplate { name } => {
                write!(
                    f,
                    "attempted to specialise `{name}` as a generic function, but this function's generic template could not be found. does it even take type parameters?"
                )
            }
            Self::UnresolvedGenericParameterDuringMonomorph {
                parameter,
                function_name,
            } => {
                write!(
                    f,
                    "when specialising `{function_name}` as a generic function, found an unresolved generic parameter `{parameter}`!"
                )
            }
            Self::UnexpectedMultipleEntry { first, second } => {
                write!(f, "unexpected mulitple declarations of an @entry function across modules, found first `{first}` and then unexpected second `{second}`")
            }
            Self::GlobalInitialisationCycle { cycle } => {
                let cycle_joined = cycle.iter().map(|node| node.to_string()).collect::<Vec<_>>().join(" -> ");
                write!(f, "unexpected cycle in dependency graph of global initiallisations, cycle: `{cycle_joined}`")
            }
        }
    }
}

impl Display for PhotonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let actual_error_kind = &self.value;
        write!(f, "{actual_error_kind}")
    }
}

/// Sources of type mistmatches down
#[derive(Debug, Clone)]
pub enum TypeMismatchSource {
    /// The foreach source must be of the type Array
    ForEachArraySource,

    /// The foreach binding from the array elements
    ForEachBinding,

    /// The for condition must be of the type Bool
    ForCondition,

    /// The dowhile condition must be of the type bool
    DoWhileCondition,

    /// The while condition must be of the type bool
    WhileCondition,

    /// The if condition must be of the type bool
    IfCondition,

    /// The value returned by a `return`
    ReturnValue,

    /// The value used in a tail-call with a method on a reciever position
    TailCallMethodReciever,

    /// In the position of a normal tail call arguments
    TailCallArguments,

    /// In the position of a method tail call arguments
    TailCallMethodArguments,

    /// In the position of an initialiser of a local
    LetLocalInitialiser,

    /// In the position of an initialiser of a global
    GlobalInitialiser,

    /// In the position of an index for this source of this array indeinxg op
    ArrayIndex { source: Box<Self> },

    /// In the position of the right-hand side of an assignment
    AssignmentRHS,

    /// In the position of a field assignment as the object we are assigning to
    FieldAssignmentReciever,

    /// In the position of a array index assignment as the array we are assigning to
    ArrayIndexAssignmentReciever,

    /// In the position of an array literal expression
    ArrayLiteralExpression,

    /// As the argument to this intrinsic call
    IntrinsicArgument { intrinsic: Intrinsic },

    /// As the condition to an assert expression
    AssertCondition,

    /// As the arguments to an object constructor
    ObjectConstructorArguments { name: QualifiedName },

    /// As the arguments to a function call
    FunctionCallArguments { name: QualifiedName },

    /// As this field of to an object constructor
    ObjectConstructorField { name: QualifiedName, field: String },

    /// As this param # to a function
    FunctionCallArgument { name: QualifiedName, argn: usize },

    /// A normal method call on an object which is the reciever
    MethodCallReceiver,

    /// As the argument to an array append call
    ArrayAppendArgument,

    /// As the argument to an array prepend call
    ArrayPrependArgument,

    /// As the reciever of a normal field access
    FieldAccessReciever { field: String },

    /// As the LHS reciever/target of a cast
    CastTarget,

    /// As part of an array indexing expresion
    ArrayIndexExpr,

    /// The condition expression of a conditional expression
    ConditionExpressionOfTheConditionalExpression,

    /// The true branch of the conditional expression
    TrueBranchCondExpr,

    /// The false branch of the conditional expression
    FalseBranchCondExpr,

    /// The operand to apply a unary operation to
    UnaryOperand,

    /// LHS of a binary op
    BinOpLHS,

    /// RHS of a binary op
    BinOpRHS,

    /// Inferrence of an object's types based of supplied
    /// arguments compared to it's declared type params
    ObjectTypeInferrence { field: String },

    /// Inferrence of an functions's types based of supplied
    /// arguments compared to it's declared type params
    FunctionTypeInferrence { argn: usize },
}

impl Display for TypeMismatchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeMismatchSource::ForEachArraySource => {
                write!(f, "the source array of a foreach statement")
            }
            TypeMismatchSource::ForEachBinding => {
                write!(f, "the binding of each foreach element")
            }
            TypeMismatchSource::ForCondition => {
                write!(f, "the condition of a for statement")
            }
            TypeMismatchSource::DoWhileCondition => {
                write!(f, "the condition of a dowhile statement")
            }
            TypeMismatchSource::WhileCondition => {
                write!(f, "the condition of a while statement")
            }
            TypeMismatchSource::IfCondition => {
                write!(f, "the condition of an if statement")
            }
            TypeMismatchSource::ReturnValue => {
                write!(f, "the expression of a return statement")
            }
            TypeMismatchSource::TailCallMethodReciever => {
                write!(f, "the reciever of a method in a tail call position")
            }
            TypeMismatchSource::TailCallArguments => {
                write!(f, "the arguments to a tail call")
            }
            TypeMismatchSource::TailCallMethodArguments => {
                write!(f, "the arguments to a tail call on an object method")
            }
            TypeMismatchSource::LetLocalInitialiser => {
                write!(f, "the initialiser of a local declaration")
            }
            TypeMismatchSource::GlobalInitialiser => {
                write!(f, "the initialiser of a global declaration")
            }
            TypeMismatchSource::ArrayIndex { source } => {
                write!(
                    f,
                    "the array index of a an array used in the position of `{source}`"
                )
            }
            TypeMismatchSource::AssignmentRHS => {
                write!(f, "the right-hand side of an assignment")
            }
            TypeMismatchSource::FieldAssignmentReciever => {
                write!(
                    f,
                    "the left-hand side of an assignment as an object for a field assignment"
                )
            }
            TypeMismatchSource::ArrayIndexAssignmentReciever => {
                write!(
                    f,
                    "the left-hand side of an assignment as an array for a field assignment"
                )
            }
            TypeMismatchSource::ArrayLiteralExpression => {
                write!(
                    f,
                    "an array being constructed by an array literal expression"
                )
            }
            TypeMismatchSource::IntrinsicArgument { intrinsic } => {
                write!(f, "as the argument to the intrinsic `{intrinsic}`")
            }
            TypeMismatchSource::AssertCondition => {
                write!(
                    f,
                    "the condition that is being asserted upon in the assert intrinsic call"
                )
            }
            TypeMismatchSource::ObjectConstructorArguments { name } => {
                write!(
                    f,
                    "arguments to the object constructor for the object `{name}`"
                )
            }
            TypeMismatchSource::FunctionCallArguments { name } => {
                write!(f, "arguments to the function `{name}`")
            }
            TypeMismatchSource::ObjectConstructorField { name, field } => {
                write!(
                    f,
                    "value for the field `{field}` for the object constructor for the object `{name}`"
                )
            }
            TypeMismatchSource::FunctionCallArgument { name, argn } => {
                write!(
                    f,
                    "argument for the parameter #`{argn}` for the function `{name}`"
                )
            }
            TypeMismatchSource::MethodCallReceiver => {
                write!(f, "the reciever of a method call")
            }
            TypeMismatchSource::ArrayAppendArgument => {
                write!(f, "the argument to an array append call")
            }
            TypeMismatchSource::ArrayPrependArgument => {
                write!(f, "the argument to an array prepend call")
            }
            TypeMismatchSource::FieldAccessReciever { field } => {
                write!(
                    f,
                    "the object reciever of the field access expression with the field `{field}`"
                )
            }
            TypeMismatchSource::CastTarget => {
                write!(f, "the target/reciever of a cast operation")
            }
            TypeMismatchSource::ArrayIndexExpr => {
                write!(f, "an array indexeing expression")
            }
            TypeMismatchSource::ConditionExpressionOfTheConditionalExpression => {
                write!(f, "the condition expression of a conditional expression")
            }
            TypeMismatchSource::FalseBranchCondExpr => {
                write!(f, "the false branch of a conditional expression")
            }
            TypeMismatchSource::TrueBranchCondExpr => {
                write!(f, "the true branch of a conditional expression")
            }
            TypeMismatchSource::UnaryOperand => {
                write!(f, "the operand to a unary expression")
            }
            TypeMismatchSource::BinOpLHS => {
                write!(f, "the left-hand side operand to a binary expression")
            }
            TypeMismatchSource::BinOpRHS => {
                write!(f, "the right-hand side operand to a binary expression")
            }
            TypeMismatchSource::ObjectTypeInferrence { field } => {
                write!(
                    f,
                    "in the type inferrence for object's type parameters against its concrete arguments for field `{field}`"
                )
            }
            TypeMismatchSource::FunctionTypeInferrence { argn } => {
                write!(
                    f,
                    "in the type inferrence for functions's type parameters against its concrete arguments for arg #`{argn}`"
                )
            }
        }
    }
}

impl From<SourceSpan> for miette::SourceSpan {
    fn from(span: SourceSpan) -> Self {
        (span.start, span.end - span.start).into()
    }
}
