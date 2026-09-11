//! The `adocers` command line.
//!
//! Argument parsing and the process exit code; everything else is the library,
//! so that the benchmarks can reach it.

use std::process::ExitCode;

use clap::Parser as _;

fn main() -> ExitCode {
    adocers::run(adocers::cli::Cli::parse())
}
