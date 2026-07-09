//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Boson3
//!
//! The `Boson3` crate is a crate that desugars some extra `Boson3` syntax
//! ontop of `Quark3`, view the `README.md` in the repository.
//! 
//! `Boson3` also adds macros which are done by the `macroprocessor`

#![feature(trim_prefix_suffix)]

use std::error::Error;

pub mod errors;
pub mod preprocessor;
pub mod macroprocessor;

use clap::Parser;
use std::{fs, path::PathBuf, process};

use crate::preprocessor::BosonLowerer;

#[derive(Parser)]
#[command(
    name = "boson3",
    about = "Lowers/Desugars Boson3 source files into Quark3 files"
)]
struct Cli {
    /// Input Boson3 source file
    input: PathBuf,

    /// Output Quark3 source file
    output: PathBuf,

    // Optional recursive macro expansion limit
    #[arg(long, short, default_value_t = 10000)]
    macro_expansion_limit: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_path = &cli.input;
    let output_path = &cli.output;
    let macro_expansion_limit = cli.macro_expansion_limit;

    // Read source file
    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", input_path.display());
        process::exit(1);
    });

    // Lower input source
    let lowerer: BosonLowerer<'_> = BosonLowerer::new(&source);
    let lowered = lowerer.lower().unwrap_or_else(|e| {
        eprintln!("lowering error: {e}");
        process::exit(1);
    });

    // Write output file
    fs::write(output_path, lowered).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", output_path.display());
        process::exit(1);
    });

    println!(
        "boson3 lowered {} -> {}",
        input_path.display(),
        output_path.display()
    );

    Ok(())
}
