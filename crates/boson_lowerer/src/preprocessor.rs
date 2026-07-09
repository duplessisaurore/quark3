//! The actual preprocessor itself,
//! this runs all the preprocessing functions and lowers
//! elements down to `Quark3`.

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

    // The current function who we are tracking for @locals
    function: Option<Boson3Function>,

    // The input source file that we are desugaring
    source: &'source str,

    // The current outputted lines of `Quark3`
    out: Vec<String>,

    // The last allocated global slot
    global_slot: u64,
}

/// A `Boson3` function whose locals count is computed by the lowerer.
///
/// This is created for all @fn declarations which must use the special
/// `Boson3` syntax.
#[derive(Debug)]
struct Boson3Function {
    /// The index of the emitted `@fn` line in the lowerer output.
    line: usize,

    /// The name of the function.
    name: String,

    /// How many arguments the function takes.
    args: u64,

    /// The locals named so far in this function, of name to slot.
    locals: HashMap<String, u64>,

    /// The next local slot to be allocated.
    next_slot: u64,
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
            function: None,
            source,
            out: Vec::new(),
            global_slot: 0,
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
                // @global <name>
                // Defines a new global slot for usage with load and store global
                // This allocates the slot.
                ["@global", name] => {
                    let name = name.to_string();
                    let slot = self.global_slot;
                    self.global_slot += 1;
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
                // @fn <name> <args> (arg_names, ...)
                ["@fn", name, arg_count, args  @ ..] => {
                    // Parse <args>
                    let args_count = parse_u64(line_number, *arg_count)?;

                    // Remaining elements which are arg names
                    let args = args.join(" ");
                    
                    // Must start with "("
                    if !args.starts_with("(") {
                        return Err(LoweringErrorKind::InvalidArgument {
                            expected: "@fn <name> <args> (<arg_name>, <arg_name>, ...)".to_string(),
                            got: tokens.join(" "),
                        }
                        .with_line(line_number));
                    }

                    let mut arg_names = args
                        .trim_prefix("(")
                        .trim_suffix(")")
                        .split(",")
                        .collect::<Vec<_>>();

                    // Reset arg names if we have no actual arguments.
                    if (args_count == 0) && arg_names.len() == 1 && arg_names[0] == "" {
                        arg_names.clear();
                    }

                    // For a function we need to have all args named.
                    if (arg_names.len() as u64) != args_count {
                        return Err(LoweringErrorKind::InvalidNamedArgsFunctionAmount {
                            name: name.to_string(),
                            args_expected: args_count,
                            args_got: arg_names.len() as u64,
                        }
                        .with_line(line_number));
                    }

                    // Create locals map for this function starting with args.
                    let mut locals_map = HashMap::with_capacity(arg_names.len());

                    for (i, arg_name) in arg_names.iter().enumerate() {
                        locals_map.insert(arg_name.trim().to_string(), i as u64);
                    }

                    // The new tracked function
                    let tracked_function =
                        Boson3Function::new(self.out.len(), name, args_count, locals_map);

                    self.out.push(tracked_function.fn_line());
                    self.function = Some(tracked_function);
                }

                // These @fn directives do not obey the requirements so they're invalid
                collected if collected[0].starts_with("@fn") => {
                    return Err(LoweringErrorKind::InvalidArgument {
                        expected: "@fn <name> <args> (<arg_name>, <arg_name>, ...)".to_string(),
                        got: tokens.join(" "),
                    }
                    .with_line(line_number));
                }

                // @local <name>
                // Allocates the next local slot in the current function under
                // the <name> similar to @global
                ["@local", name] => {
                    let Some(function) = self.function.as_mut() else {
                        return Err(LoweringErrorKind::LocalOutsideFunction {
                            local_name: name.to_string(),
                        }
                        .with_line(line_number));
                    };

                    // Add to the locals map for the current function
                    function.locals.insert(name.to_string(), function.next_slot);
                    function.next_slot += 1;

                    // The local count has grown so we need to update the @fn line
                    self.out[function.line] = function.fn_line();
                }

                // These @local directives are invalid..
                ["@local", ..]=> {
                    return Err(LoweringErrorKind::InvalidArgument {
                        expected: "@local <name>".to_string(),
                        got: tokens.join(" "),
                    }
                    .with_line(line_number));
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

                    self.out.push(format!("push.uint {global_number}"));
                    self.out.push(op.to_string());
                }

                // locals map to @fn defined local names.
                [op @ ("load.local" | "store.local" | "lol" | "stl"), local] => {
                    let local_number = self
                        .function
                        .as_ref()
                        .and_then(|counted| counted.locals.get(*local))
                        .ok_or_else(|| {
                            LoweringErrorKind::UndefinedLocal {
                                local: local.to_string(),
                            }
                            .with_line(line_number)
                        })?;

                    self.out.push(format!("push.uint {local_number}"));
                    self.out.push(op.to_string());
                }

                // capabilities map to @capability defined names.
                [op @ ("call.cap" | "cap"), capability] => {
                    let cap_number = self.capabilities.get(*capability).ok_or_else(|| {
                        LoweringErrorKind::UndefinedCapability {
                            capability: capability.to_string(),
                        }
                        .with_line(line_number)
                    })?;

                    self.out.push(format!("push.uint {cap_number}"));
                    self.out.push(op.to_string());
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

                    self.out.push(format!("push.uint {field_num}"));

                    // Need to desugar here to a swap because of field ordering in instruction
                    if *op == "object.set" || *op == "ost" {
                        self.out.push("swap".to_string());
                    }

                    self.out.push(op.to_string());
                }

                // Everything else in the file, these are just normal instructions.
                other => self.out.push(other.join(" ")),
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

impl Boson3Function {
    /// Creates a new Boson3Function for function locals tracking
    fn new(line: usize, name: &str, args: u64, initial_locals: HashMap<String, u64>) -> Self {
        Self {
            line,
            name: name.to_string(),
            args,
            locals: initial_locals,

            // The next slot for locals starts after the arguments.
            next_slot: args,
        }
    }

    /// The current locals count of this function.
    fn locals_count(&self) -> u64 {
        self.args.max(self.next_slot)
    }

    /// The complete `@fn` line with the current locals count
    fn fn_line(&self) -> String {
        format!("@fn {} {} {}", self.name, self.args, self.locals_count())
    }
}
