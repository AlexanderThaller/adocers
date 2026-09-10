//! `adocers` — render AsciiDoc documents to HTML.

mod cli;
mod diagnostics;
mod includes;
mod job;
mod render;
#[cfg(feature = "serve")]
mod serve;
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
        Command,
        CommonArgs,
        RenderArgs,
    },
    diagnostics::Reporter,
    render::Options,
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        None => render(&cli.render),
        Some(Command::Render(args)) => render(&args),

        #[cfg(feature = "serve")]
        Some(Command::Serve(args)) => serve::run(&args).map(|()| ExitCode::SUCCESS),
    };

    match result {
        Ok(code) => code,

        Err(error) => {
            eprintln!("adocers: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Render every requested document, then optionally keep watching.
fn render(args: &RenderArgs) -> Result<ExitCode> {
    let jobs = job::plan(args)?;
    let options = options(&args.common, args.fragment)?;
    let reporter = reporter(&args.common);

    if args.watch {
        watch::run(&jobs, &args.common, &options, reporter)?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut warnings = 0;

    for job in &jobs {
        let outcome = job::run(job, &args.common, &options, reporter)?;
        warnings += outcome.counts.warnings;
    }

    if args.deny_warnings && warnings > 0 {
        eprintln!("adocers: {warnings} warning(s) reported and `--deny-warnings` is in effect");

        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}

/// Decide how a rendered body should be wrapped and styled.
pub(crate) fn options(common: &CommonArgs, fragment: bool) -> Result<Options> {
    // A fragment keeps its diagrams' markup but never the script that draws
    // them: the page it is embedded in owns what it loads.
    if fragment {
        return Ok(Options {
            fragment: true,
            stylesheet: None,
            body_suffix: String::new(),
            mermaid: mermaid(common),
        });
    }

    let stylesheet = match &common.css {
        Some(path) => Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("reading stylesheet `{}`", path.display()))?,
        ),

        None if common.no_css => None,
        None => Some(render::default_stylesheet()),
    };

    Ok(Options {
        fragment: false,
        stylesheet,
        body_suffix: String::new(),
        mermaid: mermaid(common),
    })
}

/// Where diagrams are drawn from, or `None` when they are not to be drawn.
fn mermaid(common: &CommonArgs) -> Option<String> {
    if common.no_mermaid {
        return None;
    }

    Some(
        common
            .mermaid_url
            .clone()
            .unwrap_or_else(render::default_mermaid_url),
    )
}

/// Configure diagnostic output for this run.
pub(crate) fn reporter(common: &CommonArgs) -> Reporter {
    Reporter {
        color: match common.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => std::io::stderr().is_terminal(),
        },

        verbose: common.verbose,
        quiet: common.quiet,
    }
}
