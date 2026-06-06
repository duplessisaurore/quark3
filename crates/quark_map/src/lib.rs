//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Quark3 Map
//!
//! The `quark_map` crate provides mappings between
//! two different forms of textual bytecode representations to
//! the actual opcodes for the instruction.
//!
//! These two forms are the `short` form and the `long` form which are both
//! case-insensitive.
//!
//! The `short` form expresses each instruction in `3` characters. The long
//! form is a fully descriptive version that describes the entire instruction
//! as words seperated by `.` characters, this is a slightly more verbose version
//! of the original instruction set specified by the `Lepton3` virtual machine.

#![warn(clippy::pedantic)]
#![no_std]

///
/// All of the mappings of quark3 correspond
/// to some opcode in Lepton3.
///
/// Generally a mapping will be a bidirectional mapping
/// from some string <-> opcode
///
/// This `map_opcode` macro outputs the mapping as two functions,
/// one which converts the opcode to the string form, and the string
/// form to the opcode.
///
/// The string form to the opcode may fail in the match, and therefore
/// it returns an Option<>
///
/// Each entry should be as follows:
///
/// "string_name" = <Opcode>,
///
macro_rules! map_opcode {
    ($($name:literal = $opcode:path),* $(,)?) => {
        pub fn opcode_from_str(s: &str) -> Option<lepton3::Opcode> {
            match s {
                $($name => Some($opcode),)*
                _ => None,
            }
        }

        pub fn str_from_opcode(op: &lepton3::Opcode) -> &'static str {
            match op {
                $($opcode => $name,)*
            }
        }
    };
}

/// Short textual mapping
mod short;
pub use short::opcode_from_str as opcode_from_short;
pub use short::str_from_opcode as short_from_opcode;

/// Long textual mapping
mod long;
pub use long::opcode_from_str as opcode_from_long;
pub use long::str_from_opcode as long_from_opcode;
