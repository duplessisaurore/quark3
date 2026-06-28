//! Parses a Quark3 source file into a `ParsedFile` rust struct representation
//! which can then be assembled into a Lepton3 image by the assembler.

use core::fmt::Display;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use lepton3::Opcode;

use quark_map::{long_from_opcode, opcode_from_long, opcode_from_short};

/// A parsed Quark3 source file
pub struct ParsedFile {
    /// The entry point function name
    pub entry: Option<String>,

    /// Object type declarations
    pub objects: Vec<(String, u32)>,

    /// Function declarations
    pub functions: Vec<Function>,
}

/// A parsed function before assembly
pub struct Function {
    pub name: String,
    pub args: u32,
    pub locals: u32,
    pub body: Vec<Statement>,
}

/// A statement inside a function body
pub enum Statement {
    /// A label declaration e.g. `loop:` with its source line number
    Label(String, usize),

    /// Attach extra source location information, specifically
    /// a source file, line and column.
    SourceLocation(String, usize, usize),

    /// An instruction with its source line number
    Instruction(Instruction, usize),
}

/// An instruction before label and symbol resolution
///
/// We attach some extra operand powers to some instructions
/// to improve writing ability for a programmr instead of having
/// to always push.uint before calling jump or something
pub enum Instruction {
    /// A plain no-operand instruction
    Plain(Opcode),

    /// push.int <value>
    PushInt(i64),

    /// push.uint <value>
    PushUInt(u64),

    /// push.float <value>
    PushFloat(f64),

    /// push.bool <value>
    PushBool(bool),

    /// jump <label>
    Jump(String),

    /// jump.true <label>
    JumpIfTrue(String),

    /// jump.false <label>
    JumpIfFalse(String),

    /// try <label>
    Try(String),

    /// push.uint <function name>
    /// as in the index of the function
    /// with that name
    PushFunctionIndex(String),

    /// call <function name>
    Call(String),

    /// tail.call <function name>
    TailCall(String),

    /// object.new <object name>
    ObjectNew(String),
}

impl Instruction {
    /// Returns the number of bytes this instruction emits in the final image.
    ///
    /// This is used during the label prepass to compute the offset the label
    /// is at for offset as operand instructions.
    #[must_use]
    pub fn byte_size(&self) -> usize {
        match self {
            // Emits opcode and constant
            Self::PushInt(_)
            | Self::PushUInt(_)
            | Self::PushFloat(_)
            | Self::PushFunctionIndex(_) => 9,
            Self::PushBool(_) => 2,

            // Emits a `push.uint` (9 bytes) for the operand
            // followed by the opcode (1 byte) = 10 bytes total
            Self::Jump(_)
            | Self::JumpIfTrue(_)
            | Self::JumpIfFalse(_)
            | Self::Try(_)
            | Self::Call(_)
            | Self::TailCall(_)
            | Self::ObjectNew(_) => 10,

            // Emits just the opcode, only one byte
            Self::Plain(_) => 1,
        }
    }
}

/// All errors that can occur during parsing
#[derive(Debug)]
pub enum ParseError {
    /// An unknown instruction was encountered
    UnknownInstruction { instruction: String },

    /// An instruction was encountered outside of a function
    InstructionOutsideFunction,

    /// A label was encountered outside of a function
    LabelOutsideFunction,

    /// A loc was encountered outside of a function
    LocOutsideFunction,

    /// A pushfn was encountered outside of a function
    PushFnOutsideFunction,

    /// A required argument was missing
    MissingArgument { directive: String },

    /// An argument was of the wrong type
    InvalidArgument { expected: &'static str, got: String },
}

/// A `ParseError` wrapped with the line number it
/// occured at for debug information
pub struct LinedParseError {
    line: usize,
    error: ParseError,
}

impl Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownInstruction { instruction } => {
                write!(f, "unknown instruction `{instruction}`")
            }
            Self::InstructionOutsideFunction => {
                write!(f, "instruction outside of function")
            }
            Self::LabelOutsideFunction => {
                write!(f, "label outside of function")
            }
            Self::LocOutsideFunction => {
                write!(f, "loc outside of function")
            }
            Self::MissingArgument { directive } => {
                write!(f, "missing argument for `{directive}`")
            }
            Self::InvalidArgument { expected, got } => {
                write!(f, "expected `{expected}`, got `{got}`")
            }
            Self::PushFnOutsideFunction => {
                write!(f, "push.fn outside of function")
            }
        }
    }
}

impl Display for LinedParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line `{}`: {}", self.line, self.error)
    }
}

/// Parse a full Quark3 source file into a `ParsedFile`
///
/// # Errors
///
/// Returns a `LinedParseError` if the source contains a syntax error.
pub fn parse(input: &str) -> Result<ParsedFile, LinedParseError> {
    let mut entry = None;
    let mut objects = Vec::new();
    let mut functions: Vec<Function> = Vec::new();
    let mut current_function: Option<Function> = None;

    for (line_number, line) in input.lines().enumerate() {
        let line_number = line_number + 1;

        // Strip the comment from a line and ignore if empty, this means
        // we only parse actual tokens
        let line = strip_comment(line).trim();

        if line.is_empty() {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();

        match tokens.as_slice() {
            // @entry <name>
            // Defines the entry point of the image
            ["@entry", name] => {
                entry = Some(name.to_string());
            }

            // @object <name> <fields>
            // Adds a new object type to the object table
            ["@object", name, fields] => {
                let fields = parse_u32(line_number, fields)?;
                objects.push((name.to_string(), fields));
            }

            // @fn <name> <args> <locals>
            // Adds a new function to the function table
            ["@fn", name, args, locals] => {
                if let Some(func) = current_function.take() {
                    functions.push(func);
                }
                let args = parse_u32(line_number, args)?;
                let locals = parse_u32(line_number, locals)?;
                current_function = Some(Function {
                    name: name.to_string(),
                    args,
                    locals,
                    body: Vec::new(),
                });
            }

            // @loc <file_path> <source_line> <source_col>
            // Attaches a source location at the current instruction offset
            ["@loc", file, src_line, src_col] => {
                let func = current_function
                    .as_mut()
                    .ok_or(ParseError::LocOutsideFunction.with_line(line_number))?;

                let s_line = parse_u32(line_number, src_line)?;
                let s_col = parse_u32(line_number, src_col)?;

                // Clean off any accidental wrapping quotes
                let clean_file = file.trim_matches('"').to_string();

                func.body.push(Statement::SourceLocation(
                    clean_file,
                    s_line as usize,
                    s_col as usize,
                ));
            }

            // @push.fn <function_name>
            // Attaches a push.uint at the current position that pushes
            // that function's id
            ["@push.fn", function_name] => {
                let func = current_function
                    .as_mut()
                    .ok_or(ParseError::PushFnOutsideFunction.with_line(line_number))?;

                // This is sugar for PushFunctionIndex instruction
                func.body.push(Statement::Instruction(
                    Instruction::PushFunctionIndex(function_name.to_string()),
                    line_number,
                ));
            }

            // <label>:
            // Used for assisting with offset based instructions
            [label] if label.ends_with(':') => {
                let func = current_function
                    .as_mut()
                    .ok_or(ParseError::LabelOutsideFunction.with_line(line_number))?;
                let label = label.trim_end_matches(':').to_string();
                func.body.push(Statement::Label(label, line_number));
            }

            // Actual instructions part of the function
            _ => {
                let func = current_function
                    .as_mut()
                    .ok_or(ParseError::InstructionOutsideFunction.with_line(line_number))?;
                let instr = parse_instruction(line_number, &tokens)?;
                func.body.push(Statement::Instruction(instr, line_number));
            }
        }
    }

    // Push the final function if there is one
    if let Some(func) = current_function.take() {
        functions.push(func);
    }

    Ok(ParsedFile {
        entry,
        objects,
        functions,
    })
}

/// Parse a single instruction from a token slice
fn parse_instruction(line: usize, tokens: &[&str]) -> Result<Instruction, LinedParseError> {
    let name = tokens[0];

    // Try get the opcode from either the long textual form or the
    // short textual form as declared in `quark_map`
    let opcode = opcode_from_long(&name.to_lowercase())
        .or_else(|| opcode_from_short(&name.to_lowercase()))
        .ok_or(
            ParseError::UnknownInstruction {
                instruction: name.to_string(),
            }
            .with_line(line),
        )?;

    // Handle each opcode as a directive with arguments if necessary
    match opcode {
        Opcode::PushInt => {
            // The constant value is an i64 argument, try cast
            let value = require_arg(line, long_from_opcode(&Opcode::PushInt), tokens, 1)?;
            let value = value.parse::<i64>().map_err(|_| {
                ParseError::InvalidArgument {
                    expected: "64-bit Signed Integer",
                    got: value.to_string(),
                }
                .with_line(line)
            })?;
            Ok(Instruction::PushInt(value))
        }

        Opcode::PushUInt => {
            // The constant value is an u64 argument, try cast
            let value = require_arg(line, long_from_opcode(&Opcode::PushUInt), tokens, 1)?;
            let value = value.parse::<u64>().map_err(|_| {
                ParseError::InvalidArgument {
                    expected: "64-bit Unsigned Integer",
                    got: value.to_string(),
                }
                .with_line(line)
            })?;
            Ok(Instruction::PushUInt(value))
        }

        Opcode::PushFloat => {
            // The constant value is an f64 argument, try cast
            let value = require_arg(line, long_from_opcode(&Opcode::PushFloat), tokens, 1)?;
            let value = value.parse::<f64>().map_err(|_| {
                ParseError::InvalidArgument {
                    expected: "64-bit IEEE 754 floating point number",
                    got: value.to_string(),
                }
                .with_line(line)
            })?;
            Ok(Instruction::PushFloat(value))
        }

        Opcode::PushBool => {
            // Try read the argument as a boolean "true"/"false" or a "1"/"0"
            let value = require_arg(line, long_from_opcode(&Opcode::PushBool), tokens, 1)?;
            let value = match value.to_lowercase().as_str() {
                "true" | "1" => true,
                "false" | "0" => false,
                _ => {
                    return Err(ParseError::InvalidArgument {
                        expected: "true or false",
                        got: value.to_string(),
                    }
                    .with_line(line));
                }
            };
            Ok(Instruction::PushBool(value))
        }

        // These opcodes use a label as an offset and are really
        // "cumulative" opcodes which emit a pushint to the label offset
        // before emitting the instruction
        //
        // We read the label as an the argument
        Opcode::Jump => {
            let label = require_arg(line, long_from_opcode(&Opcode::Jump), tokens, 1)?;
            Ok(Instruction::Jump(label.to_string()))
        }

        Opcode::JumpIfTrue => {
            let label = require_arg(line, long_from_opcode(&Opcode::JumpIfTrue), tokens, 1)?;
            Ok(Instruction::JumpIfTrue(label.to_string()))
        }

        Opcode::JumpIfFalse => {
            let label = require_arg(line, long_from_opcode(&Opcode::JumpIfFalse), tokens, 1)?;
            Ok(Instruction::JumpIfFalse(label.to_string()))
        }

        Opcode::Try => {
            let label = require_arg(line, long_from_opcode(&Opcode::Try), tokens, 1)?;
            Ok(Instruction::Try(label.to_string()))
        }

        Opcode::Call => {
            let name = require_arg(line, long_from_opcode(&Opcode::Call), tokens, 1)?;
            Ok(Instruction::Call(name.to_string()))
        }

        Opcode::TailCall => {
            let name = require_arg(line, long_from_opcode(&Opcode::TailCall), tokens, 1)?;
            Ok(Instruction::TailCall(name.to_string()))
        }

        // This uses the object name as an argument
        Opcode::ObjectNew => {
            let name = require_arg(line, long_from_opcode(&Opcode::ObjectNew), tokens, 1)?;
            Ok(Instruction::ObjectNew(name.to_string()))
        }
        _ => Ok(Instruction::Plain(opcode)),
    }
}

/// Require a token at a given index or return a `MissingArgument` error
///
/// This should be used by directives/instructions to ensure an argument
/// exists
fn require_arg<'a>(
    line: usize,
    directive: &str,
    tokens: &[&'a str],
    index: usize,
) -> Result<&'a str, LinedParseError> {
    tokens.get(index).copied().ok_or(
        ParseError::MissingArgument {
            directive: directive.to_string(),
        }
        .with_line(line),
    )
}

/// Parse a u32 from a string token
///
/// # Errors
///
/// This will error with a `ParseError::InvalidArgument` if the
/// string token cannot be successfully converted to a `u32`
fn parse_u32(line: usize, token: &str) -> Result<u32, LinedParseError> {
    token.parse::<u32>().map_err(|_| {
        ParseError::InvalidArgument {
            expected: "32-Bit Unsigned Integer",
            got: token.to_string(),
        }
        .with_line(line)
    })
}

/// Strip a line comment from a line
fn strip_comment(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
}

impl ParseError {
    fn with_line(self, line_number: usize) -> LinedParseError {
        LinedParseError {
            line: line_number,
            error: self,
        }
    }
}
