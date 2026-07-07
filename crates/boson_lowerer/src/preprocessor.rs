/// The actual preprocessor itself,
/// this runs all the preprocessing functions and lowers
/// elements down to `Quark3`.

use std::collections::HashMap;

use crate::errors::LoweringError;

/// The actual lowerer itself.
/// 
/// This is responsible for doing all the desugaring
/// and preprocessing in `Boson3`
#[derive(Debug)]
pub struct BosonLowerer<'source> {
    // Globals tracking, all of the globals are just a simple replace
    globals: HashMap<String, u64>,

    // Capability tracking of cap name to index
    capabilities: HashMap<String, u64>,

    // Object fields tracking, of object name to field name to index.
    object_fields: HashMap<String, HashMap<String, u64>>,

    // trackers for construct ids
    if_count: usize,
    loop_count: usize,
    try_count: usize,

    // The current locals for a function mapping
    locals: HashMap<String, u64>,

    // Stack of the currently opened constructs,
    // we need to track all of the differing constructs
    // to match labels properly
    blocks: Vec<Block>,

    // The input source file that we are desugaring
    source: &'source str,

    // The current outputted lines of `Quark3`
    out: Vec<String>,
}


/// A structured construct that is currently "open"
#[derive(Debug)]
enum Block {
    If {
        id: usize,
        seen_else: bool,
        cond_jump: usize,
    },
    Loop {
        id: usize,
    },
    Try {
        id: usize,
        seen_catch: bool,
    },
}

impl<'source> BosonLowerer<'source> {
    /// Creates a new `Boson3` Lowerer, this is responsible
    /// for lowering the `Boson3` sugared quark3 code down
    /// into quark3.
    pub fn new(source: &'source str) -> Self {
        Self {
            globals: HashMap::new(),
            capabilities: HashMap::new(),
            object_fields: HashMap::new(),
            if_count: 0,
            loop_count: 0,
            try_count: 0,
            locals: HashMap::new(),
            blocks: Vec::new(),
            source,
            out: Vec::new()
        }
    }

    /// Lower a complete `Boson3` source file to `Quark3` source.
    pub fn lower(mut self) -> Result<String, LoweringError> {
        let mut text = self.out.join("\n");
        text.push('\n');
        Ok(text)
    }

}
