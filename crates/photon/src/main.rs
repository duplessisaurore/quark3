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
//! and type information ontop of `Boson3` std.b3.

use std::{fs, path::PathBuf};

use clap::Parser;
use miette::{Diagnostic, IntoDiagnostic, NamedSource, Result, SourceSpan, WrapErr, miette};
use thiserror::Error;

use crate::lexer::Lexer;
use crate::lowerer::{SourceMap, SymbolTable, lower_modules};
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

/// An error tied to a span in a source file
///
/// We then pass this to miette for fancy printing
#[derive(Debug, Error, Diagnostic)]
#[error("failed to {stage} {file}")]
#[diagnostic(code(photon3::error))]
struct SourceError {
    stage: &'static str,
    file: String,
    message: String,

    #[source_code]
    src: NamedSource<String>,

    #[label("{message}")]
    span: SourceSpan,
}

impl SourceError {
    /// Returns a new photon3::SourceError that can
    /// be returned and then prettified by miette.
    fn new(
        stage: &'static str,
        file: &str,
        source: &str,
        message: String,
        span: impl Into<SourceSpan>,
    ) -> Self {
        Self {
            stage,
            file: file.to_owned(),
            message,
            src: NamedSource::new(file, source.to_owned()),
            span: span.into(),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let input_paths = &cli.input;
    let output_dir = &cli.output_dir;

    let mut modules = Vec::new();

    // (source map, file name, source text), in the same order as modules.
    let mut sources = Vec::new();

    // Read source files
    for source_file in input_paths {
        let file_name = source_file.display().to_string();

        let source = fs::read_to_string(source_file)
            .into_diagnostic()
            .wrap_err_with(|| format!("error reading {file_name}"))?;

        let source_map = SourceMap::new(&source);

        // Lex the source file
        let tokens = Lexer::new(&source)
            .tokenize()
            .map_err(|e| SourceError::new("lex", &file_name, &source, e.to_string(), e.span))?;

        // Parse it
        let ast = PhotonParser::new(source_file.to_string_lossy().to_string(), &tokens)
            .parse_module()
            .map_err(|e| SourceError::new("parse", &file_name, &source, e.to_string(), e.span))?;

        sources.push((source_map, file_name, source));
        modules.push(ast);
    }

    // Make symbol table
    let symbol_table = SymbolTable::collect_all_symbols_from_modules(&modules)
        .map_err(|e| miette!("failed to collect symbols: {e}"))?;

    // Get every pasred module and its source map for the lowering pass.
    let inputs: Vec<_> = modules
        .iter()
        .zip(&sources)
        .map(|(module, (source_map, _, _))| (module, source_map))
        .collect();

    // Lower all the modules
    let lowered_modules = lower_modules(&symbol_table, &inputs).map_err(|(module_index, e)| {
        let (_, file_name, source) = &sources[module_index];
        SourceError::new("lower", file_name, source, e.to_string(), e.span)
    })?;

    // Outputs follow input order in `inputs`.
    for (module, lowered) in modules.iter().zip(lowered_modules) {
        let mut output_path = output_dir.clone();

        // Write output file
        output_path.push(format!(
            "{}.boson3",
            module.namespace.to_string().replace("::", "_")
        ));

        fs::write(&output_path, lowered.contents)
            .into_diagnostic()
            .wrap_err_with(|| format!("error writing {}", output_path.display()))?;
    }

    Ok(())
}
