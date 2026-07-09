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
        local_name: String
    },

    // Invalid number of fields named
    // for this construct
    InvalidNamedFieldsAmount {
        name: String,
        fields_expected: u64,
        fields_got: u64,
    },

    // Invalid number of args named
    // for this function
    InvalidNamedArgsFunctionAmount {
        name: String,
        args_expected: u64,
        args_got: u64,
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
        name: String
    },

    /// We hit the expansion limit and are no longer continuing
    ExpansionLimit {
        // This is the macro which we hit the limit in
        name: String
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
        got: u64
    },

    /// A block `{..}` was found without a terminating `}` during
    ///  the expansion of a macro
    UnterminatedBlock {
        name: String
    }
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
            Self::InvalidNamedFieldsAmount {
                name,
                fields_expected,
                fields_got,
            } => {
                write!(
                    f,
                    "The construct `{name}` expected `{fields_expected}` field names, got `{fields_got}` field names"
                )
            }
            Self::InvalidNamedArgsFunctionAmount {
                name,
                args_expected,
                args_got,
            } => {
                write!(
                    f,
                    "The function `{name}` expected `{args_expected}` arg names, got `{args_got}` arg names declared."
                )
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
            Self::DuplicateMacro { name } => {
                write!(
                    f,
                    "The macro `{name}` was found to have more than one definition (duplicate)!"
                )
            }
            Self::UndefinedMacro { name } => {
                write!(f, "The macro `{name}` was not defined")
            }
            Self::ExpansionLimit { name } => {
                write!(f, "While expanding the macro `{name}`, the macro expansion limit was hit!")
            }
            Self::MacroInvocationLeftoverTokens { name } => {
                write!(f, "While expanding the macro `{name}`, there were leftover argument tokens!")
            }
            Self::MissingMacroArguments { name, expected, got } => {
                write!(f, "The invocation of the macro `{name}` expected `{expected}` args, got `{got}`.")
            }
        }
    }
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.kind)
    }
}
