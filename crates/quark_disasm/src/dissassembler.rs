//! Dissassembler for Quark3 Assembly
//!
//! Takes an `Image` from `Lepton3` and disassembles it into
//! a Quark3 `ParsedFile`.

use core::{
    array::TryFromSliceError, num::TryFromIntError, ops::{Deref, DerefMut}
};

use alloc::{format, string::String, vec::Vec};
use hashbrown::{HashMap, HashSet};
use lepton3::{Opcode, format::Image};
use quark_asm::parser::{Function, Instruction, ParsedFile, Statement};
use quark_debug::source_map::SourceMap;

/// All errors that can occur during disassembly
#[derive(Debug)]
pub enum DisassembleError {
    /// The instruction stream was truncated unexpectedly
    UnexpectedEnd { offset: usize },

    /// An unknown opcode was encountered
    UnknownOpcode { offset: usize, byte: u8 },

    /// An offset value was invalid
    InvalidOffset { error: TryFromIntError },

    /// The value trying to be read from the stream
    /// at the cursor is invalid
    InvalidConversion { error: TryFromSliceError }
}

pub struct FunctionedDisassembleError {
    function: usize,
    inner_error: DisassembleError,
}

impl core::fmt::Display for FunctionedDisassembleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let function = self.function;

        match self.inner_error {
            DisassembleError::UnexpectedEnd { offset } => {
                write!(
                    f,
                    "function {function}: unexpected end of stream at offset {offset}"
                )
            }
            DisassembleError::UnknownOpcode { offset, byte } => {
                write!(
                    f,
                    "function {function}: unknown opcode {byte:#04x} at offset {offset}"
                )
            }
            DisassembleError::InvalidOffset { error } => {
                write!(
                    f,
                    "function {function}: offset value could not be properly converted to offset due to error: `{error}`"
                )
            }
            DisassembleError::InvalidConversion { error } => {
                write!(
                    f,
                    "function {function}: offset value could not be properly converted to value due to error: `{error}`"
                )    
            },
        }
    }
}

/// New types over hashmaps to ensure we pass correct map always to the correct
/// source map lookup functions
pub struct FunctionMap<'name>(pub HashMap<usize, &'name str>);
pub struct ObjectMap<'name>(pub HashMap<usize, &'name str>);
pub struct LabelMap<'name>(pub HashMap<usize, &'name str>);

/// Returns either the default function name for this index
/// or the actual function name from the source map provided
#[must_use]
pub fn lookup_function_name(index: &usize, source_map: &FunctionMap) -> String {
    source_map
        .get(index)
        .map_or_else(|| format!("fn_{index}"), alloc::string::ToString::to_string)
}

/// Returns either the default object name for this index
/// or the actual object name from the source map provided
#[must_use]
pub fn lookup_object_name(index: &usize, source_map: &ObjectMap) -> String {
    source_map.get(index).map_or_else(
        || format!("obj_{index}"),
        alloc::string::ToString::to_string,
    )
}

/// Returns either the default label name for this index
/// or the actual label name from the source map provided
#[must_use]
pub fn lookup_label_name(index: &usize, source_map: &LabelMap) -> String {
    source_map.get(index).map_or_else(
        || format!("label_{index}"),
        alloc::string::ToString::to_string,
    )
}

/// Disassembles a `Lepton3` bytecode image
/// back into a Lepton3 `ParsedFile` potentially
/// with a source map for nicer names of labels, objects and functions.
///
/// # Errors
///
/// If any error occurs during dissassembly (see: `DisassembleError`) then
/// this function will exit and return the error.
pub fn disassemble(
    image: &Image,
    source_map: Option<&SourceMap>,
) -> Result<ParsedFile, FunctionedDisassembleError> {
    // Build lookup maps from the source map if present
    let fn_names: FunctionMap = source_map
        .map(|source_map: &SourceMap| {
            source_map
                .functions
                .iter()
                .map(|fn_entry| (fn_entry.index as usize, fn_entry.name.as_str()))
                .collect::<HashMap<usize, &str>>()
        })
        .unwrap_or_default()
        .into();

    let obj_names: ObjectMap = source_map
        .map(|source_map| {
            source_map
                .objects
                .iter()
                .map(|obj_entry| (obj_entry.index as usize, obj_entry.name.as_str()))
                .collect::<HashMap<usize, &str>>()
        })
        .unwrap_or_default()
        .into();

    // Grab the entry point function name for @entry
    let entry = lookup_function_name(&(image.header.entry_point as usize), &fn_names);

    // Build the ParsedFile object type declarations
    let objects: Vec<(String, u32)> = image
        .object_table
        .iter()
        .enumerate()
        .map(|(index, obj)| (lookup_object_name(&index, &obj_names), obj.field_count))
        .collect();

    // Build the functions declarations table with the bodies
    let mut functions = Vec::new();

    for (func_idx, func) in image.function_table.iter().enumerate() {
        let stream_start = func.instruction_offset as usize;
        let stream_end = stream_start + func.instruction_length as usize;

        // Extract the set of instructions that fall into this function's instruction offset
        // and length
        let stream = image
            .instructions
            .get(stream_start..stream_end)
            .ok_or(DisassembleError::UnexpectedEnd { offset: 0 })
            .map_err(|err| err.with_function(func_idx))?;

        // Look up label names from the source map for this function
        let label_names: LabelMap = source_map
            .and_then(|source_map| {
                source_map
                    .functions
                    .iter()
                    .find(|fn_entry| (fn_entry.index as usize) == func_idx)
            })
            .map(|fn_entry| {
                fn_entry
                    .labels
                    .iter()
                    .map(|label_entry| (label_entry.offset as usize, label_entry.name.as_str()))
                    .collect::<HashMap<usize, &str>>()
            })
            .unwrap_or_default()
            .into();

        // Collect all label target offsets so we know
        // where to insert labels during the function body creation
        //
        // This is essentially Pass 1
        let label_targets =
            collect_label_targets(stream).map_err(|err| err.with_function(func_idx))?;

        // Decode the instruction stream into statements
        // along with other directives
        let body = decode_statements(
            func_idx,
            stream,
            &label_targets,
            &label_names,
            &fn_names,
            &obj_names,
            image,
        )
        .map_err(|err| err.with_function(func_idx))?;

        // Lookup the function name and push into our file stream
        let name = lookup_function_name(&func_idx, &fn_names);

        functions.push(Function {
            name,
            args: func.arg_count,
            locals: func.local_count,
            body,
        });
    }

    Ok(ParsedFile {
        entry: Some(entry),
        objects,
        functions,
    })
}

/// Walk the instruction stream and collect every offset that is
/// the target of a jump or try instruction which is where labels
/// will be
fn collect_label_targets(stream: &[u8]) -> Result<HashSet<usize>, DisassembleError> {
    // Put targets into a hashset since jump/try can jump to the same label multiple times
    let mut targets = HashSet::new();
    let mut cursor = 0;

    while cursor < stream.len() {
        let opcode =
            Opcode::try_from(stream[cursor]).map_err(|b| DisassembleError::UnknownOpcode {
                offset: cursor,
                byte: b,
            })?;

        // We have the full size of the instruction here
        let operand_size = opcode.operand_size() as usize;
        cursor += 1;

        if opcode == Opcode::PushUInt && cursor + 8 < stream.len() {
            // Read the offset we are jumping to
            let value = read_u64(stream, cursor)?;

            // Try read the next opcode as one that "jumps" to a label
            // so this would be a "multi-instruction" instruction
            let next_opcode = Opcode::try_from(stream[cursor + 8]).ok();
            if let Some(next) = next_opcode
                && is_label_opcode(next)
            {
                targets.insert(usize::try_from_or_disassemble_error(value)?);
            }
        }

        cursor += operand_size;
    }

    Ok(targets)
}

/// Decode the instruction stream into statements
/// along with other directives
///
/// This should properly output @loc, labels and all
/// components such that the assembler can reassemble
/// the dissassembled file and produce the same bytecode
#[allow(clippy::too_many_lines)]
fn decode_statements(
    func_idx: usize,
    stream: &[u8],
    label_targets: &HashSet<usize>,
    label_names: &LabelMap,
    fn_names: &FunctionMap,
    obj_names: &ObjectMap,
    image: &Image,
) -> Result<Vec<Statement>, DisassembleError> {
    // This function body will need to disassemble all of the statements out
    let mut statements = Vec::new();
    let mut cursor = 0;

    // Emit debug locations indexed by instruction stream offset
    let debug_locations: HashMap<usize, _> = image
        .debug_info
        .as_ref()
        .map(|d| {
            d.locations
                .iter()
                .map(|loc| (loc.instruction_offset as usize, loc))
                .collect()
        })
        .unwrap_or_default();

    // All of the debug files in the image used by our debug locations
    let debug_files = image
        .debug_info
        .as_ref()
        .map_or(&[][..], |d| d.files.as_slice());

    let stream_base = image.function_table[func_idx].instruction_offset;

    while cursor < stream.len() {
        // Insert a label here if this offset is a jump target
        if label_targets.contains(&cursor) {
            let label_name = lookup_label_name(&cursor, label_names);

            // We cant really recover the exact line number from the source
            // so just leave it as zero
            statements.push(Statement::Label(label_name, 0));
        }

        // Emit a source location if debug info exists for this offset
        let abs_offset = stream_base as usize + cursor;
        if let Some(loc) = debug_locations.get(&abs_offset)
            && let Some(file) = debug_files.get(loc.file as usize)
        {
            statements.push(Statement::SourceLocation(
                file.clone(),
                loc.line as usize,
                loc.column as usize,
            ));
        }

        // Try read the opcode now at the current location in the instruction stream
        let opcode =
            Opcode::try_from(stream[cursor]).map_err(|b| DisassembleError::UnknownOpcode {
                offset: cursor,
                byte: b,
            })?;

        cursor += 1;

        match opcode {
            Opcode::PushUInt => {
                let value = read_u64(stream, cursor)?;
                cursor += 8;

                // Peek at the next opcode to see if this is a multi-instruction
                // sugared form
                if let Some(&next_byte) = stream.get(cursor)
                    && let Ok(next_opcode) = Opcode::try_from(next_byte)
                {
                    match next_opcode {
                        // Re-output the sugared instructions into the function body
                        Opcode::Jump => {
                            cursor += 1;
                            let label = lookup_label_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                label_names,
                            );
                            statements.push(Statement::Instruction(Instruction::Jump(label), 0));
                            continue;
                        }
                        Opcode::JumpIfTrue => {
                            cursor += 1;
                            let label = lookup_label_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                label_names,
                            );
                            statements
                                .push(Statement::Instruction(Instruction::JumpIfTrue(label), 0));
                            continue;
                        }
                        Opcode::JumpIfFalse => {
                            cursor += 1;
                            let label = lookup_label_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                label_names,
                            );
                            statements
                                .push(Statement::Instruction(Instruction::JumpIfFalse(label), 0));
                            continue;
                        }
                        Opcode::Try => {
                            cursor += 1;
                            let label = lookup_label_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                label_names,
                            );
                            statements.push(Statement::Instruction(Instruction::Try(label), 0));
                            continue;
                        }
                        Opcode::Call => {
                            cursor += 1;
                            let name = lookup_function_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                fn_names,
                            );
                            statements.push(Statement::Instruction(Instruction::Call(name), 0));
                            continue;
                        }
                        Opcode::TailCall => {
                            cursor += 1;
                            let name = lookup_function_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                fn_names,
                            );
                            statements.push(Statement::Instruction(Instruction::TailCall(name), 0));
                            continue;
                        }
                        Opcode::ObjectNew => {
                            cursor += 1;
                            let name = lookup_object_name(
                                &usize::try_from_or_disassemble_error(value)?,
                                obj_names,
                            );
                            statements
                                .push(Statement::Instruction(Instruction::ObjectNew(name), 0));
                            continue;
                        }
                        _ => {}
                    }
                }

                // Plain PushUInt
                statements.push(Statement::Instruction(Instruction::PushUInt(value), 0));
            }

            // Reoutput the constant emitting instructions
            Opcode::PushInt => {
                let value = read_i64(stream, cursor)?;
                cursor += 8;
                statements.push(Statement::Instruction(Instruction::PushInt(value), 0));
            }

            Opcode::PushBool => {
                let value = stream
                    .get(cursor)
                    .copied()
                    .ok_or(DisassembleError::UnexpectedEnd { offset: cursor })?;
                cursor += 1;
                statements.push(Statement::Instruction(Instruction::PushBool(value != 0), 0));
            }

            #[cfg(feature = "floats")]
            Opcode::PushFloat => {
                let value = read_f64(stream, cursor)?;
                cursor += 8;
                statements.push(Statement::Instruction(Instruction::PushFloat(value), 0));
            }

            // Everything else is a plain no-operand instruction
            opcode => {
                statements.push(Statement::Instruction(Instruction::Plain(opcode), 0));
            }
        }
    }

    Ok(statements)
}

/// These are all the opcode variants that jump to a label
fn is_label_opcode(opcode: Opcode) -> bool {
    matches!(
        opcode,
        Opcode::Jump | Opcode::JumpIfTrue | Opcode::JumpIfFalse | Opcode::Try
    )
}

/// Reads an i64 (lepton3 int) from the instruction stream
fn read_i64(stream: &[u8], cursor: usize) -> Result<i64, DisassembleError> {
    let bytes: [u8; 8] = stream[cursor..cursor + 8].try_into().map_err(|err| DisassembleError::InvalidConversion { error: err })?;
    Ok(i64::from_le_bytes(bytes))
}

/// Reads an u64 (lepton3 uint) from the instruction stream
fn read_u64(stream: &[u8], cursor: usize) -> Result<u64, DisassembleError> {
    let bytes: [u8; 8] = stream[cursor..cursor + 8].try_into().map_err(|err| DisassembleError::InvalidConversion { error: err })?;
    Ok(u64::from_le_bytes(bytes))
}


/// Reads an f64 (lepton3 float) from the instruction stream
#[cfg(feature = "floats")]
fn read_f64(stream: &[u8], cursor: usize) -> Result<f64, DisassembleError> {
    let bytes: [u8; 8] = stream[cursor..cursor + 8].try_into().map_err(|err| DisassembleError::InvalidConversion { error: err })?;
    Ok(f64::from_le_bytes(bytes))
}

impl<'name> From<HashMap<usize, &'name str>> for FunctionMap<'name> {
    fn from(value: HashMap<usize, &'name str>) -> Self {
        Self(value)
    }
}

impl<'name> From<HashMap<usize, &'name str>> for ObjectMap<'name> {
    fn from(value: HashMap<usize, &'name str>) -> Self {
        Self(value)
    }
}

impl<'name> From<HashMap<usize, &'name str>> for LabelMap<'name> {
    fn from(value: HashMap<usize, &'name str>) -> Self {
        Self(value)
    }
}

impl<'name> Deref for FunctionMap<'name> {
    type Target = HashMap<usize, &'name str>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FunctionMap<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'name> Deref for ObjectMap<'name> {
    type Target = HashMap<usize, &'name str>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ObjectMap<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'name> Deref for LabelMap<'name> {
    type Target = HashMap<usize, &'name str>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for LabelMap<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Try convert an `u64` (Lepton3 image) to another value (offset into a stream or something)
/// with the failure case returning a `DisassembleError`
trait TryU64ToSelfWithDisassembleError: Sized {
    /// This function should try convert to this type from an `u64`
    /// or return a `DisassembleError` if it cannot be succesfully converted
    ///
    /// # Errors
    ///
    /// This should error with a `DisassembleError` if the `u64` value cannot
    /// be cast safely and successfully to the Self type
    fn try_from_or_disassemble_error(other: u64) -> Result<Self, DisassembleError>;
}

impl TryU64ToSelfWithDisassembleError for usize {
    fn try_from_or_disassemble_error(other: u64) -> Result<Self, DisassembleError> {
        usize::try_from(other).map_err(|err| DisassembleError::InvalidOffset { error: err })
    }
}

impl DisassembleError {
    #[must_use]
    pub fn with_function(self, func_idx: usize) -> FunctionedDisassembleError {
        FunctionedDisassembleError {
            function: func_idx,
            inner_error: self,
        }
    }
}
