//! `Quark3` is an experimental free and open-source textual assembly language 
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Quark3 STD
//!
//! The `quark_std` crate provides a binary for assembling `Quark3` assembly
//! language files into `Lepton3` bytecode images for systems that support the
//! rust std.

use std::{error::Error, fs, process};

use lepton3::{
    writer,
    validator,
};

use quark_asm::{
    assembler,
    parser,
};

// Bump if necessary to match `Lepton3` version
const VERSION_MAJOR: u8 = 1;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Ensure we have enough arguments
    if args.len() != 3 {
        eprintln!("usage: quark_std <program.qk3> <output.lp3>");
        process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    // Read source file
    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("error reading {input_path}: {e}");
        process::exit(1);
    });

    // Parse Quark3 source
    let parsed = parser::parse(&source).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    // Assemble into Lepton3 image
    let image = assembler::assemble(parsed, VERSION_MAJOR).unwrap_or_else(|e| {
        eprintln!("assembly error: {e}");
        process::exit(1);
    });

    // Validate generated image
    validator::validate(&image).unwrap_or_else(|e| {
        eprintln!("validation error: {e}");
        process::exit(1);
    });

    // Write image out into bytes
    let bytes = writer::write(&image).unwrap_or_else(|e| {
        eprintln!("writer error: {e}");
        process::exit(1);
    });

    // Write output file
    fs::write(output_path, bytes).unwrap_or_else(|e| {
        eprintln!("error writing {output_path}: {e}");
        process::exit(1);
    });

    println!("quark3 assembled {input_path} -> {output_path}");
    Ok(())
}