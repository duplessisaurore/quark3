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
    file: String,
}

/// All kinds of errors that can occur during the linking process
pub enum LinkerErrorKind {
    /// There was an undefined name that could not be remapped
    UndefinedName { name: String },

    /// An unqualified name resolves to multiple imported namespaces
    ///
    /// This is not permitted, e.g which namespace do we actually resolve it to?
    AmbiguousUsing {
        name: String,
        first: String,
        second: String,
    },

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

    /// Invalid source location due to invalid arguments
    InvalidSourceLocationDirective { reason: &'static str },
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

    // The global remapped valid symbols
    valid_symbols: HashSet<String>,

    // The filename to filename indxs for loc mapping
    loc_maps: HashMap<String, usize>,
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

    // The namespace for the file, this is found by the linker
    // and should be set to None by default.
    pub namespace: Option<NameSpace>,

    // This is built after the top level names have been remapped
    // for this linkable file
    pub remap_maps: Option<RemapMaps>,
}

/// The Remap-maps for a file which tells how the linker
/// should remap everything, this is built by remapping all
/// the top-level names using the namespace of the file
pub struct RemapMaps {
    // @global remap
    pub globals_map: HashMap<String, String>,

    // @fn remap
    pub function_map: HashMap<String, String>,

    // @capability remap
    pub capability_map: HashMap<String, String>,

    // @object remap
    pub object_map: HashMap<String, String>,

    // @macro remap
    pub macro_map: HashMap<String, String>,

    /// File-wide namespaces opened by `@using`.
    pub usings: Vec<String>,
}

/// The kind a symbol is, for mapping with
#[derive(Clone, Copy)]
enum SymbolKind {
    Global,
    Function,
    Capability,
    Object,
    Macro,
}

/// Index of symbols by their type
/// 
/// This is global across all files.
#[derive(Default)]
struct SymbolIndex {
    globals: HashSet<String>,
    functions: HashSet<String>,
    capabilities: HashSet<String>,
    objects: HashSet<String>,
    macros: HashSet<String>,
}

impl SymbolIndex {
    // Grabs all of the symbols in all of the provided linkable files
    // to produce one global symbol index for checking with
    fn from_sources(sources: &[LinkableFile]) -> Self {
        let mut index = Self::default();

        for file in sources {
            let maps = file
                .remap_maps
                .as_ref()
                .expect("expected all top-level remap maps to be resolved");

            // Extend the index with file specific remap maps
            index.globals.extend(maps.globals_map.values().cloned());
            index.functions.extend(maps.function_map.values().cloned());
            index
                .capabilities
                .extend(maps.capability_map.values().cloned());
            index.objects.extend(maps.object_map.values().cloned());
            index.macros.extend(maps.macro_map.values().cloned());
        }
        index
    }

    // Returns whether or not this `name` with this `kind` of symbol
    // exists in the full index of symbols
    fn contains(&self, kind: SymbolKind, name: &str) -> bool {
        match kind {
            SymbolKind::Global => self.globals.contains(name),
            SymbolKind::Function => self.functions.contains(name),
            SymbolKind::Capability => self.capabilities.contains(name),
            SymbolKind::Object => self.objects.contains(name),
            SymbolKind::Macro => self.macros.contains(name),
        }
    }
}

/// Resolves a symbol to the full symbol with a namespace
///
/// order is:
///
/// local
/// qualified name
/// using
///
fn resolve_symbol(
    name: &str,
    local_map: &HashMap<String, String>,
    usings: &[String],
    valid_symbols: &HashSet<String>,
    symbol_index: &SymbolIndex,
    kind: SymbolKind,
) -> Result<String, LinkerErrorKind> {
    // local first
    if let Some(local) = local_map.get(name) {
        return Ok(local.clone());
    }

    // Preserve fully qualified names already.
    if valid_symbols.contains(name) {
        return Ok(name.to_string());
    }

    // Qualified names can never be @using'd because
    // that means that namespace probably doesn't exist (rather than being @using'd)
    if name.contains("::") {
        return Err(LinkerErrorKind::UndefinedName {
            name: name.to_string(),
        });
    }

    // Try find matching candidate in a @using namespace
    let mut found: Option<String> = None;

    for namespace in usings {
        // Test if this candidate exists
        let candidate = format!("{namespace}::{name}");
        if !symbol_index.contains(kind, &candidate) {
            continue;
        }

        // Duplicate candidate match
        if let Some(first) = &found {
            return Err(LinkerErrorKind::AmbiguousUsing {
                name: name.to_string(),
                first: first.clone(),
                second: candidate,
            });
        }
        found = Some(candidate);
    }

    found.ok_or_else(|| LinkerErrorKind::UndefinedName {
        name: name.to_string(),
    })
}

impl Linker {
    /// Creates a new linker that will link all of these source files together
    pub fn new(sources: Vec<LinkableFile>) -> Self {
        Self {
            sources,
            output: Vec::new(),
            namespaces: HashSet::new(),
            namespace_map: HashMap::new(),
            valid_symbols: HashSet::new(),
            loc_maps: HashMap::new(),
        }
    }

    /// Links all the files together, returns the outputted linked
    /// together `Boson3` file.
    pub fn link(mut self) -> Result<String, Vec<LinkerError>> {
        // Create file index maps
        for (i, file) in self.sources.iter().enumerate() {
            self.loc_maps.insert(file.full_file_name.clone(), i);
        }

        self.collect_namespaces().map_err(|err| vec![err])?;
        self.check_all_requires()?;
        self.remap_all_top_levels()?;
        self.remap_all_instructions()?;

        // Create file table first
        self.output.push("// File table for locs\n".to_string());
        for (i, file) in self.sources.iter().enumerate() {
            self.output
                .push(format!("@file {} {}\n", i, file.full_file_name))
        }

        self.output.push("\n".to_string());

        // Combine all files together into the output
        for file in self.sources {
            self.output.push(format!(
                "// Linked namespace `{}` from file `{}`\n",
                file.namespace.expect("namespace to be resolved").0,
                file.full_file_name.clone()
            ));
            self.output.push(file.file_contents);
            self.output.push("\n\n".to_string());
        }

        Ok(self.output.join(""))
    }

    /// Remap all of the top-level names in all files
    fn remap_all_top_levels(&mut self) -> Result<(), Vec<LinkerError>> {
        // The full remap-map generated by this phase
        let mut remap_map = std::mem::take(&mut self.valid_symbols);
        let mut errors = Vec::new();

        // Remap each file
        for file in &mut self.sources {
            if let Err(this_errors) = Linker::remap_top_levels(&mut remap_map, file) {
                errors.extend(this_errors);
            }
        }

        // Put back for remap instructions
        self.valid_symbols = remap_map;

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors)
    }

    /// Remap all of the instructions in all files
    fn remap_all_instructions(&mut self) -> Result<(), Vec<LinkerError>> {
        // The full valid map for matching on each source file
        let valid_map = std::mem::take(&mut self.valid_symbols);

        // Full index of symbols across all files.
        let symbol_index = SymbolIndex::from_sources(&self.sources);

        let mut errors = Vec::new();

        // Remap each file's instructions
        for file in &mut self.sources {
            if let Err(this_errors) =
                Linker::remap_instructions(&self.loc_maps, &valid_map, &symbol_index, file)
            {
                errors.extend(this_errors);
            }
        }

        // Put back for fun
        self.valid_symbols = valid_map;

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors)
    }

    /// Remaps all the instructions in a file
    ///
    /// This assumes the remap map was already defined in the file
    fn remap_instructions(
        loc_maps: &HashMap<String, usize>,
        valid_symbols: &HashSet<String>,
        symbol_index: &SymbolIndex,
        file: &mut LinkableFile,
    ) -> Result<(), Vec<LinkerError>> {
        // The new file contents after remapping
        let mut output = Vec::new();
        let mut errors = Vec::new();

        // Find whether or not this file contains the @source_loc directive, which
        // means the automatic linker loc's wont be inserted and instead based off the @source_loc's.
        let has_source_locs = file.file_contents.lines().any(|line| {
            matches!(
                strip_comment(line)
                    .trim()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["@source_loc", _, _]
            )
        });

        // The index of this file into the files table, which we use for @loc's
        let file_index = get_file_idx(loc_maps, &file.full_file_name);

        // The current parameters of the macro body we are walking if any,
        // we need to NOT remap macro params inserted in-line and stuff so this is for that!
        let mut macro_params: Option<Vec<String>> = None;

        // Remap-maps
        let remap_maps = file
            .remap_maps
            .as_ref()
            .expect("expected remap maps to already have been gathered for file");

        let globals_map = &remap_maps.globals_map;
        let function_map = &remap_maps.function_map;
        let capability_map = &remap_maps.capability_map;
        let object_map = &remap_maps.object_map;
        let macro_map = &remap_maps.macro_map;
        let usings = &remap_maps.usings;

        let file_namespace = &file
            .namespace
            .as_ref()
            .expect("expected namespace to already have been gathered for file")
            .0;

        // Set by @source_loc, applies to every following line until the next @source_loc.
        let mut current_sloc_line = None;

        // The pass, remap all instructions now with names
        for (line_number, line) in file.file_contents.lines().enumerate() {
            let line_number = match current_sloc_line {
                None => line_number + 1,
                Some(sloc_line_num) => sloc_line_num,
            };

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            let blocked_line = line
                .replace("{", "\n{\n")
                .replace("}", "\n}")
                .replace("self::", &format!("{file_namespace}::"));

            for line in blocked_line.lines() {
                let tokens: Vec<&str> = line.split_whitespace().collect();

                match tokens.as_slice() {
                    // This is a special loc marker emitted by higher layers
                    // which we translate down into real-loc's for this file
                    ["@source_loc", source_line, source_col] => {
                        let source_line = match source_line.parse::<usize>().map_err(|_| {
                            LinkerErrorKind::InvalidSourceLocationDirective {
                                reason: "invalid line",
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }) {
                            Ok(line) => line,
                            Err(error) => {
                                errors.push(error);
                                continue;
                            }
                        };
                        let source_col = match source_col.parse::<usize>().map_err(|_| {
                            LinkerErrorKind::InvalidSourceLocationDirective {
                                reason: "invalid column",
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }) {
                            Ok(col) => col,
                            Err(error) => {
                                errors.push(error);
                                continue;
                            }
                        };

                        current_sloc_line = Some(source_line);
                        output.push(format!("@loc {file_index} {source_line} {source_col}"));
                    }

                    // @macro <name> (<param>, ...) opens a macro body,
                    // we should be careful to not replace any of the params with remapped things
                    ["@macro", _, params @ ..] => {
                        let param_names = params
                            .join(" ")
                            .replace(['(', ')', ','], " ")
                            .split_whitespace()
                            .map(|name| format!("${name}"))
                            .collect();

                        macro_params = Some(param_names);
                        output.push(tokens.join(" "));
                    }

                    // @end closes a macro body
                    ["@end", ..] => {
                        macro_params = None;
                        output.push(tokens.join(" "));
                    }

                    // globals remapping
                    [
                        op @ ("store.global" | "load.global" | "log" | "stg"),
                        global,
                    ] => {
                        // Do not overwrite macro params!
                        if let Some(params) = &macro_params
                            && params.iter().any(|param| param == global)
                        {
                            output.push(format!("{op} {global}"));
                            continue;
                        }

                        // Get the name from the map, else test if its in the valid symbols map.
                        let new_global_name = match resolve_symbol(
                            global,
                            globals_map,
                            usings,
                            valid_symbols,
                            symbol_index,
                            SymbolKind::Global,
                        ) {
                            Ok(name) => name,
                            Err(kind) => {
                                errors
                                    .push(kind.with_line(line_number, file.full_file_name.clone()));
                                continue;
                            }
                        };

                        push_source_aware(
                            file_index,
                            format!("{op} {new_global_name}"),
                            line_number,
                            &mut output,
                            has_source_locs,
                        );
                    }

                    // object remapping
                    [
                        op @ ("object.new" | "onw" | "@push.object" | "object.type.tag"),
                        object,
                    ] => {
                        // Do not overwrite macro params!
                        if let Some(params) = &macro_params
                            && params.iter().any(|param| param == object)
                        {
                            output.push(format!("{op} {object}"));
                            continue;
                        }

                        // @raw is passthrough to quark3
                        let new_object_name = if object != &"@raw" {
                            // not @raw, resolve actual object name
                            match resolve_symbol(
                                object,
                                object_map,
                                usings,
                                valid_symbols,
                                symbol_index,
                                SymbolKind::Object,
                            ) {
                                Ok(name) => name,
                                Err(kind) => {
                                    errors.push(
                                        kind.with_line(line_number, file.full_file_name.clone()),
                                    );
                                    continue;
                                }
                            }
                        } else {
                            "@raw".to_string()
                        };

                        push_source_aware(
                            file_index,
                            format!("{op} {new_object_name}"),
                            line_number,
                            &mut output,
                            has_source_locs,
                        );
                    }

                    [op @ ("object.set" | "ost" | "object.get" | "ogt"), object] => {
                        // Do not overwrite macro params! (they dont have dots anyway)
                        if let Some(params) = &macro_params
                            && params.iter().any(|param| param == object)
                        {
                            output.push(format!("{op} {object}"));
                            continue;
                        }

                        // We the field and object name (2 elements)
                        let split_access = object.split(".").collect::<Vec<_>>();

                        // Fail in lowering.
                        if split_access.len() != 2 {
                            output.push(tokens.join(" "));
                            continue;
                        }

                        let object_name = split_access[0];
                        let field = split_access[1];

                        // @raw is passthrough to quark3
                        let new_object_name = if object != &"@raw" {
                            // not @raw, resolve actual object name
                            match resolve_symbol(
                                object_name,
                                object_map,
                                usings,
                                valid_symbols,
                                symbol_index,
                                SymbolKind::Object,
                            ) {
                                Ok(name) => name,
                                Err(kind) => {
                                    errors.push(
                                        kind.with_line(line_number, file.full_file_name.clone()),
                                    );
                                    continue;
                                }
                            }
                        } else {
                            "@raw".to_string()
                        };

                        push_source_aware(
                            file_index,
                            format!("{op} {new_object_name}.{field}"),
                            line_number,
                            &mut output,
                            has_source_locs,
                        );
                    }

                    // function remapping
                    [
                        op @ ("call" | "cal" | "tail.call" | "tcl" | "@push.fn"),
                        function,
                    ] => {
                        // Do not overwrite macro params!
                        if let Some(params) = &macro_params
                            && params.iter().any(|param: &String| param == function)
                        {
                            output.push(format!("{op} {function}"));
                            continue;
                        }

                        // @raw is passthrough to quark3
                        let new_function_name = if function != &"@raw" {
                            // not @raw, lookup map
                            match resolve_symbol(
                                function,
                                function_map,
                                usings,
                                valid_symbols,
                                symbol_index,
                                SymbolKind::Function,
                            ) {
                                Ok(name) => name,
                                Err(kind) => {
                                    errors.push(
                                        kind.with_line(line_number, file.full_file_name.clone()),
                                    );
                                    continue;
                                }
                            }
                        } else {
                            "@raw".to_string()
                        };

                        push_source_aware(
                            file_index,
                            format!("{op} {new_function_name}"),
                            line_number,
                            &mut output,
                            has_source_locs,
                        );
                    }

                    // capabilities remapping
                    [op @ ("call.cap" | "cap"), capability] => {
                        // Do not overwrite macro params!
                        if let Some(params) = &macro_params
                            && params.iter().any(|param| param == capability)
                        {
                            output.push(format!("{op} {capability}"));
                            continue;
                        }

                        let new_capability_name = match resolve_symbol(
                            capability,
                            capability_map,
                            usings,
                            valid_symbols,
                            symbol_index,
                            SymbolKind::Capability,
                        ) {
                            Ok(name) => name,
                            Err(kind) => {
                                errors
                                    .push(kind.with_line(line_number, file.full_file_name.clone()));
                                continue;
                            }
                        };

                        push_source_aware(
                            file_index,
                            format!("{op} {new_capability_name}"),
                            line_number,
                            &mut output,
                            has_source_locs,
                        );
                    }

                    // A macro invocation remapping
                    [invocation, args @ ..] if invocation.starts_with("!") => {
                        // The name of the macro
                        let name = invocation.strip_prefix("!").unwrap_or(*invocation);

                        // Find new remapped name
                        let new_macro_name = match resolve_symbol(
                            name,
                            macro_map,
                            usings,
                            valid_symbols,
                            symbol_index,
                            SymbolKind::Macro,
                        ) {
                            Ok(name) => name,
                            Err(kind) => {
                                errors
                                    .push(kind.with_line(line_number, file.full_file_name.clone()));
                                continue;
                            }
                        };

                        // Rebuild new macro invocation with all the arguments
                        let mut rebuilt = vec![format!("!{new_macro_name}")];
                        rebuilt.extend(args.iter().map(|token| token.to_string()));

                        push_source_aware(
                            file_index,
                            rebuilt.join(" "),
                            line_number,
                            &mut output,
                            has_source_locs,
                        );
                    }

                    // Directives cant have @loc attached
                    directive if directive.iter().any(|tok| tok.starts_with("@")) => {
                        output.push(directive.join(" "))
                    }

                    // Non-remappable things
                    other => push_source_aware(
                        file_index,
                        other.join(" "),
                        line_number,
                        &mut output,
                        has_source_locs,
                    ),
                }
            }
        }

        file.file_contents = output.join("\n");

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors)
    }

    /// Remaps all the top-level names in a file
    ///
    /// This assumes namespace was already defined in the file
    fn remap_top_levels(
        valid_map: &mut HashSet<String>,
        file: &mut LinkableFile,
    ) -> Result<(), Vec<LinkerError>> {
        // The new file contents after remapping
        let mut output = Vec::new();
        let mut errors = Vec::new();

        // Remap-maps
        let mut globals_map = HashMap::new();
        let mut function_map = HashMap::new();
        let mut capability_map = HashMap::new();
        let mut object_map = HashMap::new();
        let mut macro_map = HashMap::new();
        let mut usings = Vec::new();

        let file_namespace = file
            .namespace
            .clone()
            .expect("expected namespace to already have been gathered for file")
            .0;

        // First pass, gather all renamed elements
        for line in file.file_contents.lines() {
            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();

            match tokens.as_slice() {
                // @using opens a namespace for all references in this file.
                ["@using", namespace] => {
                    if !usings
                        .iter()
                        .any(|used: &String| used.as_str() == *namespace)
                    {
                        usings.push(namespace.to_string());
                    }
                }
                // @global <name>
                ["@global", name] => {
                    let remapped = format!("{}::{}", file_namespace, name);
                    globals_map.insert(name.to_string(), remapped);
                }

                // @fn <name>
                ["@fn", name, ..] => {
                    let remapped = format!("{}::{}", file_namespace, name);
                    function_map.insert(name.to_string(), remapped);
                }

                // @object <name>
                ["@object", name, ..] => {
                    let remapped = format!("{}::{}", file_namespace, name);
                    object_map.insert(name.to_string(), remapped);
                }

                // @capability <name>
                ["@capability", name, ..] => {
                    let remapped = format!("{}::{}", file_namespace, name);
                    capability_map.insert(name.to_string(), remapped);
                }

                // @macro <name> (args)
                ["@macro", name, ..] => {
                    let remapped = format!("{}::{}", file_namespace, name);
                    macro_map.insert(name.to_string(), remapped);
                }

                // Non-remappable things
                _ => {}
            }
        }

        // Second pass, remap everything now with gathered names
        for (line_number, line) in file.file_contents.lines().enumerate() {
            let line_number = line_number + 1;

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                output.push(String::new());
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
                        || remap_directive[0].starts_with("@entry")
                        || remap_directive[0].starts_with("@macro") =>
                {
                    // ???, let another phase handle
                    if remap_directive.len() < 2 {
                        output.push(remap_directive.join(" "));
                        continue;
                    }

                    let remap_type = remap_directive[0];
                    let name = remap_directive[1];

                    // Map to the remapped name for this.
                    let remapped_name = match remap_type {
                        "@capability" => capability_map.get(name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: name.to_string(),
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }),

                        "@object" => object_map.get(name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: name.to_string(),
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }),

                        "@global" => globals_map.get(name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: name.to_string(),
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }),

                        "@fn" => function_map.get(name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: name.to_string(),
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }),

                        "@macro" => macro_map.get(name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: name.to_string(),
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }),

                        "@entry" => function_map.get(name).ok_or_else(|| {
                            LinkerErrorKind::UndefinedName {
                                name: name.to_string(),
                            }
                            .with_line(line_number, file.full_file_name.clone())
                        }),

                        _ => unreachable!(),
                    };

                    // Build errors
                    let remapped_name = match remapped_name {
                        Ok(remapped_name) => remapped_name,
                        Err(error) => {
                            output.push(String::new());
                            errors.push(error);
                            continue;
                        }
                    };

                    let mut new_directive = (*remap_directive).to_vec();
                    new_directive[1] = remapped_name;

                    output.push(new_directive.join(" ").to_string());
                }

                // These are already consumed earlier, we ignore them now
                consumed
                    if consumed[0].starts_with("@namespace")
                        || consumed[0].starts_with("@requires")
                        || consumed[0].starts_with("@using") =>
                {
                    output.push(String::new());
                }

                // We leave all other remapping to after all files have remapped their top level elements.
                other => output.push(other.join(" ")),
            }
        }

        file.file_contents = output.join("\n");

        // Add all these remapped symbols to the list of valid symbols
        valid_map.extend(
            globals_map
                .values()
                .cloned()
                .chain(capability_map.values().cloned())
                .chain(function_map.values().cloned())
                .chain(object_map.values().cloned())
                .chain(macro_map.values().cloned()),
        );

        // Build the remap-map for this file
        let remap_map = RemapMaps {
            globals_map,
            function_map,
            capability_map,
            object_map,
            macro_map,
            usings,
        };

        file.remap_maps = Some(remap_map);

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors)
    }

    /// Collects all the namespaces from all files and prevents duplicates.
    fn collect_namespaces(&mut self) -> Result<(), LinkerError> {
        for file in &mut self.sources {
            let namespace = Linker::find_namespace(file)?;

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
                .with_line(0, file.full_file_name.clone()));
            }

            // Add to maps
            self.namespaces.insert(namespace.clone());
            self.namespace_map
                .insert(namespace, file.full_file_name.clone());
        }

        Ok(())
    }

    /// Checks that all requires in all files are declared
    fn check_all_requires(&self) -> Result<(), Vec<LinkerError>> {
        let mut errors = Vec::new();

        // Check on each file that the requires are declared
        for file in &self.sources {
            if let Err(this_errors) = self.check_requires(file) {
                errors.extend(this_errors);
            }
        }

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors)
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
            if let ["@requires" | "@using", namespace] = tokens.as_slice() {
                // Make sure the namespace is defined.
                if !self.namespaces.contains(&NameSpace(namespace.to_string())) {
                    errors.push(
                        LinkerErrorKind::NamespaceNotFound {
                            namespace: namespace.to_string(),
                            file_requires: file.full_file_name.clone(),
                        }
                        .with_line(line_number, file.full_file_name.clone()),
                    );
                }
            }
        }

        if errors.is_empty() {
            return Ok(());
        }

        Err(errors)
    }

    /// Finds the namespace used for a file
    fn find_namespace(file: &mut LinkableFile) -> Result<NameSpace, LinkerError> {
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
            if let ["@namespace", namespace] = tokens.as_slice() {
                if found_namespace.is_none() {
                    let namespace = Some(NameSpace(namespace.to_string()));
                    found_namespace = namespace.clone();
                    file.namespace = namespace;
                } else {
                    return Err(LinkerErrorKind::MoreThanOneNamespace {
                        file: file.full_file_name.clone(),
                    }
                    .with_line(0, file.full_file_name.clone()));
                }
            }
        }

        found_namespace.ok_or_else(|| {
            LinkerErrorKind::UndeclaredNamespace {
                file: file.full_file_name.clone(),
            }
            .with_line(0, file.full_file_name.clone())
        })
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
    pub fn with_line(self, line_number: usize, file: String) -> LinkerError {
        LinkerError {
            line: line_number,
            kind: self,
            file,
        }
    }
}

impl Display for LinkerErrorKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UndefinedName { name } => {
                write!(f, "An undefined name was referenced: `{name}`")
            }
            Self::AmbiguousUsing {
                name,
                first,
                second,
            } => {
                write!(
                    f,
                    "The name `{name}` is ambiguous due to namespace conflict between `{first}` and `{second}`, use a fully qualified name"
                )
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
            Self::NamespaceNotFound {
                namespace,
                file_requires,
            } => {
                write!(
                    f,
                    "The file `{file_requires}` references the namespace `{namespace}` via @requires or @using, but it was not found"
                )
            }
            Self::InvalidSourceLocationDirective { reason } => {
                write!(
                    f,
                    "A source location directive @source_loc was found to be invalid: `{reason}`!"
                )
            }
        }
    }
}

impl Display for LinkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}` in `{}`: {}", self.line, self.file, self.kind)
    }
}

/// Inserts a @loc directive at the current position with the source
/// being the original boson3 file
fn insert_loc(file_idx: usize, line_number: usize, output: &mut Vec<String>) {
    output.push(format!("@loc {file_idx} {line_number} 0"))
}

/// Pushes a line of output to the `output`, with a LOC attached.
///
/// This should be used for instructions, not directives.
fn push_out(file_idx: usize, contents: String, line_number: usize, output: &mut Vec<String>) {
    insert_loc(file_idx, line_number, output);
    output.push(contents);
}

// Emit an instruction after remapping while respecting any @source_loc's
///
/// Essentially any @source_loc's pass through the linker as "real" locs
/// rather than the linker inserting them automatically.
///
/// This is important as the linker really is only linking "b3" so any layers
/// above this linker would not be able to properly map back to it's layer without
/// @source_loc's.
fn push_source_aware(
    file_idx: usize,
    contents: String,
    generated_line_number: usize,
    output: &mut Vec<String>,
    has_source_locs: bool,
) {
    if has_source_locs {
        output.push(contents);
    } else {
        push_out(file_idx, contents, generated_line_number, output);
    }
}

/// Get the file index for this files name
fn get_file_idx(loc_maps: &HashMap<String, usize>, file_name: &str) -> usize {
    *loc_maps
        .get(file_name)
        .expect("expected file name to be already in file loc maps")
}
