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

use std::collections::HashMap;
use std::{error::Error, fs, path::PathBuf, process};

use clap::Parser;

use crate::lexer::Lexer;
use crate::lowerer::{Lowerer, SourceMap, SymbolTable};
use crate::parser::Parser as PhotonParser;
mod ast;
mod errors;
mod lexer;
mod lowerer;
mod parser;

#[derive(Parser)]
#[command(
    name = "photon3",
    about = "Lowers/Desugars Photon3 source files into Boson3 files"
)]
struct Cli {
    /// Input Photon3 source files
    input: Vec<PathBuf>,

    /// Output Boson3 source file directory
    #[arg(short, long)]
    output_dir: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_paths = &cli.input;
    let output_dir = &cli.output_dir;

    let mut modules = Vec::new();
    let mut source_maps = HashMap::new();

    // Read source files
    for source_file in input_paths {
        let source = fs::read_to_string(source_file).unwrap_or_else(|e| {
            eprintln!("error reading {}: {e}", source_file.display());
            process::exit(1);
        });

        let source_map = SourceMap::new(&source);

        // Lex the source file
        let tokens = Lexer::new(&source).tokenize().unwrap_or_else(|e| {
            eprintln!(
                "failed to parse file: {e}, {} {}:{}",
                source_file.to_string_lossy(),
                source_map.span_start(e.span).line,
                source_map.span_start(e.span).column
            );
            process::exit(1);
        });

        // Parse it
        let ast = PhotonParser::new(source_file.to_string_lossy().to_string(), &tokens)
            .parse_module()
            .unwrap_or_else(|e| {
                eprintln!(
                    "failed to parse file: {e}, {} {}:{}",
                    source_file.to_string_lossy(),
                    source_map.span_start(e.span).line,
                    source_map.span_start(e.span).column
                );
                process::exit(1);
            });

        source_maps.insert(ast.namespace.clone(), source_map);
        modules.push(ast);
    }

    // make symbol table
    let symbol_table =
        SymbolTable::collect_all_symbols_from_modules(&modules).unwrap_or_else(|e| {
            eprintln!("failed to collect smybols for file: {e} ");
            process::exit(1);
        });

    for module in modules {
        let source_map = source_maps.get(&module.namespace).unwrap();
        let lowered_stuff = Lowerer::new(&symbol_table, &source_map, &module)
            .lower_to_string()
            .unwrap_or_else(|e| {
                eprintln!(
                    "failed to lower file: {e}, {} {}:{}",
                    module.namespace,
                    source_map.span_start(e.span).line,
                    source_map.span_start(e.span).column
                );
                process::exit(1);
            });

        let mut output_path = output_dir.clone();

        // Write output file
        output_path.push(format!(
            "{}.boson3",
            module.namespace.to_string().replace("::", "_")
        ));

        fs::write(output_path.clone(), lowered_stuff.contents).unwrap_or_else(|e| {
            eprintln!("error writing {}: {e}", output_path.display());
            process::exit(1);
        });
    }

    Ok(())
}
