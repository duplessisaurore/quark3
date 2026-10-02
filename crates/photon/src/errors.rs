//! All errors that can occur during the `Photon3`
//! sugar pass

use std::fmt::Display;

use crate::ast::{Located, SourceSpan};

/// All possible error kinds that can occur during
/// the `Photon3` sugar pass.
pub enum PhotonErrorKind {
    /// An integer radix prefex (etc 0x or 0o)
    /// did not follow with any digits!
    MissingDigitsAfterIntRadixPrefix
}

/// Located version of `PhotonErrorKind` w source span info
pub type PhotonError = Located<PhotonErrorKind>;

impl PhotonErrorKind {
    /// Returns a new located `PhotonErrorKind` which represents
    /// some error into the source.
    fn error(kind: Self, span: SourceSpan) -> PhotonError {
        Located::new(kind, span)
    }
}

impl Display for PhotonErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MissingDigitsAfterIntRadixPrefix => {
                write!(f, "expected digits after integer radix prefix")
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
