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

pub mod linker;

use clap::Parser;
use regex::Regex;
use std::{fs, path::PathBuf, process};

use crate::linker::{LinkableFile, Linker};

#[derive(Parser)]
#[command(
    name = "gluon3",
    about = "Links multiple Boson3 source files together into one Boson3 file"
)]
struct Cli {
    /// Input Boson3 source files
    input: Vec<PathBuf>,

    /// Output Quark3 source file
    #[arg(short, long)]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_paths = &cli.input;
    let output_path = &cli.output;

    // we need this for later..
    let original_source_regex = Regex::new(r"(?m)^\s*@original_source.*$")?;

    // Read source files3
    let source_files = input_paths
        .iter()
        .map(|input_path| {
            let mut file_contents = fs::read_to_string(input_path).unwrap_or_else(|e| {
                eprintln!("error reading {}: {e}", input_path.display());
                process::exit(1);
            });

            // check if original source line exists
            let original_source_line = file_contents.lines().find(|line| {
                line.trim().starts_with("@original_source")
            }).map(|original_source| {
                original_source.trim()
                    .strip_prefix("@original_source")
                    .expect(&format!("original source directive in file `{input_path:?}` must be followed by original file name!"))
                    .to_string()
            });

            // Remove the original source line..
            file_contents = original_source_regex.replace(&file_contents, "\n").to_string();
            
            // Resolve the input path down for file name
            let file_name = original_source_line.clone().unwrap_or_else(|| input_path
                .file_name()
                .unwrap_or_else(|| {
                    eprintln!("unable to simplify input path {}", input_path.display());
                    process::exit(1);
                })
                .to_string_lossy()
                .to_string()
            );

            // full file path for errors/file table
            let full_file_name = original_source_line.unwrap_or_else(|| input_path.to_string_lossy().to_string());

            LinkableFile {
                file_contents,
                file_name,
                full_file_name,
                namespace: None,
                remap_maps: None,
            }
        })
        .collect::<Vec<_>>();

    // Link input sources
    let linker = Linker::new(source_files);
    let linked = linker.link().unwrap_or_else(|e| {
        eprintln!("linking errors:");
        for error in e {
            eprintln!("{error}");
        }

        process::exit(1);
    });

    // Write output file
    fs::write(output_path, linked).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", output_path.display());
        process::exit(1);
    });

    println!(
        "gluon3 linked {:?} -> {}",
        input_paths,
        output_path.display()
    );

    Ok(())
}
