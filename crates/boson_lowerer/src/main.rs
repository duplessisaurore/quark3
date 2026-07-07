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

use std::error::Error;

pub mod preprocessor;
pub mod errors;

fn main() -> Result<(), Box<dyn Error>> {
    
    
    Ok(())
}