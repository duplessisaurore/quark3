//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Photon
//!
//! The `Photon3` crate is a crate that desugars some extra `Photon3` syntax
//! and type information ontop of `Boson3` std.b3, view the `README.md` in the repository.

use std::{error::Error, fs, path::PathBuf, process};

use clap::Parser;

use crate::lexer::Lexer;
use crate::parser::Parser as PhotonParser;
mod ast;
mod errors;
mod lexer;
mod parser;
mod lowerer;

#[derive(Parser)]
#[command(
    name = "photon3",
    about = "Lowers/Desugars Photon3 source files into Boson3 files"
)]
struct Cli {
    /// Input Photon3 source file
    input: PathBuf,

    /// Output Boson3 source file
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_path = &cli.input;
    let _output_path = &cli.output;

    // Read source file
    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", input_path.display());
        process::exit(1);
    });

    // Lex the source file
    let tokens = Lexer::new(&source).tokenize().unwrap_or_else(|e| {
        eprintln!("failed to tokenise file: {e}");
        process::exit(1);
    });

    let ast = PhotonParser::new(&tokens)
        .parse_module()
        .unwrap_or_else(|e| {
            eprintln!("failed to parse file: {e}");
            process::exit(1);
        });

    println!("{:?}", ast);

    Ok(())
}
