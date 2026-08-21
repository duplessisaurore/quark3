//! All errors that can occur during the `Boson3`
//! lowering pass

use std::fmt::Display;

/// All possible error kinds that can occurr
/// during the lowering process
pub enum LoweringErrorKind {
    // Invalid argument to a directive
    InvalidArgument {
        expected: String,
        got: String,
    },

    // A local declaration was found
    // outside of a valid `Boson3` function
    LocalOutsideFunction {
        local_name: String,
    },

    // An undefined global was used in a
    // global instruction
    UndefinedGlobal {
        global: String,
    },

    // An undefined local was used in a
    // local instruction
    UndefinedLocal {
        local: String,
    },

    // An undefined capability was used
    UndefinedCapability {
        capability: String,
    },

    /// There was an invalid object field
    /// access here
    InvalidObjectField {
        got: String,
    },

    /// Attempted to access an object, but
    /// that object doesn't even have fields defined
    AccessObjectWithNoFieldDefs {
        object_name: String,
        field: String,
    },

    /// Attempted to access an object but
    /// the field does not exist on this object!
    InvalidObjectFieldAccess {
        object_name: String,
        field: String,
    },

    /// A nested macro definition was found, this is
    /// not permitted!
    NestedMacroDefinition,

    /// A macro was defined without the corresponding @end
    UnterminatedMacro {
        name: String,
    },

    /// Two macros were defined with the same name
    DuplicateMacro {
        name: String,
    },

    /// Attempted to invoke an invalid macro which was
    /// not defined
    UndefinedMacro {
        name: String,
    },

    /// We hit the total expansion limit and are no longer continuing
    ExpansionTotalLimit {
        // This is the macro which we hit the limit in
        name: String,
    },

    /// Leftover tokens as an argument to a macro invocation
    MacroInvocationLeftoverTokens {
        name: String,
    },

    /// Missing macro arguments! there were not enough arguments
    /// to fufill this macro invocation
    MissingMacroArguments {
        name: String,
        expected: u64,
        got: u64,
    },

    /// A block `{..}` was found without a terminating `}` during
    ///  the expansion of a macro
    UnterminatedBlock {
        name: String,
    },

    /// A block argument was used in-line for this param
    /// is not supported, only alone can it be used.
    BlockArgumentInline {
        name: String,
        param: String,
    },

    /// File index was not found in the file table
    FileIndexNotDefined {
        index: u64,
    },

    /// The `passes` limit was hit, meaning we
    /// expanded too much
    PassLimit {
        passes: u64,
    },

    /// This macro parameter was found to be unnamed
    UndefinedMacroParameter {
        name: String,
        param: String,
    },

    /// We are trying to use a raw directive in a non-block argument
    /// which is not allowed!
    RawDirectiveAsArgument {
        name: String,
    },

    /// The scope is unbalanced and did not close
    UnbalancedScope,

    /// The scope directive was found outside of a scope
    /// such as break/continue
    ScopeDirectiveOutsideScope {
        directive: String,
    },

    /// The target of the this scope directive was found
    /// to be unavailable/could not be retrieved
    ScopeTargetUnavailable {
        directive: String,
    },
}

/// Located version of `LoweringErrorKind`
pub struct LoweringError {
    kind: LoweringErrorKind,

    // Line in the input source file.
    line: usize,
}

impl LoweringErrorKind {
    /// Adds a line to this `LoweringErrorKind` turning it into a
    /// `LoweringError`.
    pub fn with_line(self, line_number: usize) -> LoweringError {
        LoweringError {
            line: line_number,
            kind: self,
        }
    }
}

impl Display for LoweringErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidArgument { expected, got } => {
                write!(f, "expected `{expected}`, got `{got}`")
            }
            Self::UndefinedGlobal { global } => {
                write!(f, "The global `{global}` was not defined")
            }
            Self::UndefinedLocal { local } => {
                write!(f, "The local `{local}` was not defined")
            }
            Self::UndefinedCapability { capability } => {
                write!(f, "The capability `{capability}` was not defined")
            }
            Self::InvalidObjectField { got } => {
                write!(
                    f,
                    "The field access `{got}` is not in the valid format of <object_type>.<field>"
                )
            }
            Self::AccessObjectWithNoFieldDefs { object_name, field } => {
                write!(
                    f,
                    "Attempted to access field `{field}` on object with type `{object_name}`, but `{object_name}` has no fields defined!"
                )
            }
            Self::InvalidObjectFieldAccess { object_name, field } => {
                write!(
                    f,
                    "Attempted to access field `{field}` on object with type `{object_name}`, but `{object_name}` doesn't contain that field!"
                )
            }
            Self::LocalOutsideFunction { local_name } => {
                write!(
                    f,
                    "Attempted to declare local with name `{local_name}` outside of a function!"
                )
            }
            Self::NestedMacroDefinition => {
                write!(
                    f,
                    "Found a nested macro definition! Move the nested definition out and call it using the macro call syntax!"
                )
            }
            Self::UnterminatedMacro { name } => {
                write!(
                    f,
                    "The macro `{name}` does not have a corresponding `@end` directive!"
                )
            }
            Self::UnterminatedBlock { name } => {
                write!(
                    f,
                    "The macro `{name}` during expansion contains an unterminated block!"
                )
            }
            Self::BlockArgumentInline { name, param } => {
                write!(
                    f,
                    "The macro `{name}` during expansion encounted the block-type parameter `{param}` being used in an in-line context!"
                )
            }
            Self::DuplicateMacro { name } => {
                write!(
                    f,
                    "The macro `{name}` was found to have more than one definition (duplicate)!"
                )
            }
            Self::UndefinedMacro { name } => {
                write!(f, "The macro `{name}` was not defined")
            }
            Self::ExpansionTotalLimit { name } => {
                write!(
                    f,
                    "While expanding the macro `{name}`, the total macro expansion limit was hit!"
                )
            }
            Self::MacroInvocationLeftoverTokens { name } => {
                write!(
                    f,
                    "While expanding the macro `{name}`, there were leftover argument tokens!"
                )
            }
            Self::MissingMacroArguments {
                name,
                expected,
                got,
            } => {
                write!(
                    f,
                    "The invocation of the macro `{name}` expected `{expected}` args, got `{got}`."
                )
            }
            Self::FileIndexNotDefined { index } => {
                write!(
                    f,
                    "the file index `{index}` was referenced but it was not defined"
                )
            }
            Self::PassLimit { passes } => {
                write!(
                    f,
                    "The macro expansion pass limit of `{passes}` was hit! This usually means a macro invokes itself, directly or indirectly, without terminating."
                )
            }
            Self::UndefinedMacroParameter { name, param } => {
                write!(
                    f,
                    "The macro `{name}` refers to the parameter `{param}`, but no such parameter is declared in its signature!"
                )
            }
            Self::RawDirectiveAsArgument { name } => {
                write!(
                    f,
                    "The invocation of the macro `{name}` passes a raw directive as an argument, these are only permitted inside a `{{..}}` block!"
                )
            }
            Self::UnbalancedScope => {
                write!(
                    f,
                    "Found an unbalanced scope! An `@scope_push` is missing its corresponding `@scope_pop`, or a `@scope_pop` appeared with nothing to close."
                )
            }
            Self::ScopeDirectiveOutsideScope { directive } => {
                write!(
                    f,
                    "The directive `{directive}` was used outside of any enclosing scope!"
                )
            }
            Self::ScopeTargetUnavailable { directive } => {
                write!(
                    f,
                    "The directive `{directive}` was used in a scope that does not provide a target for it!"
                )
            }
        }
    }
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.kind)
    }
}
