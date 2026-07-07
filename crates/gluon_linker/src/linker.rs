//! The actual linker itself,
//! this runs all the linking and remapping
//! for all the files down into one

use std::{collections::HashMap, fmt::Display};

/// Errors that can occur during the linking process
pub struct LinkerError {
    kind: LinkerErrorKind,
    line: usize,
}

/// All kinds of errors that can occur during the linking process
pub enum LinkerErrorKind {
    /// There was an undefined name that could not be remapped
    UndefinedName { name: String },
}

/// The actual linker itself, this maps a set
/// of input Boson3 source files that import eachother
/// out into one final Boson3 file
pub struct Linker {
    sources: HashMap<(String, String), String>,
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
    pub full_file_name: String,
}

impl Linker {
    /// Creates a new linker that will link all of these source files together
    pub fn new(sources: Vec<LinkableFile>) -> Self {
        let mut source_map = HashMap::new();

        for source in sources {
            source_map.insert(
                (source.full_file_name, source.file_name),
                source.file_contents,
            );
        }

        Self {
            sources: source_map,
        }
    }

    /// Links all the files together, returns the outputted linked
    /// together `Boson3` file.
    pub fn link(self) -> Result<String, LinkerError> {
        let mut output = Vec::new();

        for ((long_name, file), source) in self.sources.into_iter() {
            let mut globals_map = HashMap::new();
            let mut function_map = HashMap::new();
            let mut capability_map = HashMap::new();
            let mut object_map = HashMap::new();

            let namespace = extract_namespace(&file);

            // First pass, gather all renamed elements
            for line in source.lines() {
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
                    _ => {}
                }
            }

            // Second pass, remap everything now with gathered names
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
                    // These all have name as first arg and need to be remapped.
                    remap_directive
                        if remap_directive[0].starts_with("@capability")
                            || remap_directive[0].starts_with("@object")
                            || remap_directive[0].starts_with("@global")
                            || remap_directive[0].starts_with("@fn")
                            || remap_directive[0].starts_with("@entry") =>
                    {
                        // ???, let another phase handle
                        if remap_directive.len() < 2 {
                            output.push(remap_directive.join(" "));
                        }

                        let remap_type = remap_directive[0];
                        let name = remap_directive[1];

                        // Map to the remapped name for this.
                        let remapped_name = match remap_type {
                            "@capability" => capability_map.get(name).ok_or_else(|| {
                                LinkerErrorKind::UndefinedName {
                                    name: name.to_string(),
                                }
                                .with_line(line_number)
                            })?,

                            "@object" => object_map.get(name).ok_or_else(|| {
                                LinkerErrorKind::UndefinedName {
                                    name: name.to_string(),
                                }
                                .with_line(line_number)
                            })?,

                            "@global" => globals_map.get(name).ok_or_else(|| {
                                LinkerErrorKind::UndefinedName {
                                    name: name.to_string(),
                                }
                                .with_line(line_number)
                            })?,

                            "@fn" => function_map.get(name).ok_or_else(|| {
                                LinkerErrorKind::UndefinedName {
                                    name: name.to_string(),
                                }
                                .with_line(line_number)
                            })?,

                            "@entry" => function_map.get(name).ok_or_else(|| {
                                LinkerErrorKind::UndefinedName {
                                    name: name.to_string(),
                                }
                                .with_line(line_number)
                            })?,

                            _ => unreachable!(),
                        }
                        .to_string();

                        let mut new_directive = (*remap_directive).to_vec();
                        new_directive[1] = &remapped_name;

                        output.push(new_directive.join(" ").to_string());
                    }

                    // globals remapping
                    [
                        op @ ("store.global" | "load.global" | "log" | "stg"),
                        global,
                    ] => {
                        let new_global_name = globals_map.get(*global).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: global.to_string(),
                            }
                            .with_line(line_number)
                        })?;

                        push_out(
                            &long_name,
                            format!("{op} {new_global_name}"),
                            line_number,
                            &mut output,
                        );
                    }

                    // object remapping
                    [op @ ("object.new" | "onw"), object] => {
                        let new_object_name = object_map.get(*object).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: object.to_string(),
                            }
                            .with_line(line_number)
                        })?;

                        push_out(
                            &long_name,
                            format!("{op} {new_object_name}"),
                            line_number,
                            &mut output,
                        );
                    }

                    [op @ ("object.set" | "ost" | "object.get" | "ogt"), object] => {
                        // We the field and object name (2 elements)
                        let split_access = object.split(".").collect::<Vec<_>>();

                        // Fail in lowering.
                        if split_access.len() != 2 {
                            continue;
                        }

                        let object_name = split_access[0];

                        let new_object_name = object_map.get(object_name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: object_name.to_string(),
                            }
                            .with_line(line_number)
                        })?;

                        push_out(
                            &long_name,
                            format!("{op} {new_object_name}"),
                            line_number,
                            &mut output,
                        );
                    }

                    // function remapping
                    [op @ ("call" | "cal" | "tail.call" | "tcl"), function] => {
                        let new_function_name = function_map.get(*function).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: function.to_string(),
                            }
                            .with_line(line_number)
                        })?;

                        push_out(
                            &long_name,
                            format!("{op} {new_function_name}"),
                            line_number,
                            &mut output,
                        );
                    }

                    // capabilities remapping
                    [op @ ("call.cap" | "cap"), capability] => {
                        let new_capability_name =
                            capability_map.get(*capability).ok_or_else(|| {
                                LinkerErrorKind::UndefinedName {
                                    name: capability.to_string(),
                                }
                                .with_line(line_number)
                            })?;

                        push_out(
                            &long_name,
                            format!("{op} {new_capability_name}"),
                            line_number,
                            &mut output,
                        );
                    }

                    // Directives cant have @loc attached
                    directive if directive.iter().any(|tok| tok.starts_with("@")) => {
                        output.push(directive.join(" "))
                    }

                    // Non-remappable things
                    other => push_out(&long_name, other.join(" "), line_number, &mut output),
                }
            }
        }

        Ok(output.join("\n"))
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

impl LinkerErrorKind {
    /// Adds a line to this `LinkerErrorKind` turning it into a
    /// `LinkerError`.
    pub fn with_line(self, line_number: usize) -> LinkerError {
        LinkerError {
            line: line_number,
            kind: self,
        }
    }
}

impl Display for LinkerErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UndefinedName { name } => {
                write!(f, "An undefined name was referenced: `{name}`")
            }
        }
    }
}

impl Display for LinkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.kind)
    }
}

/// Inserts a @loc directive at the current position with the source
/// being the original boson3 file
fn insert_loc(filename: &str, line_number: usize, output: &mut Vec<String>) {
    output.push(format!("@loc {} {line_number} 0", filename))
}

fn push_out(file_name: &str, contents: String, line_number: usize, output: &mut Vec<String>) {
    insert_loc(file_name, line_number, output);
    output.push(contents);
}
