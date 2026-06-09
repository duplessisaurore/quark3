//! A source map for mapping back disassembled functions/labels etc.
//! back into names

use alloc::{string::String, vec::Vec};

// The magic bytes expected at the beginning of a Quark3 source map
pub const MAGIC: &[u8] = b"QK3SMAP";

/// Maps a single label to its byte offset within a function's instruction stream
#[derive(Clone)]
pub struct LabelEntry {
    pub offset: u32,
    pub name: String,
}

/// Maps a single function index back to its name
#[derive(Clone)]
pub struct FunctionEntry {
    pub index: u32,
    pub name: String,

    /// Label entries, offsets relative to `instruction_offset`
    pub labels: Vec<LabelEntry>,
}

/// Maps a single object type index back to its name
#[derive(Clone)]
pub struct ObjectEntry {
    /// Index into the image's object table
    pub index: u32,
    pub name: String,
}

/// A source map for a Lepton3 image, allowing disassemblers to recover
/// function, label, and object type names from raw bytecode
#[derive(Clone)]
pub struct SourceMap {
    pub functions: Vec<FunctionEntry>,
    pub objects: Vec<ObjectEntry>,
}
