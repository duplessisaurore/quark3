//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! The entry point of the assembler is through `quark_asm` crate, which can be used to assemble
//! a `Quark3` file into a `Lepton3` image for execution by the `Lepton3` virtual machine.
//!
//! ## This Crate
//!
//! The `quark` crate is a meta crate that rexports all of the sub-components of quark into one
//! simpler interface.

#![warn(clippy::pedantic)]
#![no_std]

// Rexport all bits of the quark crate
// from the assembler side that would be used frequently
pub use quark_asm::assembler::AssembleError;
pub use quark_asm::assembler::assemble;
pub use quark_asm::parser::LinedParseError;
pub use quark_asm::parser::ParseError;
pub use quark_asm::parser::parse;

// Rexport the internal crates for usage
// if necessary
pub use quark_asm;
pub use quark_map;
