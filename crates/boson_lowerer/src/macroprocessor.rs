//! The actual macro processor itself,
//! this runs all the macro functions in `Boson3`

/// A @macro definition, this is essentially
/// some textual replacement form with some extra
/// logic for hygiene
#[derive(Debug)]
struct Macro {
    /// The parameter names of the macro
    params: Vec<String>,

    /// The body lines of the macro
    body: Vec<String>,

    /// The names introduced by the body which need to
    /// be remapped such as in labels and @locals
    introduced: Vec<String>,
}

/// An argument provided during the invocation of a macro
#[derive(Debug, Clone)]
enum MacroArg {
    /// A simple one-line token
    Token(String),

    /// A block of lines
    Block(Vec<String>),
}

