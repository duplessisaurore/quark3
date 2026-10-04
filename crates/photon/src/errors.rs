//! All errors that can occur during the `Photon3`
//! sugar pass

use std::fmt::Display;

use crate::{
    ast::{Located, SourceSpan},
    lexer::TokenKind,
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
