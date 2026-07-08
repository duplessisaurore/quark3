//! The actual linker itself,
//! this runs all the linking and remapping
//! for all the files down into one

use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};

/// Errors that can occur during the linking process
pub struct LinkerError {
    kind: LinkerErrorKind,
    line: usize,
}

/// All kinds of errors that can occur during the linking process
pub enum LinkerErrorKind {
    /// There was an undefined name that could not be remapped
    UndefinedName { name: String },

    /// There was a file that did not declare its namspace
    UndeclaredNamespace { file: String },

    /// There was a duplicate namespace found, this is not allowed.
    DuplicateNamespace {
        // The namespace
        namespace: String,

        // These two files have duplicate namespaces
        file_a: String,
        file_b: String,
    },

    /// More than one namespace declared in a file!
    MoreThanOneNamespace { file: String },

    /// A file required a namespace that was not included
    /// in the linking process
    NamespaceNotFound {
        namespace: String,
        file_requires: String,
    },
}

/// A namespace, essentially just a string for
/// remapping things imported
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone)]
pub struct NameSpace(String);

/// The actual linker itself, this maps a set
/// of input Boson3 source files that import eachother
/// out into one final Boson3 file
pub struct Linker {
    sources: Vec<LinkableFile>,

    /// The output linked file
    output: Vec<String>,

    /// All declared namespaces
    namespaces: HashSet<NameSpace>,

    // Mapping of the name space to the filename that declared it
    namespace_map: HashMap<NameSpace, String>,
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
        Self {
            sources,
            output: Vec::new(),
            namespaces: HashSet::new(),
            namespace_map: HashMap::new(),
        }
    }

    /// Links all the files together, returns the outputted linked
    /// together `Boson3` file.
    pub fn link(mut self) -> Result<String, Vec<LinkerError>> {
        self.collect_namespaces().map_err(|err| vec![err])?;

        Ok(self.output.join("\n"))
    }

    /// Collects all the namespaces from all files and prevents duplicates.
    fn collect_namespaces(&mut self) -> Result<(), LinkerError> {
        for file in &self.sources {
            let namespace = self.find_namespace(file)?;

            // Check for duplicates
            if self.namespaces.contains(&namespace) {
                let original = self
                    .namespace_map
                    .get(&namespace)
                    .expect("expected namespace map and namespaces to be synced");
                return Err(LinkerErrorKind::DuplicateNamespace {
                    namespace: namespace.0,
                    file_a: original.clone(),
                    file_b: file.full_file_name.clone(),
                }
                .with_line(0));
            }

            // Add to maps
            self.namespaces.insert(namespace.clone());
            self.namespace_map
                .insert(namespace, file.full_file_name.clone());
        }

        Ok(())
    }

    /// Checks that all requires in a file are declared
    /// in the `namespaces`
    fn check_requires(&self, file: &LinkableFile) -> Result<(), Vec<LinkerError>> {
        let mut errors = Vec::new();

        for (line_number, line) in file.file_contents.lines().enumerate() {
            let line_number = line_number + 1;

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();

            // Search for the requires directive in the file.
            match tokens.as_slice() {
                ["@requires", namespace] => {
                    // Make sure the namespace is defined.
                    if !self.namespaces.contains(&NameSpace(namespace.to_string())) {
                        errors.push(
                            LinkerErrorKind::NamespaceNotFound {
                                namespace: namespace.to_string(),
                                file_requires: file.full_file_name.clone(),
                            }
                            .with_line(line_number),
                        );
                    }
                }

                _ => {}
            }
        }

        if errors.len() == 0 {
            return Ok(())
        }

        Err(errors)
    }

    /// Finds the namespace used for a file
    fn find_namespace(&self, file: &LinkableFile) -> Result<NameSpace, LinkerError> {
        let mut found_namespace = None;

        for line in file.file_contents.lines() {
            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();

            // Search for the namespace directive in the file.
            match tokens.as_slice() {
                ["@namespace", namespace] => {
                    if found_namespace.is_none() {
                        found_namespace = Some(NameSpace(namespace.to_string()));
                    } else {
                        return Err(LinkerErrorKind::MoreThanOneNamespace {
                            file: file.full_file_name.clone(),
                        }
                        .with_line(0));
                    }
                }

                _ => {}
            }
        }

        found_namespace.ok_or_else(|| {
            LinkerErrorKind::UndeclaredNamespace {
                file: file.full_file_name.clone(),
            }
            .with_line(0)
        })
    }

    /// Inserts a @loc directive at the current position with the source
    /// being the original boson3 file
    fn insert_loc(&mut self, filename: &str, line_number: usize) {
        self.output
            .push(format!("@loc {} {line_number} 0", filename))
    }

    fn push_out(&mut self, file_name: &str, contents: String, line_number: usize) {
        self.insert_loc(file_name, line_number);
        self.output.push(contents);
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
            Self::DuplicateNamespace {
                namespace,
                file_a,
                file_b,
            } => {
                write!(
                    f,
                    "The duplicate namespace `{namespace}` was declared in both `{file_a}` and `{file_b}`"
                )
            }
            Self::MoreThanOneNamespace { file } => {
                write!(
                    f,
                    "The file `{file}` contains more than one namespace declaration"
                )
            }
            Self::UndeclaredNamespace { file } => {
                write!(f, "The file `{file}` did not declare a namespace")
            }
            Self::NamespaceNotFound { namespace, file_requires } => {
                write!(f, "The file `{file_requires}` @requires the namespace `{namespace}` but it was not found")
            }
        }
    }
}

impl Display for LinkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.kind)
    }
}
