use std::fmt::Display;

/// All errors that can occur during the `Boson3`
/// lowering pass


/// All possible error kinds that can occurr
/// during the lowering process
pub enum LoweringErrorKind {
    // Unclosed if/loop/try block in the code
    UnclosedConstruct,

    // Invalid argument to a directive
    InvalidArgument {
        expected: String,
        got: String
    },

    // Invalid number of fields named
    // for this object
    InvalidNamedFieldsAmount {
        object_name: String,
        fields_expected: u64,
        fields_got: u64
    }
}

/// Located version of `LoweringErrorKind`
pub struct LoweringError {
    kind: LoweringErrorKind,

    // Line in the input source file.
    line: usize
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
            Self::UnclosedConstruct => {
                write!(f, "An unclosed construct was found, make sure all `if`/`loop`/`try`s are closed!")
            },
            Self::InvalidArgument { expected, got } => {
                write!(f, "expected `{expected}`, got `{got}`")
            },
            Self::InvalidNamedFieldsAmount { object_name, fields_expected, fields_got } => {
                write!(f, "The object `{object_name}` expected `{fields_expected}` field names, got `{fields_got}` field names")
            }
        }
    }
}

impl Display for LoweringError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.kind)
    }
}