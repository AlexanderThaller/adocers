//! Command line surface for `adocers`.

use std::path::PathBuf;

use clap::{
    Args,
    Parser,
    Subcommand,
    ValueEnum,
};

/// Render AsciiDoc documents to HTML.
///
/// With no subcommand the arguments are those of `render`, so `adocers
/// doc.adoc` and `adocers render doc.adoc` do the same thing.
#[derive(Debug, Parser)]
#[command(name = "adocers", version, about, long_about = None)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
pub struct Cli {
    /// The subcommand to run, if one was named.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// The arguments of the implied `render` command.
    #[command(flatten)]
    pub render: RenderArgs,
}

/// What the invocation asked for.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Render documents to HTML files (the default).
    Render(RenderArgs),

    /// Serve a directory over HTTP, rendering documents as they are requested.
    #[cfg(feature = "serve")]
    Serve(ServeArgs),
}

/// Options shared by every way of producing HTML.
#[expect(
    clippy::struct_excessive_bools,
    reason = "these are command line flags, and one field per flag is what clap asks for"
)]
#[derive(Args, Clone, Debug)]
pub struct CommonArgs {
    /// Embed this stylesheet in the page instead of the built-in one.
    #[arg(long, value_name = "FILE", conflicts_with = "no_css", global = true)]
    pub css: Option<PathBuf>,

    /// Emit pages without any stylesheet.
    #[arg(long = "no-css", global = true)]
    pub no_css: bool,

    /// Set a document attribute, as `NAME`, `NAME=VALUE`, or `NAME!` to unset.
    ///
    /// Repeat the flag to set several. These behave like attributes passed on
    /// the Asciidoctor command line: they are set before the document header is
    /// parsed and cannot be overridden by the document itself.
    #[arg(
        short = 'a',
        long = "attribute",
        value_name = "NAME[=VALUE]",
        global = true
    )]
    pub attributes: Vec<String>,

    /// Mark admonitions with their label instead of an icon.
    #[arg(long = "no-icons", global = true)]
    pub no_icons: bool,

    /// Load mermaid from this URL instead of the copy built into this binary.
    ///
    /// It has to be a UMD build — one that defines `window.mermaid` — such as
    /// `https://cdn.jsdelivr.net/npm/mermaid@12/dist/mermaid.min.js`. Without
    /// this flag no page reaches the network: the server hands out its own
    /// copy, and a file render writes one beside the page.
    #[arg(long, value_name = "URL", conflicts_with = "no_mermaid", global = true)]
    pub mermaid_url: Option<String>,

    /// Show mermaid diagrams as the listing blocks they were written as.
    #[arg(long = "no-mermaid", global = true)]
    pub no_mermaid: bool,

    /// How far a document may reach outside itself.
    #[arg(long, value_enum, default_value_t = SafeMode::Unsafe, value_name = "MODE", global = true)]
    pub safe_mode: SafeMode,

    /// Also report low-severity (debug) diagnostics.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress diagnostics entirely.
    #[arg(short, long, conflicts_with = "verbose", global = true)]
    pub quiet: bool,

    /// When to colorize diagnostics.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, value_name = "WHEN", global = true)]
    pub color: ColorChoice,
}

/// Arguments of the `render` command.
///
/// By default each `FILE` is rendered to a sibling `.html` file. Parse
/// diagnostics are reported against the source with `ariadne` and never stop a
/// render; pass `--deny-warnings` to make them fail the run instead.
#[derive(Args, Debug)]
pub struct RenderArgs {
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
    #[arg(long)]
    pub fragment: bool,

    /// Re-render each document whenever it or one of its includes changes.
    #[arg(short, long)]
    pub watch: bool,

    /// Exit with a non-zero status if any warning is reported.
    ///
    /// Ignored while `--watch` is in effect, where a failing render must not
    /// end the session.
    #[arg(long)]
    pub deny_warnings: bool,

    /// Options shared with `serve`.
    #[command(flatten)]
    pub common: CommonArgs,
}

/// Arguments of the `serve` command.
///
/// Documents are rendered when they are requested, so there is nothing to build
/// first and nothing left behind. Pages reload themselves when anything under
/// the served directory changes.
#[cfg(feature = "serve")]
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Directory to serve.
    #[arg(value_name = "DIR", default_value = ".")]
    pub root: PathBuf,

    /// Address to listen on, as `HOST:PORT` or just a port number.
    #[arg(short, long, value_name = "ADDR", default_value = "127.0.0.1:8080")]
    pub bind: String,

    /// Document to render for a directory, tried in the order given.
    ///
    /// Replaces the defaults (`INDEX.adoc` and `README.adoc`) rather than
    /// adding to them. Repeat the flag to name several.
    #[arg(
        long = "index-file",
        value_name = "NAME",
        conflicts_with = "no_index_file"
    )]
    pub index_files: Vec<String>,

    /// Never render a document in place of a directory.
    #[arg(long = "no-index-file")]
    pub no_index_file: bool,

    /// Do not show a browsable listing of a directory's contents.
    #[arg(long = "no-listing")]
    pub no_listing: bool,

    /// Do not reload pages in the browser when their sources change.
    #[arg(long = "no-reload")]
    pub no_reload: bool,

    /// Options shared with `render`.
    #[command(flatten)]
    pub common: CommonArgs,
}

/// The index documents tried for a directory when none were named.
#[cfg(feature = "serve")]
pub const DEFAULT_INDEX_FILES: [&str; 2] = ["INDEX.adoc", "README.adoc"];

#[cfg(feature = "serve")]
impl ServeArgs {
    /// The documents to try for a directory, in order; empty when the lookup is
    /// switched off.
    pub fn index_files(&self) -> Vec<String> {
        if self.no_index_file {
            return Vec::new();
        }

        if self.index_files.is_empty() {
            return DEFAULT_INDEX_FILES
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
        }

        self.index_files.clone()
    }
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

impl RenderArgs {
    /// Whether the single output destination is standard output.
    pub fn writes_to_stdout(&self) -> bool {
        self.output
            .as_deref()
            .is_some_and(|path| path == std::path::Path::new("-"))
    }
}
