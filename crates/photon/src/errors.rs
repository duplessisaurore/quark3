//! All errors that can occur during the `Photon3`
//! sugar pass

use std::fmt::Display;

use crate::{
    ast::{AssignmentOperator, Located, QualifiedName, SourceSpan, StepOperator, TypeName}, lexer::TokenKind,
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

    /// Expected another argument follow since a comma
    /// was found here
    UnexpectedEndOfArgsFollowingComma,

    /// Expected another array element follow since a comma
    /// was found here
    UnexpectedEndOfArrayElemsFollowingComma,

    /// Unexpected directive in the current position
    UnexpectedDirective { found: String, expected: String },

    /// Invalid numeric literal for the @capability directive
    InvalidCapabilityNumber { found: TokenKind },

    /// A non expression token was found in the place of an
    /// expression
    UnexpectedNonExpression { found: TokenKind },

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

    /// A duplicate function was declared with this name!
    DuplicateFunction { name: QualifiedName },

    /// A duplicate object was declared with this name!
    DuplicateObject { name: QualifiedName },

    /// An overlapping callable, either an object or a function
    /// was obth declared with the same name!
    DuplicateCallable { name: QualifiedName },

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
    UnknownObjectField {
        name: QualifiedName,
        field: String,
    },

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

    /// A global was looked up but it could not be found!
    UnknownGlobal { name: String },
    
    /// A type mismatch for a step operator! It isn't defined
    /// for this type which is used for this local
    StepOperatorTypeMismatch {
        type_name: TypeName,
        operator: StepOperator,
        local_name: String,
    },

    /// A type mismatch for a compound operator! It isn't defined
    /// for this type
    CompoundAssignmentOperatorTypeMismatch {
        type_name: TypeName,
        operator: AssignmentOperator,
    },

    /// Found the usage of a field assignment to a non-object type when
    /// trying to assign using object-field assignment syntax
    FieldAssignmentToNonObjectType {
        found: TypeName
    },

    /// Invalid assignment target, non-array, object, local or global
    InvalidAssignmentTarget
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
                    "unexpected {found} at the current position, there should have been a {expected}!"
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
            Self::UnexpectedEndOfArgsFollowingComma => {
                write!(
                    f,
                    "unexpected right `)` here when more arguments should have followed after `,`"
                )
            }
            Self::UnexpectedDirective { found, expected } => {
                write!(
                    f,
                    "unexpected @{found} directive at the current position, there should have been an @{expected}!"
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
                write!(
                    f,
                    "found reference to unknown local `{name}`"
                )
            }
            Self::UnknownAssignmentTarget { name } => {
                write!(
                    f,
                    "found reference to unknown assignment target `{name}`, this could not be resolved to a local or a global"
                )
            }
            Self::UnknownGlobal { name } => {
                write!(
                    f,
                    "found reference to unknown global `{name}`"
                )
            }
            Self::FieldAssignmentToNonObjectType { found } => {
                write!(
                    f,
                    "unexpected illegal field assignment to non object type `{found}`"
                )
            }
            Self::UnknownObjectField { name, field } => {
                                write!(
                    f,
                    "found reference to unknown field `{field}` on object with type `{name}`"
                )
            }
            Self::InvalidAssignmentTarget => {
                write!(f, "invalid assignment target, assignment target must be a local, global, object field, or array element")
            }
        }
    }
}

impl Display for PhotonError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let actual_error_kind = &self.value;
        let actual_error_span = &self.span;
        write!(f, "{actual_error_kind} at byte range {actual_error_span}")
    }
}

/// Sources of type mistmatches down
#[derive(Debug, Clone)]
pub enum TypeMismatchSource {
    /// The foreach source must be of the type Array
    ForEachArraySource,

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

    /// In the position of an index for this source of this array indeinxg op
    ArrayIndex { source: Box<Self> },

    /// In the position of the right-hand side of an assignment
    AssignmentRHS,

    /// In the position of a field assignment as the object we are assigning to
    FieldAssignmentReciever,

    /// In the position of a array index assignment as the array we are assigning to
    ArrayIndexAssignmentReciever
}

impl Display for TypeMismatchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeMismatchSource::ForEachArraySource => {
                write!(f, "The source array of a foreach statement")
            }
            TypeMismatchSource::ForCondition => {
                write!(f, "The condition of a for statement")
            }
            TypeMismatchSource::DoWhileCondition => {
                write!(f, "The condition of a dowhile statement")
            }
            TypeMismatchSource::WhileCondition => {
                write!(f, "The condition of a while statement")
            }
            TypeMismatchSource::IfCondition => {
                write!(f, "The condition of an if statement")
            }
            TypeMismatchSource::ReturnValue => {
                write!(f, "The expression of a return statement")
            }
            TypeMismatchSource::TailCallMethodReciever => {
                write!(f, "The reciever of a method in a tail call position")
            }
            TypeMismatchSource::TailCallArguments => {
                write!(f, "The arguments to a tail call")
            }
            TypeMismatchSource::TailCallMethodArguments => {
                write!(f, "The arguments to a tail call on an object method")
            }
            TypeMismatchSource::LetLocalInitialiser => {
                write!(f, "The initialiser of a local declaration")
            }
            TypeMismatchSource::ArrayIndex { source } => {
                write!(f, "The array index of a an array used in the position of `{source}`")
            }
            TypeMismatchSource::AssignmentRHS => {
                write!(f, "The right-hand side of an assignment")
            }
            TypeMismatchSource::FieldAssignmentReciever => {
                write!(f, "The left-hand side of an assignment as an object for a field assignment")   
            }
            TypeMismatchSource::ArrayIndexAssignmentReciever => {
                write!(f, "The left-hand side of an assignment as an array for a field assignment")   
            }
        }
    }
}
