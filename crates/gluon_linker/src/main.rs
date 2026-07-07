//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Gluon3
//!
//! The `Gluon3` crate is a crate that links together
//! multiple `Boson3` files together into one, with namespace
//! resolution as described in the `README.md`

use std::error::Error;

use clap::Parser;
use std::{fs, path::PathBuf, process};

#[derive(Parser)]
#[command(
    name = "gluon3",
    about = "Links multiple Boson3 source files together into one Boson3 file"
)]
struct Cli {
    /// Input Boson3 source files
    input: Vec<PathBuf>,

    /// Output Quark3 source file
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_paths = &cli.input;
    let output_path = &cli.output;

    // Read source files
    let source_files = input_paths.iter().map(|input_path| {
        fs::read_to_string(input_path).unwrap_or_else(|e| {
            eprintln!("error reading {}: {e}", input_path.display());
            process::exit(1);
        })
    }).collect::<Vec<_>>();

    Ok(())
}
