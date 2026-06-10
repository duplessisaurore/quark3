//! Pretty prints a Quark3 `ParsedFile` into a String which
//! can then be written to a file as output

use alloc::format;
use alloc::string::{String, ToString};
use core::fmt::{Display, Write};
use quark_asm::parser::{Instruction, ParsedFile, Statement};
use quark_map::long_from_opcode;

#[must_use]
pub fn pretty_print(source: impl Display, file: &ParsedFile) -> String {
    let mut out = String::new();

    // source file header content
    let _ = write!(out, "// Disassembled from {source}\n");

    // @entry
    if let Some(entry) = &file.entry {
        let _ = write!(out, "@entry {entry}\n\n");
    }

    // @object declarations
    for (name, fields) in &file.objects {
        let _ = writeln!(out, "@object {name} {fields}");
    }

    if !file.objects.is_empty() {
        out.push('\n');
    }

    // Functions
    for func in &file.functions {
        let _ = writeln!(out, "@fn {} {} {}", func.name, func.args, func.locals);

        for statement in &func.body {
            match statement {
                Statement::Label(name, _) => {
                    let _ = writeln!(out, "{name}:");
                }

                Statement::SourceLocation(file, line, col) => {
                    let _ = writeln!(out, "    @loc {file} {line} {col}");
                }

                Statement::Instruction(instr, _) => {
                    let text = match instr {
                        Instruction::Plain(opcode) => long_from_opcode(opcode).to_string(),
                        Instruction::PushInt(v) => format!("push.int {v}"),
                        Instruction::PushBool(v) => format!("push.bool {v}"),
                        #[cfg(feature = "floats")]
                        Instruction::PushFloat(v) => format!("push.float {v}"),
                        Instruction::Jump(label) => format!("jump {label}"),
                        Instruction::JumpIfTrue(label) => format!("jump.true {label}"),
                        Instruction::JumpIfFalse(label) => format!("jump.false {label}"),
                        Instruction::Try(label) => format!("try {label}"),
                        Instruction::Call(name) => format!("call {name}"),
                        Instruction::TailCall(name) => format!("tail.call {name}"),
                        Instruction::ObjectNew(name) => format!("object.new {name}"),
                    };
                    let _ = writeln!(out, "    {text}");
                }
            }
        }

        out.push('\n');
    }

    out
}
