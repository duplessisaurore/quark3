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

    // Invalid number of fields named
    // for this construct
    InvalidNamedFieldsAmount {
        name: String,
        fields_expected: u64,
        fields_got: u64,
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
        }
    }
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.kind)
    }
}
