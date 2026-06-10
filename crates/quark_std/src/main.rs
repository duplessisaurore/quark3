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

use clap::Parser;
use lepton3::{validator, writer};
use quark_asm::{assembler, parser};
use quark_debug::image::DebugStrippableImage;
use std::{error::Error, fs, path::PathBuf, process};

// Bump if necessary to match `Lepton3` version
const VERSION_MAJOR: u8 = 1;

#[derive(Parser)]
#[command(
    name = "quark3",
    about = "Assembles Quark3 source files into Lepton3 bytecode images"
)]
struct Cli {
    /// Input Quark3 source file
    input: PathBuf,

    /// Output Lepton3 bytecode image
    output: PathBuf,

    /// Strip debugging source locations from the Lepton3 image
    #[arg(long)]
    strip_debug: bool,

    /// Write a source map to the given file for name mapping during disassembly
    #[arg(long, value_name = "FILE")]
    source_map: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_path = &cli.input;
    let output_path = &cli.output;

    // Read source file
    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", input_path.display());
        process::exit(1);
    });

    // Parse Quark3 source
    let parsed = parser::parse(&source).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    // Assemble into Lepton3 image
    let output = assembler::assemble(parsed, VERSION_MAJOR, cli.source_map.is_some())
        .unwrap_or_else(|e| {
            eprintln!("assembly error: {e}");
            process::exit(1);
        });

    let mut image = output.image;

    // Strip debug info from the image if requested
    if cli.strip_debug {
        println!("quark3 stripping debug table in {}", output_path.display());
        image.strip_debug();
    }

    // Validate generated image
    validator::validate(&image).unwrap_or_else(|e| {
        eprintln!("validation error: {e}");
        process::exit(1);
    });

    // Write image out into bytes
    let image_bytes = writer::write(&image).unwrap_or_else(|e| {
        eprintln!("image encoder error: {e}");
        process::exit(1);
    });

    // Write output file
    fs::write(output_path, image_bytes).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", output_path.display());
        process::exit(1);
    });

    println!(
        "quark3 assembled {} -> {}",
        input_path.display(),
        output_path.display()
    );

    // Write out source map if provided
    if let (Some(map_path), Some(map)) = (&cli.source_map, output.source_map) {
        let source_map_bytes = quark_debug::writer::write(&map).unwrap_or_else(|e| {
            eprintln!("source map encoder error: {e}");
            process::exit(1);
        });

        fs::write(map_path, source_map_bytes).unwrap_or_else(|e| {
            eprintln!("error writing source map {}: {e}", map_path.display());
            process::exit(1);
        });

        println!(
            "quark3 source map {} -> {}",
            input_path.display(),
            map_path.display()
        );
    }

    Ok(())
}
