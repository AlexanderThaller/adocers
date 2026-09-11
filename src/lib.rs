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
    render::{
        Options,
        diagram::{
            self,
            Source,
        },
        math,
    },
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

/// Where diagrams are drawn from, or `None` when they are not to be drawn.
///
/// Without `--mermaid-url` this names the copy vendored into this binary, at
/// the path a file render writes it to. Whoever is producing the page — the
/// file writer, or the server — replaces that with the path its own reader
/// will be able to reach; see [`uses_vendored_mermaid`].
fn mermaid(common: &CommonArgs) -> Option<Source> {
    if common.no_mermaid {
        return None;
    }

    Some(Source::Url(match &common.mermaid_url {
        Some(url) => url.clone(),
        None => diagram::ASSET_HREF.to_string(),
    }))
}

/// Whether diagrams are drawn from the vendored copy rather than a URL the
/// author named.
///
/// Only then does the caller have to put that copy somewhere the page can
/// reach: beside it on disk, or behind the server's own reserved path.
pub fn uses_vendored_mermaid(common: &CommonArgs) -> bool {
    !common.no_mermaid && common.mermaid_url.is_none()
}

/// Where equations are typeset from, or `None` when they are to be left as the
/// notation they were written in.
///
/// The same arrangement as [`mermaid`]: without `--mathjax-url` this names the
/// vendored copy, and whoever produces the page replaces it with a path its
/// reader can reach.
fn math(common: &CommonArgs) -> Option<math::Source> {
    if common.no_math {
        return None;
    }

    match &common.mathjax_url {
        Some(url) => Some(math::Source::Url(url.clone())),

        // A build without the `math` feature has nothing vendored to point at,
        // so an equation stays as it was written unless a URL was named.
        None if cfg!(feature = "math") => Some(math::Source::Url(math::ASSET_HREF.to_string())),
        None => None,
    }
}

/// Whether equations are typeset from the vendored copy rather than a URL the
/// author named.
pub fn uses_vendored_mathjax(common: &CommonArgs) -> bool {
    cfg!(feature = "math") && !common.no_math && common.mathjax_url.is_none()
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
