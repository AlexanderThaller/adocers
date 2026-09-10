//! `adocers` — render AsciiDoc documents to HTML.

mod cli;
mod diagnostics;
mod includes;
mod job;
mod render;
mod watch;

use std::{
    io::IsTerminal,
    process::ExitCode,
};

use anyhow::{
    Context,
    Result,
};
use clap::Parser as _;

use crate::{
    cli::{
        Cli,
        ColorChoice,
    },
    diagnostics::Reporter,
    render::Options,
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match run(&cli) {
        Ok(code) => code,

        Err(error) => {
            eprintln!("adocers: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Render every requested document, then optionally keep watching.
fn run(cli: &Cli) -> Result<ExitCode> {
    let jobs = job::plan(cli)?;
    let options = options(cli)?;
    let reporter = reporter(cli);

    if cli.watch {
        watch::run(&jobs, cli, &options, &reporter)?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut warnings = 0;

    for job in &jobs {
        let outcome = job::run(job, cli, &options, &reporter)?;
        warnings += outcome.counts.warnings;
    }

    if cli.deny_warnings && warnings > 0 {
        eprintln!("adocers: {warnings} warning(s) reported and `--deny-warnings` is in effect");

        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}

/// Decide how the rendered body should be wrapped and styled.
fn options(cli: &Cli) -> Result<Options> {
    if cli.fragment {
        return Ok(Options {
            fragment: true,
            stylesheet: None,
        });
    }

    let stylesheet = match &cli.css {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("reading stylesheet `{}`", path.display()))?,
        ),

        None if cli.no_css => None,
        None => Some(render::default_stylesheet()),
    };

    Ok(Options {
        fragment: false,
        stylesheet,
    })
}

/// Configure diagnostic output for this run.
fn reporter(cli: &Cli) -> Reporter {
    Reporter {
        color: match cli.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => std::io::stderr().is_terminal(),
        },

        verbose: cli.verbose,
        quiet: cli.quiet,
    }
}
