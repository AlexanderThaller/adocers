//! `adocers` — render AsciiDoc documents to HTML.
//!
//! The command-line tool is a thin shell around this library: `src/main.rs`
//! parses the arguments and calls [`run`]. Everything else lives here so that
//! the benchmarks in `benches/` can reach it, since a benchmark can only link
//! against a library target.

pub mod cli;
pub mod diagnostics;
pub mod includes;
pub mod job;
pub mod render;
#[cfg(feature = "serve")]
pub mod serve;
pub mod watch;

use std::{
    io::IsTerminal,
    process::ExitCode,
};

use anyhow::{
    Context,
    Result,
};

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

/// Run one invocation, having already parsed the command line.
///
/// The exit code is returned rather than taken, so that a caller which is not
/// a process — a test, or a benchmark — can see what happened.
pub fn run(cli: Cli) -> ExitCode {
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
pub fn options(common: &CommonArgs, fragment: bool) -> Result<Options> {
    // A fragment keeps its diagrams' markup but never the script that draws
    // them: the page it is embedded in owns what it loads.
    if fragment {
        return Ok(Options {
            fragment: true,
            stylesheet: None,
            body_suffix: String::new(),
            icons: !common.no_icons,
            highlight: !common.no_highlight,
            mermaid: mermaid(common),
            math: math(common),
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
        icons: !common.no_icons,
        highlight: !common.no_highlight,
        mermaid: mermaid(common),
        math: math(common),
    })
}

/// Whether a mermaid block is drawn as a diagram.
///
/// Drawing happens here, while the page is being built, so there is nothing for
/// the page to fetch and nothing to place beside it. A build without the
/// `mermaid` feature draws nothing whatever this says.
fn mermaid(common: &CommonArgs) -> bool {
    cfg!(feature = "mermaid") && !common.no_mermaid
}

/// Whether an equation is converted to `MathML`.
///
/// Conversion happens here, while the page is being built, so there is nothing
/// for the page to fetch. A build without the `math` feature converts nothing
/// whatever this says.
fn math(common: &CommonArgs) -> bool {
    cfg!(feature = "math") && !common.no_math
}

/// Configure diagnostic output for this run.
pub fn reporter(common: &CommonArgs) -> Reporter {
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
