//! `adocers-review`: read a directory of AsciiDoc on a phone, and leave
//! comments on its blocks.
//!
//! It is `adocers serve` with one thing added. Documents are rendered by the
//! same pipeline, with the same options on the command line, and each page
//! carries a script that lets a reader tap a paragraph, a list item, a listing
//! or a heading and say something about it. The comments are kept in JSON
//! files beside the tree (see [`store`]), where whoever writes the documents
//! can read them back.
//!
//! It is a binary of its own, rather than a subcommand, while its shape
//! settles.

mod listing;
mod server;
mod store;

use std::{
    path::PathBuf,
    process::ExitCode,
};

use adocers::cli::CommonArgs;
use clap::Parser;

/// Read AsciiDoc on a phone and leave comments on its blocks.
#[derive(Debug, Parser)]
#[command(name = "adocers-review", version)]
struct Args {
    /// Directory of documents to review.
    #[arg(value_name = "DIR", default_value = ".")]
    root: PathBuf,

    /// Address to listen on, as `HOST:PORT` or just a port number.
    ///
    /// Only this machine can reach the default. To review from a phone, listen
    /// on the network as well — `--bind 0.0.0.0:8080` — and bear in mind that
    /// anyone who can reach the address can then read the documents and
    /// comment on them.
    #[arg(short, long, value_name = "ADDR", default_value = "127.0.0.1:0")]
    bind: String,

    /// Options shared with `adocers render`.
    #[command(flatten)]
    common: CommonArgs,
}

fn main() -> ExitCode {
    match server::run(&Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,

        Err(error) => {
            eprintln!("adocers-review: {error:#}");
            ExitCode::FAILURE
        }
    }
}
