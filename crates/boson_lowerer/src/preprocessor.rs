/// The actual preprocessor itself,
/// this runs all the preprocessing functions and lowers
/// elements down to `Quark3`.
use std::collections::HashMap;

use crate::errors::{LoweringError, LoweringErrorKind};

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

    // The current locals for a function mapping
    locals: HashMap<String, u64>,

    // The input source file that we are desugaring
    source: &'source str,

    // The current outputted lines of `Quark3`
    out: Vec<String>,

    // The name of the file we are lowering,
    // to insert locs everywhere for debugging.
    filename: String,
}

impl<'source> BosonLowerer<'source> {
    /// Creates a new `Boson3` Lowerer, this is responsible
    /// for lowering the `Boson3` sugared quark3 code down
    /// into quark3.
    pub fn new(source: &'source str, filename: String) -> Self {
        Self {
            globals: HashMap::new(),
            capabilities: HashMap::new(),
            object_fields: HashMap::new(),
            locals: HashMap::new(),
            source,
            out: Vec::new(),
            filename,
        }
    }

    /// Lower a complete `Boson3` source file to `Quark3` source.
    pub fn lower(mut self) -> Result<String, LoweringError> {
        // Collect all the global table elements into the `Lowerer`
        self.collect()?;

        // Lower all the lines out
        self.lower_lines()?;

        let mut text = self.out.join("\n");
        text.push('\n');
        Ok(text)
    }

    /// The first part of the lowering process is collecting
    /// all the easy global names that we can just replace easily
    /// throughout the whole file
    ///
    /// These are the @global's, @capabilities and the @object fields.
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
                // @global <name> <slot>
                // Defines a new global slot for usage with load and store global
                ["@global", name, slot] => {
                    let name = name.to_string();
                    let slot = parse_u64(line_number, slot)?;
                    self.globals.insert(name, slot);
                }

                // @capability <name> <num>
                // Defines a new capability with this alias that refers
                // to the specific capability number
                ["@capability", name, num] => {
                    let name = name.to_string();
                    let num = parse_u64(line_number, num)?;
                    self.capabilities.insert(name, num);
                }

                // @object <name> <fields> (field_name, ...)
                other if other.iter().any(|tok| tok.starts_with("@object")) => {
                    // The @object, <name>, <fields> part are required, with an optional field_names
                    // which this desugarer handles, so less than 4 means no field names.
                    if other.len() < 4 {
                        self.out.push(other.join(" "));
                        continue;
                    };

                    // Parse <name> <fields>
                    let name = other[1];
                    let field_count = parse_u64(line_number, other[2])?;

                    // Remaining elements which are field names
                    let fields = other[3..].join(" ");
                    let field_names = fields
                        .trim_prefix("(")
                        .trim_suffix(")")
                        .split(",")
                        .collect::<Vec<_>>();

                    // For a named object, we need to have all fields named.
                    if (field_names.len() as u64) != field_count {
                        return Err(LoweringErrorKind::InvalidNamedFieldsAmount {
                            name: name.to_string(),
                            fields_expected: field_count,
                            fields_got: field_names.len() as u64,
                        }
                        .with_line(line_number));
                    }

                    // Create field map for this object.
                    let mut field_map = HashMap::with_capacity(field_names.len());

                    for (i, field_name) in field_names.iter().enumerate() {
                        field_map.insert(field_name.trim().to_string(), i as u64);
                    }

                    self.object_fields.insert(name.to_string(), field_map);
                    self.out.push(format!("@object {name} {field_count}"))
                }

                // Non-collectable things
                _ => {}
            }
        }

        Ok(())
    }

    /// Lowers each line in the input source file down into quark3.
    ///
    /// This relys on the fact that `collect` has already ran for collecting
    /// things into the global tables.
    fn lower_lines(&mut self) -> Result<(), LoweringError> {
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
                // @fn <name> <args> <locals> (field_name, ...)
                function if function.iter().any(|tok| tok.starts_with("@fn")) => {
                    // No matter what, encountering a function directive resets the locals table.
                    self.locals.clear();

                    // The @fn, <name>, <args>, <locals> part are required, with an optional field_names
                    // which this desugarer handles, so less than 5 means no field names.
                    if function.len() < 5 {
                        self.out.push(function.join(" "));
                        continue;
                    };

                    // Parse <name>, <args> and <locals>, since args shares locals
                    let name = function[1];
                    let args = function[2];
                    let locals_count = parse_u64(line_number, function[3])?;

                    // Remaining elements which are field names
                    let fields = function[4..].join(" ");
                    let field_names = fields
                        .trim_prefix("(")
                        .trim_suffix(")")
                        .split(",")
                        .collect::<Vec<_>>();

                    // For a function we need to have all locals named.
                    if (field_names.len() as u64) != locals_count {
                        return Err(LoweringErrorKind::InvalidNamedFieldsAmount {
                            name: name.to_string(),
                            fields_expected: locals_count,
                            fields_got: field_names.len() as u64,
                        }
                        .with_line(line_number));
                    }

                    // Create field map for this function.
                    let mut field_map = HashMap::with_capacity(field_names.len());

                    for (i, field_name) in field_names.iter().enumerate() {
                        field_map.insert(field_name.trim().to_string(), i as u64);
                    }

                    self.locals.extend(field_map);
                    self.out.push(format!("@fn {name} {args} {locals_count}"))
                }

                // Theses directives were already handled
                collected
                    if collected[0].starts_with("@capability")
                        || collected[0].starts_with("@object")
                        || collected[0].starts_with("@global") => {}

                // Instruction desugaring

                // globals map to @global defined value.
                [
                    op @ ("store.global" | "load.global" | "log" | "stg"),
                    global,
                ] => {
                    let global_number = self.globals.get(*global).ok_or_else(|| {
                        LoweringErrorKind::UndefinedGlobal {
                            global: global.to_string(),
                        }
                        .with_line(line_number)
                    })?;

                    self.push_out(format!("push.uint {global_number}"), line_number);
                    self.push_out(op.to_string(), line_number);
                }

                // locals map to @fn defined local names.
                [op @ ("load.local" | "store.local" | "lol" | "stl"), local] => {
                    let local_number = self.locals.get(*local).ok_or_else(|| {
                        LoweringErrorKind::UndefinedLocal {
                            local: local.to_string(),
                        }
                        .with_line(line_number)
                    })?;

                    self.push_out(format!("push.uint {local_number}"), line_number);
                    self.push_out(op.to_string(), line_number);
                }

                // capabilities map to @capability defined names.
                [op @ ("call.cap" | "cap"), capability] => {
                    let cap_number = self.capabilities.get(*capability).ok_or_else(|| {
                        LoweringErrorKind::UndefinedCapability {
                            capability: capability.to_string(),
                        }
                        .with_line(line_number)
                    })?;

                    self.push_out(format!("push.uint {cap_number}"), line_number);
                    self.push_out(op.to_string(), line_number);
                }

                // object set/get with object field name as ObjectType.Field
                [op @ ("object.set" | "ost" | "object.get" | "ogt"), field] => {
                    // We the field and object name (2 elements)
                    let split_access = field.split(".").collect::<Vec<_>>();

                    if split_access.len() != 2 {
                        return Err(LoweringErrorKind::InvalidObjectField {
                            got: field.to_string(),
                        }
                        .with_line(line_number));
                    }

                    let object_name = split_access[0];
                    let field = split_access[1];

                    // Get the field number from the object fields map.
                    let field_num = self
                        .object_fields
                        .get(object_name)
                        .ok_or_else(|| {
                            LoweringErrorKind::AccessObjectWithNoFieldDefs {
                                object_name: object_name.to_string(),
                                field: field.to_string(),
                            }
                            .with_line(line_number)
                        })?
                        .get(field)
                        .ok_or_else(|| {
                            LoweringErrorKind::InvalidObjectFieldAccess {
                                object_name: object_name.to_string(),
                                field: field.to_string(),
                            }
                            .with_line(line_number)
                        })?;

                    self.push_out(format!("push.uint {field_num}"), line_number);

                    // Need to desugar here to a swap because of field ordering in instruction
                    if *op == "object.set" || *op == "ost" {
                        self.push_out("swap".to_string(), line_number);
                    }

                    self.push_out(op.to_string(), line_number);
                }

                // Directives cant have @loc attached
                directive if directive.iter().any(|tok| tok.starts_with("@")) => {
                    self.out.push(directive.join(" "))
                }

                // Everything else in the file, these are just normal instructions.
                other => self.push_out(other.join(" "), line_number),
            }
        }

        Ok(())
    }

    /// Inserts a @loc directive at the current position with the source
    /// being the original boson3 file
    fn insert_loc(&mut self, line_number: usize) {
        self.out
            .push(format!("@loc {} {line_number} 0", self.filename))
    }

    fn push_out(&mut self, contents: String, line_number: usize) {
        self.insert_loc(line_number);
        self.out.push(contents);
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

/// Parse a u64 from a string token
///
/// # Errors
///
/// This will error with a `LoweringError::InvalidArgument` if the
/// string token cannot be successfully converted to a `u64`
fn parse_u64(line: usize, token: &str) -> Result<u64, LoweringError> {
    token.parse::<u64>().map_err(|_| {
        LoweringErrorKind::InvalidArgument {
            expected: "64-Bit Unsigned Integer".to_string(),
            got: token.to_string(),
        }
        .with_line(line)
    })
}
