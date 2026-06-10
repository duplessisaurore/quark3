//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Quark3 Dissassembler
//!
//! The `quark_disasm` crate provides the dissassembler for the `Lepton3` bytecode
//! to convert it back into the textual `Quark3` language optionally with source
//! maps for improved readability.

#![warn(clippy::pedantic)]
#![no_std]

extern crate alloc;

/// The dissassembler itself which takes in a `Lepton3` image and produces
/// a Quark3 parsed output
pub mod dissassembler;

/// The pretty printer that can take a Quark3 parsed output and produce the
/// Quark3 textual code from it
pub mod pretty_printer;

// Re-export the actual parts that are important
pub use dissassembler::DisassembleError;
pub use dissassembler::disassemble;

pub use pretty_printer::pretty_print;
