//! Assembler for Quark3 Assembly
//!
//! Takes a `ParsedFile` from the parser and assembles it into
//! a Lepton3 image.

use core::num::TryFromIntError;

use alloc::string::ToString;
use alloc::{string::String, vec::Vec};
use hashbrown::HashMap;
use lepton3::Opcode;
use lepton3::format::{DebugInfo, Function, Header, Image, ObjectType, SourceLocation};
use lepton3::lepton_image::flags::ImageFlags;
use quark_debug::source_map::{FunctionEntry, LabelEntry, ObjectEntry, SourceMap};

use crate::parser::{Instruction, ParsedFile, Statement};

/// All errors that can occur during assembly
#[derive(Debug)]
pub enum AssembleError {
    /// A label was referenced but never defined within the function
    UndefinedLabel { line: usize, label: String },

    /// A function was referenced but never defined
    UndefinedFunction { line: usize, name: String },

    /// An object was referenced but never defined
    UndefinedObject { line: usize, name: String },

    /// No entry point was declared via `@entry`
    NoEntryPoint,

    /// The declared entry point function was not found
    InvalidEntryPoint { name: String },

    /// A duplicate label was declared within the same function
    DuplicateLabel { line: usize, label: String },

    /// The instruction stream is too long for a function
    /// and cannot be assembled properly into a bytecode image
    InstructionStreamTooLong { function: String },

    /// The instruction offset/table entry is too large to be converted to an
    /// integer
    InstructionOperandTooLarge,

    /// There are too many debug files to be succesfully converted
    /// into the image
    DebugFilesTooLong,

    /// The debug source location column/line is too large and
    /// cannot be properly turned into a lepton3 debug location
    DebugSourceColumnLineTooLarge,

    /// There are too many functions and the output cannot be emitted
    TooManyFunctions,

    /// There are too many object types and the output cannot be emitted
    TooManyObjects,
}

/// Output of the assembler
pub struct AssembleOutput {
    pub image: Image,

    /// Optional source map for dissassembly
    pub source_map: Option<SourceMap>,
}

impl core::fmt::Display for AssembleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UndefinedLabel { line, label } => {
                write!(f, "line {line}: undefined label `{label}`")
            }
            Self::UndefinedFunction { line, name } => {
                write!(f, "line {line}: undefined function `{name}`")
            }
            Self::UndefinedObject { line, name } => {
                write!(f, "line {line}: undefined object `{name}`")
            }
            Self::NoEntryPoint => {
                write!(f, "no entry point declared, use `@entry <name>`")
            }
            Self::InvalidEntryPoint { name } => {
                write!(f, "entry point `{name}` is not a defined function")
            }
            Self::DuplicateLabel { line, label } => {
                write!(f, "line {line}: duplicate label `{label}`")
            }
            Self::InstructionStreamTooLong { function } => {
                write!(
                    f,
                    "function {function}: instruction stream too long, cannot write to image"
                )
            }
            Self::DebugFilesTooLong => {
                write!(f, "too many debug files, cannot write to image")
            }
            Self::DebugSourceColumnLineTooLarge => {
                write!(
                    f,
                    "too large column or line in source location, cannot write to image"
                )
            }
            Self::TooManyFunctions => {
                write!(
                    f,
                    "too many functions for function tables, cannot write to image"
                )
            }
            Self::TooManyObjects => {
                write!(
                    f,
                    "too many objects for object tables, cannot write to image"
                )
            }
            Self::InstructionOperandTooLarge => {
                write!(
                    f,
                    "instruction operand too large to be converted to integer"
                )
            }
        }
    }
}

/// Assemble a `ParsedFile` into a `Lepton3 Image`
///
/// # Errors
///
/// Returns an `AssembleError` if the assembly contains a semantic
/// error such as an undefined label, function, or object reference.
#[allow(clippy::too_many_lines)]
pub fn assemble(
    parsed: ParsedFile,
    version_major: u8,
    emit_source_map: bool,
) -> Result<AssembleOutput, AssembleError> {
    // Build a map of object names to the index
    // they will be in the image
    let object_map = parsed
        .objects
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (name.as_str(), i))
        .collect();

    // Build a map of function name to function indices
    let function_map: HashMap<&str, usize> = parsed
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    // Resolve the entry point from the parsed file
    let entry_name = parsed.entry.ok_or(AssembleError::NoEntryPoint)?;
    let entry_point = function_map
        .get(entry_name.as_str())
        .copied()
        .ok_or(AssembleError::InvalidEntryPoint { name: entry_name })?;

    // Build the object table
    let object_table = parsed
        .objects
        .iter()
        .map(|(_, fields)| ObjectType {
            field_count: *fields,
        })
        .collect();

    // Setup debug tracking
    let mut debug_files: Vec<String> = Vec::new();
    let mut file_to_idx: HashMap<String, u32> = HashMap::new();
    let mut debug_locations: Vec<SourceLocation> = Vec::new();

    // Build the flags the image will have at the end
    let mut flags = ImageFlags::from_raw(0);
    flags.set(ImageFlags::DEBUG_INFO);

    // Assemble each function into the instruction stream
    let mut function_table = Vec::new();
    let mut instruction_stream = Vec::new();

    // Source map if requested for function mapping
    let mut source_map_functions: Vec<FunctionEntry> = Vec::new();

    for func in &parsed.functions {
        let instruction_offset = u32::try_from_or_assemble_error(instruction_stream.len(), |_| {
            AssembleError::InstructionStreamTooLong {
                function: func.name.clone(),
            }
        })?;

        // We need to do a prepass for grabbing all the offsets to
        // each label in the function for the instructions
        //
        // This basically just builds a map from the label name
        // to its offset in the current function
        let mut labels: HashMap<&str, usize> = HashMap::new();
        let mut offset = 0usize;

        for statement in &func.body {
            match statement {
                // Check for duplicate labels, which don't make
                // sense so we should error
                Statement::Label(name, line) => {
                    if labels.contains_key(name.as_str()) {
                        return Err(AssembleError::DuplicateLabel {
                            line: *line,
                            label: name.clone(),
                        });
                    }
                    labels.insert(name.as_str(), offset);
                }

                // Else just get the size of the next instruction which
                // is known and add it to the offset
                Statement::Instruction(instr, _) => {
                    offset += instr.byte_size();
                }

                // A sourcelocation is a seperate directive that does not
                // contribute to size
                Statement::SourceLocation(_, _, _, _) => {}
            }
        }

        // Now we have all labels, we can handle all statements properly
        for statement in &func.body {
            match statement {
                // Emit an instruction into the stream
                Statement::Instruction(instr, line) => {
                    emit_instruction(
                        instr,
                        *line,
                        &labels,
                        &function_map,
                        &object_map,
                        &mut instruction_stream,
                    )?;
                }
                // Emit a source location into the debug info
                Statement::SourceLocation(file_path, line, col, context) => {
                    // Get the file name index if it exists, else add to the debug table
                    let file_idx = if let Some(&idx) = file_to_idx.get(file_path) {
                        idx
                    } else {
                        let idx = u32::try_from_or_assemble_error(debug_files.len(), |_| {
                            AssembleError::DebugFilesTooLong
                        })?;
                        debug_files.push(file_path.clone());
                        file_to_idx.insert(file_path.clone(), idx);
                        idx
                    };

                    // Capture the current stream position for the impending instruction
                    debug_locations.push(SourceLocation {
                        instruction_offset: u32::try_from_or_assemble_error(
                            instruction_stream.len(),
                            |_| AssembleError::InstructionStreamTooLong {
                                function: func.name.clone(),
                            },
                        )?,
                        file: file_idx,
                        line: u32::try_from_or_assemble_error(*line, |_| {
                            AssembleError::DebugSourceColumnLineTooLarge
                        })?,
                        column: u32::try_from_or_assemble_error(*col, |_| {
                            AssembleError::DebugSourceColumnLineTooLarge
                        })?,
                        context: context.clone(),
                    });
                }
                Statement::Label(_, _) => {}
            }
        }

        // And then push each function into the image's function table
        let instruction_length = u32::try_from_or_assemble_error(instruction_stream.len(), |_| {
            AssembleError::InstructionStreamTooLong {
                function: func.name.clone(),
            }
        })? - instruction_offset;

        // Emit source map for this function if requested, this allows us to
        // map the function name back into the name in the qk3 source and
        // the labels back
        if emit_source_map {
            let func_idx = function_map[func.name.as_str()];
            source_map_functions.push(FunctionEntry {
                index: u32::try_from_or_assemble_error(func_idx, |_| {
                    AssembleError::TooManyFunctions
                })?,
                name: func.name.clone(),
                labels: labels
                    .iter()
                    .map(|(&name, &offset)| {
                        Ok(LabelEntry {
                            offset: u32::try_from_or_assemble_error(offset, |_| {
                                AssembleError::InstructionStreamTooLong {
                                    function: func.name.clone(),
                                }
                            })?,
                            name: name.into(),
                        })
                    })
                    .collect::<Result<_, AssembleError>>()?,
            });
        }

        function_table.push(Function {
            arg_count: func.args,
            local_count: func.locals,
            instruction_offset,
            instruction_length,
        });
    }

    let debug_info = Some(DebugInfo {
        files: debug_files,
        locations: debug_locations,
    });

    let image = Image {
        header: Header {
            version_major,
            flags,
            entry_point: u32::try_from_or_assemble_error(entry_point, |_| {
                AssembleError::TooManyFunctions
            })?,
        },
        object_table,
        function_table,
        instructions: instruction_stream,
        debug_info,
    };

    // Collect source map entries for objects
    let objects: Vec<ObjectEntry> = if emit_source_map {
        let mut entries = object_map
            .iter()
            .map(|(&name, &index)| {
                let index =
                    u32::try_from_or_assemble_error(index, |_| AssembleError::TooManyObjects)?;

                Ok(ObjectEntry {
                    index,
                    name: name.into(),
                })
            })
            .collect::<Result<Vec<ObjectEntry>, AssembleError>>()?;

        // Sort by index so the source map order matches the object table
        entries.sort_unstable_by_key(|entry| entry.index);
        entries
    } else {
        Vec::new()
    };

    let source_map = emit_source_map.then_some(SourceMap {
        functions: source_map_functions,
        objects,
    });

    Ok(AssembleOutput { image, source_map })
}

/// Emit a single instruction into the instruction stream
#[allow(clippy::too_many_lines)]
fn emit_instruction(
    instr: &Instruction,
    line: usize,
    labels: &HashMap<&str, usize>,
    function_map: &HashMap<&str, usize>,
    object_map: &HashMap<&str, usize>,
    out: &mut Vec<u8>,
) -> Result<(), AssembleError> {
    match instr {
        // A simple instruction which is just an opcode
        Instruction::Plain(opcode) => {
            out.push(*opcode as u8);
        }

        // Pushing constants
        Instruction::PushInt(value) => {
            out.push(Opcode::PushInt as u8);
            out.extend_from_slice(&value.to_le_bytes());
        }

        Instruction::PushUInt(value) => {
            out.push(Opcode::PushUInt as u8);
            out.extend_from_slice(&value.to_le_bytes());
        }

        Instruction::PushFloat(value) => {
            out.push(Opcode::PushFloat as u8);
            out.extend_from_slice(&value.to_le_bytes());
        }

        Instruction::PushBool(value) => {
            out.push(Opcode::PushBool as u8);
            out.push(u8::from(*value));
        }

        // Multi-output instructions
        // For the labels we can now resolve them using our label map.
        Instruction::Jump(label) => {
            let offset = resolve_label(line, label, labels)?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(offset, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::Jump as u8);
        }

        Instruction::JumpIfTrue(label) => {
            let offset = resolve_label(line, label, labels)?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(offset, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::JumpIfTrue as u8);
        }

        Instruction::JumpIfFalse(label) => {
            let offset = resolve_label(line, label, labels)?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(offset, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::JumpIfFalse as u8);
        }

        Instruction::Try(label) => {
            let offset = resolve_label(line, label, labels)?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(offset, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::Try as u8);
        }

        // We resolve functions using the function map.
        Instruction::Call(name) => {
            let idx = function_map.get(name.as_str()).copied().ok_or(
                AssembleError::UndefinedFunction {
                    line,
                    name: name.clone(),
                },
            )?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(idx, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::Call as u8);
        }

        Instruction::TailCall(name) => {
            let idx = function_map.get(name.as_str()).copied().ok_or(
                AssembleError::UndefinedFunction {
                    line,
                    name: name.clone(),
                },
            )?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(idx, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::TailCall as u8);
        }

        // And objects using the object map.
        Instruction::ObjectNew(name) => {
            let idx =
                object_map
                    .get(name.as_str())
                    .copied()
                    .ok_or(AssembleError::UndefinedObject {
                        line,
                        name: name.clone(),
                    })?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(idx, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::ObjectNew as u8);
        }
        Instruction::ObjectTypeTag(name) => {
            let idx =
                object_map
                    .get(name.as_str())
                    .copied()
                    .ok_or(AssembleError::UndefinedObject {
                        line,
                        name: name.clone(),
                    })?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(idx, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
            out.push(Opcode::ObjectTypeTag as u8);
        }
        Instruction::PushFunctionIndex(name) => {
            let idx = function_map.get(name.as_str()).copied().ok_or(
                AssembleError::UndefinedFunction {
                    line,
                    name: name.clone(),
                },
            )?;
            push_uint(
                out,
                u64::try_from_or_assemble_error(idx, |_| {
                    AssembleError::InstructionOperandTooLarge
                })?,
            );
        }
    }

    Ok(())
}

/// Emit a `push.uint` instruction with the given value into the output
///
/// This is a helper used by instructions that need to push an operand
/// before emitting their opcode, such as for indices
fn push_uint(out: &mut Vec<u8>, value: u64) {
    out.push(Opcode::PushUInt as u8);
    out.extend_from_slice(&value.to_le_bytes());
}

/// Resolve a label name to its byte offset within the current function
///
/// This is just a helper to stop repeating the labels.get.blahblah in each
/// match path.
fn resolve_label(
    line: usize,
    label: &str,
    labels: &HashMap<&str, usize>,
) -> Result<usize, AssembleError> {
    labels
        .get(label)
        .copied()
        .ok_or(AssembleError::UndefinedLabel {
            line,
            label: label.to_string(),
        })
}

/// Try convert an value (Lepton3 image) to another value (offset into a stream or something)
/// with the failure case returning an `AssembleError` from a closure
trait TryFromOrAssembleError<Source: Sized>: Sized {
    type Error;

    /// This function should try convert to this type from another
    /// or return an `AssembleError` if it cannot be succesfully converted
    ///
    /// # Errors
    ///
    /// This should error with an `AssembleError` if the `other` value cannot
    /// be cast safely and successfully to the Self type
    fn try_from_or_assemble_error<T: Fn(Self::Error) -> AssembleError>(
        value: Source,
        err: T,
    ) -> Result<Self, AssembleError>;
}

impl TryFromOrAssembleError<usize> for u32 {
    type Error = TryFromIntError;

    fn try_from_or_assemble_error<T: Fn(Self::Error) -> AssembleError>(
        value: usize,
        err: T,
    ) -> Result<Self, AssembleError> {
        u32::try_from(value).map_err(err)
    }
}

impl TryFromOrAssembleError<usize> for u64 {
    type Error = TryFromIntError;

    fn try_from_or_assemble_error<T: Fn(Self::Error) -> AssembleError>(
        value: usize,
        err: T,
    ) -> Result<Self, AssembleError> {
        u64::try_from(value).map_err(err)
    }
}
