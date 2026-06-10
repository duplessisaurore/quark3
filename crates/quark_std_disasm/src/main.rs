//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Quark3 STD Disasm
//!
//! The `quark_std_disasm` crate provides a binary for disassembling `Lepton3` bytecode
//! images back into `Quark3` textual source code for systems that support the
//! rust std.

use clap::Parser;
use lepton3::format::Image;
use lepton3::parser;
use lepton3::validator;
use quark_debug::parser as source_map_parser;
use quark_disasm::disassemble;
use quark_disasm::pretty_print;
use std::{error::Error, fs, path::PathBuf, process};

#[derive(Parser)]
#[command(
    name = "quark3-dis",
    about = "Disassembles Lepton3 bytecode images into Quark3 textual source files"
)]
struct Cli {
    /// Input Lepton3 bytecode image
    input: PathBuf,

    /// Output Quark3 textual source code file
    output: PathBuf,

    /// Optional source map file for recovering function, label, and object names
    #[arg(long, value_name = "FILE")]
    source_map: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let input_path = &cli.input;
    let output_path = &cli.output;

    // Read the bytecode image
    let image_bytes = fs::read(input_path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", input_path.display());
        process::exit(1);
    });

    // Parse the image from the bytes.
    let image: Image = parser::parse(&image_bytes).unwrap_or_else(|e| {
        eprintln!("error parsing image {}: {e}", input_path.display());
        process::exit(1);
    });

    // Validate the file to ensure it's validity
    validator::validate(&image).unwrap_or_else(|e| {
        eprintln!("error validating image {}: {e}", input_path.display());
        process::exit(1);
    });

    // Load source map if provided
    let source_map = cli.source_map.as_ref().map(|map_path| {
        let source_map_bytes = fs::read(map_path).unwrap_or_else(|e| {
            eprintln!("error reading source map {}: {e}", map_path.display());
            process::exit(1);
        });

        source_map_parser::parse(&source_map_bytes).unwrap_or_else(|e| {
            eprintln!("source map parse error: {e}");
            process::exit(1);
        })
    });

    // Disassemble the image into a ParsedFile
    let parsed = disassemble(&image, source_map.as_ref()).unwrap_or_else(|e| {
        eprintln!("disassembly error: {e}");
        process::exit(1);
    });

    // Pretty print the ParsedFile into Quark3 assembly source
    let source = pretty_print(input_path.display(), &parsed);

    // Write output file
    fs::write(output_path, source).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", output_path.display());
        process::exit(1);
    });

    println!(
        "quark3 disassembled {} -> {}",
        input_path.display(),
        output_path.display()
    );

    Ok(())
}
