use chumsky::span::SimpleSpan;

/// A span into a photon3 source file built off chumsksy's
pub type SourceSpan = SimpleSpan<usize>;

/// A value with a `SourceSpan` location into a source file
#[derive(Debug, Clone, PartialEq)]
pub struct Located<T> {
    pub value: T,
    pub span: SourceSpan,
}

impl<T> Located<T> {
    /// Returns a new `Locaated` of a value `T`, representing
    /// its `span`/location in a source file.
    pub fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }

    /// maps over the `value` of the `Located`, preserving its span.
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Located<U> {
        Located {
            value: map(self.value),
            span: self.span,
        }
    }
}