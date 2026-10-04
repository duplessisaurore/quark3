//! lowerer.rs
//!
//! This is the lowering component of the `Photon3` sugar layer,
//! responsible for:
//!
//! - taking the full ast
//! - resolving it down into std.b3 source code using its macros
//! - resolving types (simply)
//! - resolving whether things produce values or not and handling void appropriately with drop

use std::collections::HashMap;

use crate::ast::{QualifiedName, SourceSpan, TypeName};

/// The output of the lowering phase,
/// this is the lowered version of a module as a textual version
/// of the std.b3 macro library with boson3 macros
#[derive(Debug, Clone)]
pub struct LoweredModule {
    pub contents: String,
}

/// One lowered value, containing the code required
/// to produce the value, the type of the value expected
///
/// `Void` expressions must explicitly leave no values on the stack!
/// everything else must produce the value desired by the type.
struct LoweredExpression {
    code: String,
    value_type: TypeName,
}

/// The context of a function,
/// this is essentially a mapping of locals currently
/// in the context of the function to their expected type.
#[derive(Debug)]
struct FunctionContext {
    locals: HashMap<String, TypeName>,
}

/// The full symbol table for a module, which defines
/// all the functions and objects that can be referred to
///
/// (capabilities are missing because they are expected
/// to be invoked using the escape boson3 statement hatch).
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    functions: HashMap<QualifiedName, FunctionSignature>,
    objects: HashMap<QualifiedName, ObjectSignature>,
    globals: HashMap<QualifiedName, TypeName>,
}

/// An output source location which can be accepted by the gluon3 linker
/// with line and col info.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

/// Converts byte offsets from lexer/parser spans back into line/col
/// positions that the tokens actually occured at.
#[derive(Debug, Clone)]
pub struct SourceMap {
    line_starts: Vec<usize>,
}

/// One defined function in the symbol table that can be referred to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub name: QualifiedName,
    pub parameters: Vec<TypeName>,
    pub return_type: TypeName,
}

/// One defined object in the symbol table that can be referred to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSignature {
    pub name: QualifiedName,
    pub fields: Vec<(String, TypeName)>,
}

/// The actual lowerer itself, this is responsible
/// for handling the lowering from the produced `ast` to
/// boson3 source code with std.b3 included.
pub struct Lowerer<'symbols> {
    /// All of the introduced names/elements that
    /// can be referred to by a symbol in the current context
    /// globally (see: `FunctionContext`)
    symbols: &'symbols SymbolTable,
    source_map: SourceMap,
    namespace: QualifiedName,
}

impl LoweredExpression {
    /// Creates a new lowered expression, this should be some
    /// code that produces a value of some type.
    ///
    /// A `Void` type expression is one which is assumed to produce no value
    /// on the stack.
    fn value(code: impl Into<String>, value_type: TypeName) -> Self {
        Self {
            code: code.into(),
            value_type,
        }
    }

    /// Returns a lowered expresion that produces no value, essentially
    /// an expression of the Void type.
    fn no_value(code: impl Into<String>) -> Self {
        Self::value(code, TypeName::Void)
    }

    /// Returns whether or not this lowered expression produces a value
    /// e.g it is of the `Void` type.
    fn produces_value(&self) -> bool {
        self.value_type == TypeName::Void
    }
}

impl SourceMap {
    /// Creates a new source map over the provided source,
    /// this will map byte offsets back to lines based on the
    /// contents of `source`.
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];

        // Map all newline locations, so we get the byte offset of each newline (line starts)
        for (byte_offset, character) in source.char_indices() {
            if character == '\n' {
                line_starts.push(byte_offset + character.len_utf8());
            }
        }
        Self { line_starts }
    }

    /// Returns the source location of a byte offset into the
    /// provided `SourceMap`'s source
    pub fn location(&self, byte_offset: usize) -> SourceLocation {
        // Find the corresponding line that matches this byte offset
        let line_index = self
            .line_starts
            .partition_point(|line_start| *line_start <= byte_offset)
            .saturating_sub(1);

        let line_start = self.line_starts[line_index];

        // remaining offset is then offset into that line (col)
        SourceLocation {
            line: line_index + 1,
            column: byte_offset.saturating_sub(line_start),
        }
    }

    /// Turns a span into a source location based on the start of the span
    /// as the source location requested.
    pub fn span_start(&self, span: SourceSpan) -> SourceLocation {
        self.location(span.start)
    }
}

impl ObjectSignature {
    /// Returns the type of a field based on it's name (if it exists),
    /// otherwise None
    pub fn field_type(&self, field_name: &str) -> Option<&TypeName> {
        self.fields
            .iter()
            .find_map(|(name, field_type)| (name == field_name).then_some(field_type))
    }
}
