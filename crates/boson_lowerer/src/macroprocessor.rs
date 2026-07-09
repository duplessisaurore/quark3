//! The actual macro processor itself,
//! this runs all the macro functions in `Boson3`

use std::collections::HashMap;

use crate::errors::LoweringError;

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

/// The macro expander itself
/// 
/// This takes some source input and outputs a macro-expanded
/// version of the source with all `@macro` directives removed.
#[derive(Debug)]
pub struct MacroExpander<'source> {
    /// All collected macro definitions by name.
    macros: HashMap<String, Macro>,

    /// The input source file we are expanding.
    source: &'source str,

    /// The expanded output lines.
    out: Vec<String>,

    /// How many cycles have run so far of expansion
    expansions: u64,

    /// The maximum allowed number of expansions to permit
    /// before erroring
    max_expansions: u64
}

impl<'source> MacroExpander<'source> {
    /// Creates a new `Boson3` macro expander, this is responsible
    /// for expanding the `Boson3` macros out
    pub fn new(source: &'source str, max_expansions: u64) -> Self {
        Self {
            macros: HashMap::new(),
            source,
            out: Vec::new(),
            expansions: 0,
            max_expansions
        }
    }

    /// Expand a complete `Boson3` source file's macros
    pub fn expand(mut self) -> Result<String, LoweringError> {
        // Collect all of the @macro definitions
        self.collect()?;

        let mut text = self.out.join("\n");
        text.push('\n');
        Ok(text)
    }

    /// Collects every @macro ... @end definition out of the source into
    /// the macro table
    /// 
    /// This will then be used for macro expansion later during the "recursive" phase
    fn collect(&mut self) -> Result<(), LoweringError> {
        for (line_number, line) in self.source.lines().enumerate() {
            let line_number = line_number + 1;

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();

            match tokens.as_slice() {
                // Non-macro definitions, ignore !
                _ => {}
            }
        }

        Ok(())
    }

}

/// Strip a line comment from a line
fn strip_comment(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
}
