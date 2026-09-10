//! Command line surface for `adocers`.

use std::path::PathBuf;

use clap::{
    Parser,
    ValueEnum,
};

/// Render AsciiDoc documents to HTML.
///
/// By default each `FILE` is rendered to a sibling `.html` file. Parse
/// diagnostics are reported against the source with `ariadne` and never stop a
/// render; pass `--deny-warnings` to make them fail the run instead.
#[derive(Debug, Parser)]
#[command(name = "adocers", version, about, long_about = None)]
pub struct Cli {
    /// AsciiDoc files to render.
    #[arg(value_name = "FILE", required = true)]
    pub inputs: Vec<PathBuf>,

    /// Write output here instead of beside each input.
    ///
    /// With several inputs this must name a directory. With a single input it
    /// may name a file, a directory, or `-` for standard output.
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Emit only the rendered body, without the surrounding HTML page.
    #[arg(long, conflicts_with_all = ["css", "no_css"])]
    pub fragment: bool,

    /// Embed this stylesheet in the page instead of the built-in one.
    #[arg(long, value_name = "FILE", conflicts_with = "no_css")]
    pub css: Option<PathBuf>,

    /// Emit the page without any stylesheet.
    #[arg(long = "no-css")]
    pub no_css: bool,

    /// Set a document attribute, as `NAME`, `NAME=VALUE`, or `NAME!` to unset.
    ///
    /// Repeat the flag to set several. These behave like attributes passed on
    /// the Asciidoctor command line: they are set before the document header is
    /// parsed and cannot be overridden by the document itself.
    #[arg(short = 'a', long = "attribute", value_name = "NAME[=VALUE]")]
    pub attributes: Vec<String>,

    /// How far a document may reach outside itself.
    #[arg(long, value_enum, default_value_t = SafeMode::Unsafe, value_name = "MODE")]
    pub safe_mode: SafeMode,

    /// Re-render each document whenever it or one of its includes changes.
    #[arg(short, long)]
    pub watch: bool,

    /// Exit with a non-zero status if any warning is reported.
    ///
    /// Ignored while `--watch` is in effect, where a failing render must not
    /// end the session.
    #[arg(long)]
    pub deny_warnings: bool,

    /// Also report low-severity (debug) diagnostics.
    #[arg(short, long)]
    pub verbose: bool,

    /// Suppress diagnostics entirely.
    #[arg(short, long, conflicts_with = "verbose")]
    pub quiet: bool,

    /// When to colorize diagnostics.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, value_name = "WHEN")]
    pub color: ColorChoice,
}

/// Mirrors [`asciidoc_parser::SafeMode`] as a `clap` value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum SafeMode {
    /// No restrictions; the default for this tool, as for the Asciidoctor CLI.
    Unsafe,
    /// Includes and file reads are confined to the document's own directory.
    Safe,
    /// As `safe`, and attributes that expose the local file system are locked.
    Server,
    /// As `server`, and docinfo and embedded content are disabled.
    Secure,
}

impl From<SafeMode> for asciidoc_parser::SafeMode {
    fn from(mode: SafeMode) -> Self {
        match mode {
            SafeMode::Unsafe => Self::Unsafe,
            SafeMode::Safe => Self::Safe,
            SafeMode::Server => Self::Server,
            SafeMode::Secure => Self::Secure,
        }
    }
}

/// When to emit ANSI color in diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum ColorChoice {
    /// Colorize when stderr is a terminal.
    Auto,
    /// Always colorize.
    Always,
    /// Never colorize.
    Never,
}

impl Cli {
    /// Whether the single output destination is standard output.
    pub fn writes_to_stdout(&self) -> bool {
        self.output
            .as_deref()
            .is_some_and(|p| p == std::path::Path::new("-"))
    }
}
