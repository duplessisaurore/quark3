//! All errors that can occur during the `Photon3`
//! sugar pass

use std::fmt::{Display, write};

use crate::{ast::{Located, SourceSpan}, lexer::{Token, TokenKind}};

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
    UnexpectedCharacter {
        character: char
    },

    /// A missing directive name was found here
    MissingDirectiveName,

    /// Unexpected end of file here, we should
    /// see more tokens
    UnexpectedEndOfFile,
    
    /// A token was found to be here that we did
    /// not expect
    UnexpectedToken {
        found: TokenKind,
        expected: TokenKind
    }
}

/// Located version of `PhotonErrorKind` w source span info
pub type PhotonError = Located<PhotonErrorKind>;

/// Result type for photon functions
pub type PhotonResult<T> = Result<T, PhotonError>;

impl PhotonErrorKind {
    /// Returns a new located `PhotonErrorKind` which represents
    /// some error into the source.
    pub fn error(kind: Self, span: SourceSpan) -> PhotonError {
        Located::new(kind, span)
    }
}

impl Display for PhotonErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnterminatedLegacyBoson3Block => {
                write!(f, "expected to find a closing `}}`, instead found unterminated `b3<Type> {{ ... }}` block")
            }
            Self::MalformedNumber { reason } => {
                write!(f, "expected to find a valid numeric literal here, instead the literal was malformed: `{reason}`")
            }
            Self::UnexpectedCharacter { character } => {
                write!(f, "expected a valid symbol character, instead found unexpected character `{character}`")
            }
            Self::MissingDirectiveName => {
                write!(f, "expected a directive name after `@`")
            }
            Self::UnexpectedEndOfFile => {
                write!(f, "unexpected end of file at the current position, there should be more tokens!")
            }
            Self::UnexpectedToken { found, expected } => {
                write!(f, "unexpected {found} at the current position, there should have been a {expected}!")
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
