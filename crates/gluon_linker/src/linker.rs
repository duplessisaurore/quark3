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
        }

        Self {
            sources: source_map,
        }
    }

    /// Links all the files together, returns the outputted linked
    /// together `Boson3` file.
    pub fn link(self) -> String {
        let mut output = Vec::new();

        for (file, source) in self.sources.into_iter() {
            let mut globals_map = HashMap::new();
            let mut function_map = HashMap::new();
            let mut capability_map = HashMap::new();
            let mut object_map = HashMap::new();
            
            let namespace = extract_namespace(&file);

            // First pass, gather all renamed elements
            for (line_number, line) in source.lines().enumerate() {
                let line_number = line_number + 1;

                // Strip the comment from a line and ignore if empty, this means
                // we only parse actual tokens
                let line = strip_comment(line).trim();

                if line.is_empty() {
                    continue;
                }

                let tokens: Vec<&str> = line.split_whitespace().collect();

                match tokens.as_slice() {
                    // @global <name>
                    ["@global", name] => {
                        let remapped = format!("{}::{}", namespace, name);
                        globals_map.insert(name.to_string(), remapped);
                    }

                    // @fn <name>
                    function if function.iter().any(|tok| tok.starts_with("@fn")) => {
                        // Function needs at least @fn and <name> to be remapped.
                        if function.len() < 2 {
                            continue;
                        };

                        let name = function[1];

                        let remapped = format!("{}::{}", namespace, name);
                        function_map.insert(name.to_string(), remapped);
                    }

                    // @object <name>
                    object if object.iter().any(|tok| tok.starts_with("@object")) => {
                        // Object needs at least @object and <name> to be remapped.
                        if object.len() < 2 {
                            continue;
                        };

                        let name = object[1];

                        let remapped = format!("{}::{}", namespace, name);
                        object_map.insert(name.to_string(), remapped);
                    }

                    // @capability <name>
                    capability if capability.iter().any(|tok| tok.starts_with("@capability")) => {
                        // Capabilities need at least @capability and <name> to be remapped.
                        if capability.len() < 2 {
                            continue;
                        };

                        let name = capability[1];

                        let remapped = format!("{}::{}", namespace, name);
                        capability_map.insert(name.to_string(), remapped);
                    }

                    // Non-remappable things
                    other => {}
                }
            }

            
        }

        output.join("\n")
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

/// Extracts the to-use namespace from a filename
fn extract_namespace(file_name: &str) -> String {
    let path = std::path::Path::new(file_name);
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
        .to_string()
}
