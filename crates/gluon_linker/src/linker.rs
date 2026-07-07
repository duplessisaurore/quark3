//! The actual linker itself,
//! this runs all the linking and remapping
//! for all the files down into one

use std::collections::HashMap;

/// The actual linker itself, this maps a set
/// of input Boson3 source files that import eachother
/// out into one final Boson3 file
pub struct Linker {
    sources: HashMap<String, String>,
}

/// The linkable file, this contains
/// a file that can be linked together with other files
///
/// The file contents here are the actual contents of
/// the file under `file_name`.
///
/// The `file_name` should just be the absoslute file name
/// of this file with the extension.
pub struct LinkableFile {
    pub file_contents: String,
    pub file_name: String,
}

impl Linker {
    /// Creates a new linker that will link all of these source files together
    pub fn new(sources: Vec<LinkableFile>) -> Self {
        let mut source_map = HashMap::new();

        for source in sources {
            source_map.insert(source.file_name, source.file_contents);
        };

        Self {
            sources: source_map
        }
    }


    /// Links all the files together, returns the outputted linked
    /// together `Boson3` file.
    pub fn link(mut self) -> String {
        String::new()
    }
}