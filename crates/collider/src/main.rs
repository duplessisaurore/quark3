//! `Quark3` is an experimental free and open-source textual assembly language
//! that compiles to `Lepton3` bytecode as part of the `Fermion3` language project.
//!
//! Check out the [repository README](https://github.com/duplessisaurore/quark3/blob/main/README.md)
//! for more information about the project and join the [Discord](https://discord.gg/wXzj2cqZ3Q) for
//! any discussion.
//!
//! ## Collider
//!
//! The `Collider` crate is a crate that provides a KISS build system
//! for a `Quark3` project.

use std::{error::Error, io, path::Path, process::Command};

use clap::Parser;
use serde::Deserialize;
use std::{fs, path::PathBuf, process};

#[derive(Parser)]
#[command(
    name = "collider3",
    about = "A KISS TOML based build system for a Gluon3 project"
)]
struct Cli {
    /// Input Collider3 build.toml
    input: PathBuf,

    /// Strip debugging source locations from the final Lepton3 image
    #[arg(long)]
    strip_debug: bool,

    /// Write a source map next to the final image
    #[arg(long)]
    source_map: bool,

    /// Verbose output or not
    #[arg(short, long)]
    verbose: bool,
}

/// The `build.toml` manifest for a project
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    /// Project name, used for the image output name
    name: String,

    /// Boson3 source files
    files: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let input_path = &cli.input;
    let verbose = cli.verbose;

    verbose_print(
        format!("collider3 >> reading source {}", input_path.display()),
        verbose,
    );

    // Read build manifest
    let source = fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", input_path.display());
        process::exit(1);
    });

    verbose_print(
        format!("collider3 >> parsing source {}", input_path.display()),
        verbose,
    );

    // Parse build manifest
    let manifest: Manifest = toml::from_str(&source).unwrap_or_else(|e| {
        eprintln!("error parsing {}: {e}", input_path.display());
        process::exit(1);
    });

    // All paths in the manifest are resolved relative to the manifest's
    // directory rather than the command
    let manifest_dir = match input_path.parent() {
        Some(path) if !path.as_os_str().is_empty() => path.to_path_buf(),
        _ => PathBuf::from("."),
    };

    verbose_print(
        format!("collider3 >> manifest dir {}", manifest_dir.display()),
        verbose,
    );

    verbose_print("collider3 >> resolving files".to_string(), verbose);

    // Resolve globs into a deduplicated file list
    let files = resolve_files(&manifest_dir, &manifest.files);

    if files.is_empty() {
        eprintln!("error: no input files matched in {}", input_path.display());
        process::exit(1);
    }

    // Create the build directory for intermediates and the final image
    let build_dir = manifest_dir.join("build");
    fs::create_dir_all(&build_dir).unwrap_or_else(|e| {
        eprintln!("error creating {}: {e}", build_dir.display());
        process::exit(1);
    });

    verbose_print(
        format!("collider3 >> build dir {}", build_dir.display()),
        verbose,
    );

    // These are the names of the temporaries/output (l3)
    let name = &manifest.name;
    let photon_output_dir = build_dir.join("photon3");
    let linked_path = build_dir.join(format!("{name}.linked.boson3"));
    let lowered_path = build_dir.join(format!("{name}.quark3"));
    let image_path = build_dir.join(format!("{name}.lepton3"));

    // Run all build tools

    // run photon3 first to lower all photon3 files to boson3
    let (photon_files, mut link_files): (Vec<PathBuf>, Vec<PathBuf>) =
        files.into_iter().partition(|path| is_photon_source(path));

    if !photon_files.is_empty() {
        // obliterate all photon3 files every build, else we may get stale bad things
        recreate_directory(&photon_output_dir);

        // run photon3

        let mut photon = Command::new("photon3");
        photon
            .args(&photon_files)
            .arg("--output-dir")
            .arg(&photon_output_dir);

        verbose_print(format!("collider3 >> running {:?}", photon), verbose);
        run(photon);

        // get all the generated boson3 files
        
        let mut generated_boson_files =
            files_in_directory_with_extension(&photon_output_dir, "boson3");

        if generated_boson_files.is_empty() {
            eprintln!(
                "photon3 succeeded but produced no .boson3 files in {}",
                photon_output_dir.display()
            );
            process::exit(1);
        }

        verbose_print(
            format!(
                "collider3 >> photon3 produced {} Boson3 file(s)",
                generated_boson_files.len()
            ),
            verbose,
        );

         link_files.append(&mut generated_boson_files);
    }

    // link
    if link_files.is_empty() {
        eprintln!("error: no Boson3 files available for linking");
        process::exit(1);
    }

    // Link native Boson3 and Photon3-generated Boson3 together.
    let mut gluon = Command::new("gluon3");
    gluon.args(&link_files).arg("--output").arg(&linked_path);

    verbose_print(format!("collider3 >> running {:?}", gluon), verbose);
    run(gluon);

    // lower
    let mut boson = Command::new("boson3");
    boson.arg(&linked_path).arg(&lowered_path);

    verbose_print(format!("collider3 >> running {:?}", boson), verbose);
    run(boson);

    // asm
    let mut quark = Command::new("quark3");
    quark.arg(&lowered_path).arg(&image_path);
    if cli.strip_debug {
        quark.arg("--strip-debug");
        println!("collider3 >> stripping debug information");
    }

    // asm with source map if requested
    if cli.source_map {
        let source_map_location = build_dir.join(format!("{name}.map"));

        quark.arg("--source-map").arg(&source_map_location);

        println!(
            "collider3 >> building source map {name} -> {}",
            source_map_location.display()
        );
    }
    verbose_print(format!("collider3 >> running {:?}", quark), verbose);
    run(quark);

    // the successful output !!
    println!("collider3 >> built {name} -> {}", image_path.display());

    Ok(())
}

/// Expands every pattern relative to `manifest_dir` with globbing support
fn resolve_files(manifest_dir: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();

    // Get all the patterns
    for pattern in patterns {
        let full_pattern = manifest_dir.join(pattern);

        // Glob them all if they're a globby pattern
        let entries = glob::glob(&full_pattern.to_string_lossy()).unwrap_or_else(|e| {
            eprintln!("invalid glob pattern `{pattern}`: {e}");
            process::exit(1);
        });

        let mut matched = 0usize;

        // Add all the path entries to our total files.
        for entry in entries {
            let path = entry.unwrap_or_else(|e| {
                eprintln!("error while matching `{pattern}`: {e}");
                process::exit(1);
            });

            if !path.is_file() {
                continue;
            }

            matched += 1;

            // A file may be matched by more than one pattern, only keep
            // the first occurrence
            if !files.contains(&path) {
                files.push(path);
            }
        }

        if matched == 0 {
            // A literal path that matched nothing is a missing file
            //
            // This is not allowed, other than a glob (which will warn)
            if pattern.contains(['*', '?', '[']) {
                eprintln!("warning: pattern `{pattern}` matched no files");
            } else {
                eprintln!("error: file {} not found", full_pattern.display());
                process::exit(1);
            }
        }
    }

    files
}

/// Runs a build tool to completion, exiting with its status code if it fails.
fn run(mut command: Command) {
    let program = command.get_program().to_string_lossy().to_string();

    let status = command.status().unwrap_or_else(|e| {
        eprintln!("error running {program}: {e}");

        // help texts for some errors
        match e.kind() {
            io::ErrorKind::NotFound => eprintln!("is `{program}` installed and on your PATH?"),
            io::ErrorKind::PermissionDenied => eprintln!("is `{program}` executable?"),
            _ => {}
        }

        process::exit(1);
    });

    if !status.success() {
        eprintln!("{program} failed, aborting build");
        process::exit(status.code().unwrap_or(1));
    }
}

fn verbose_print(out: String, verbose: bool) {
    if verbose {
        println!("{}", out)
    };
}

/// Removes `path` if it already exists, then recreates it empty.
fn recreate_directory(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap_or_else(|error| {
            eprintln!("error cleaning {}: {error}", path.display());
            process::exit(1);
        });
    }

    fs::create_dir_all(path).unwrap_or_else(|error| {
        eprintln!("error creating {}: {error}", path.display());
        process::exit(1);
    });
}

/// Returns whether this source should first be lowered through Photon3.
fn is_photon_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("p3") | Some("photon3")
    )
}

/// Returns files directly inside `directory` with the requested extension.
fn files_in_directory_with_extension(directory: &Path, extension: &str) -> Vec<PathBuf> {
    let entries = fs::read_dir(directory).unwrap_or_else(|error| {
        eprintln!("error reading {}: {error}", directory.display());
        process::exit(1);
    });

    let mut files = entries
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                eprintln!(
                    "error reading directory entry in {}: {error}",
                    directory.display()
                );
                process::exit(1);
            })
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value == extension)
        })
        .collect::<Vec<_>>();

    files.sort();
    files
}