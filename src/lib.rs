//! `adocers` — render AsciiDoc documents to HTML.
//!
//! The command-line tool is a thin shell around this library: `src/main.rs`
//! parses the arguments and calls [`run`]. Everything else lives here so that
//! the benchmarks in `benches/` can reach it, since a benchmark can only link
//! against a library target.

pub mod cli;
pub mod diagnostics;
pub mod includes;
pub mod inputs;
pub mod job;
pub mod lint;
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
        CheckArgs,
        Cli,
        ColorChoice,
        Command,
        CommonArgs,
        MessageFormat,
        RenderArgs,
    },
    diagnostics::{
        Format,
        Reporter,
    },
    render::Options,
};

/// Run one invocation, having already parsed the command line.
///
/// The exit code is returned rather than taken, so that a caller which is not
/// a process — a test, or a benchmark — can see what happened.
pub fn run(cli: Cli) -> ExitCode {
    let format = format(match &cli.command {
        None => &cli.render.common,
        Some(Command::Render(args)) => &args.common,
        Some(Command::Check(args)) => &args.common,

        #[cfg(feature = "serve")]
        Some(Command::Serve(args)) => &args.common,
    });

    let result = match cli.command {
        None => render(&cli.render),
        Some(Command::Render(args)) => render(&args),
        Some(Command::Check(args)) => check(&args),

        #[cfg(feature = "serve")]
        Some(Command::Serve(args)) => serve::run(&args).map(|()| ExitCode::SUCCESS),
    };

    match result {
        Ok(code) => code,

        Err(error) => {
            diagnostics::error(format, &error);
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
        if format(&args.common) == Format::Json {
            diagnostics::summary(Format::Json, warnings, jobs.len());
        } else {
            eprintln!("adocers: {warnings} warning(s) reported and `--deny-warnings` is in effect");
        }

        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}

/// Report every requested document's diagnostics, and fail if there were any
/// warnings.
///
/// A directory among the inputs stands for every AsciiDoc document beneath it,
/// which is what makes `adocers check .` a whole CI step.
fn check(args: &CheckArgs) -> Result<ExitCode> {
    let inputs = inputs::expand(&args.inputs)?;
    let reporter = reporter(&args.common);
    let mut warnings = 0;

    for input in &inputs {
        warnings += job::check_file(input, &args.common, reporter)?.warnings;
    }

    if warnings > 0 {
        if !args.common.quiet {
            diagnostics::summary(format(&args.common), warnings, inputs.len());
        }

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
        format: format(common),
    }
}

/// How this run writes its diagnostics.
fn format(common: &CommonArgs) -> Format {
    match common.message_format {
        MessageFormat::Text => Format::Text,
        MessageFormat::Json => Format::Json,
    }
}
