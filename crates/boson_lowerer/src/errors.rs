/// All errors that can occur during the `Boson3`
/// lowering pass


/// All possible error kinds that can occurr
/// during the lowering process
pub enum LoweringErrorKind {
    // Unclosed if/loop/try block in the code
    UnclosedConstruct,
}

/// Located version of `LoweringErrorKind`
pub struct LoweringError {
    kind: LoweringErrorKind,

    // Line in the input source file.
    line: usize
}