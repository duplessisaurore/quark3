//! `Quark3` is an experimental free and open-source textual assembly language 
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Quark3 Assembler
//!
//! The `quark_asm` crate provides the parser and assembler for the `Quark3`
//! assembly language for `Lepton3`. This assembler supports `no_std`
//! environments but must recieve an allocated string to parse/assemble into
//! `Lepton3` bytecode.

#![warn(clippy::pedantic)]
#![no_std]

extern crate alloc;

/// The parser portion of the assembler which takes the source
/// code and turns it into a `ParsedFile` that can be assembled
pub mod parser;

/// The assemebler portion, which takes a `ParsedFile` and outputs
/// the `Lepton3` image.
pub mod assembler;