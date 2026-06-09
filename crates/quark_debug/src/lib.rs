//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Quark3 Debug
//!
//! The `quark_debug` crate provides debugging extensions including
//! a source map of names to indices in the Lepton3 image for better
//! dissassembly output
//!
//! The structure of the source map is as follows:
//!
//! [ HEADER ]
//!   `magic`:                 [u8; 7]    // "QK3SMAP"
//!
//! [ FUNCTION TABLE ]
//!   `count`:                 u32        // total number of functions
//!   for each function:
//!     `index`:               u32        // index into the image's function table
//!     `name_length`:         u16
//!     `name`:                [u8]       // utf-8 function name
//!     `label_count`:         u32        // number of labels in this function
//!     for each label:
//!       `offset`:            u32        // byte offset from the function's instruction base
//!       `name_length`:       u16
//!       `name`:              [u8]       // utf-8 label name
//!
//! [ OBJECT TABLE ]
//!   `count`:                 u32        // total number of objects
//!   for each object:
//!     `index`:               u32        // index into the image's object table
//!     `name_length`:         u16
//!     `name`:                [u8]       // utf-8 object name

#![warn(clippy::pedantic)]
#![no_std]

extern crate alloc;

/// Extensions to the Lepton3 image for more debugging features
pub mod image;

/// A source map for better disassembly output with names
pub mod source_map;

/// Parses the source map from raw bytes into the expected format
#[cfg(feature = "parser")]
pub mod parser;

/// Serialises the rust struct representation back into an source map
#[cfg(feature = "writer")]
pub mod writer;
