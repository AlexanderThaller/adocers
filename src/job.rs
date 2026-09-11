//! Turning the command line into render jobs, and running one.

use std::{
    fs,
    io::Write as _,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Context,
    Result,
    bail,
};
use asciidoc_parser::{
    Parser,
    parser::ModificationContext,
};

use crate::{
    cli::{
        CommonArgs,
        RenderArgs,
    },
    diagnostics::{
        Counts,
        Reporter,
    },
    includes::{
        Dependencies,
        FsIncludeHandler,
    },
    render::{
        self,
        Options,
    },
};

/// Where a rendered document is written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination {
    /// A file on disk.
    File(PathBuf),

    /// Standard output.
    Stdout,
}

/// One document to render, and where its HTML goes.
#[derive(Clone, Debug)]
pub struct Job {
    /// The AsciiDoc file to read.
    pub input: PathBuf,

    /// Where to put the result.
    pub destination: Destination,
}

/// What one render produced.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// The rendered HTML.
    pub html: String,

    /// Diagnostics reported for the document.
    pub counts: Counts,

    /// Every file the render read: the document and its includes.
    ///
    /// This is what `--watch` watches; an include that appears only after the
    /// document is edited is picked up on the next pass.
    pub dependencies: Vec<PathBuf>,
}

/// Work out where each input's HTML should go.
pub fn plan(args: &RenderArgs) -> Result<Vec<Job>> {
    let single = args.inputs.len() == 1;

    if args.writes_to_stdout() && !single {
        bail!(
            "`--output -` writes one document to standard output, but {} were given",
            args.inputs.len()
        );
    }

    let mut jobs = Vec::with_capacity(args.inputs.len());

    for input in &args.inputs {
        let destination = match &args.output {
            None => Destination::File(html_sibling(input)),

            Some(output) if output == Path::new("-") => Destination::Stdout,

            Some(output) => {
                // With several inputs the destination can only sensibly be a
                // directory; with one it may be either, so an existing
                // directory (or a trailing separator) decides.
                if !single || is_directory_path(output) {
                    fs::create_dir_all(output).with_context(|| {
                        format!("creating output directory `{}`", output.display())
                    })?;

                    Destination::File(output.join(html_name(input)?))
                } else {
                    Destination::File(output.clone())
                }
            }
        };

        jobs.push(Job {
            input: input.clone(),
            destination,
        });
    }

    Ok(jobs)
}

/// Parse and render one document, without deciding what to do with the result.
///
/// This is the whole pipeline a rendered page comes out of, and it is shared:
/// the `render` command writes the HTML to a file, and `serve` hands it
/// straight back to the browser.
pub fn render_file(
    input: &Path,
    common: &CommonArgs,
    options: &Options,
    reporter: Reporter,
) -> Result<Outcome> {
    let source =
        fs::read_to_string(input).with_context(|| format!("reading `{}`", input.display()))?;

    let dependencies = Dependencies::default();
    dependencies.insert(input.to_path_buf());

    let display_name = input.to_string_lossy().into_owned();
    let safe_mode = common.safe_mode.into();

    let mut parser = Parser::default()
        .with_safe_mode(safe_mode)
        .with_primary_file_name(&display_name)
        .with_include_file_handler(FsIncludeHandler::new(
            input,
            safe_mode,
            dependencies.clone(),
        ));

    for attribute in &common.attributes {
        parser = apply_attribute(parser, attribute);
    }

    let document = parser.parse(&source);
    let counts = reporter.report(&document, &display_name);
    let rendered = render::render(&document, options);

    Ok(Outcome {
        html: rendered.html,
        counts,
        dependencies: dependencies.snapshot(),
    })
}

/// Render one document and put it where the job says.
pub fn run(
    job: &Job,
    common: &CommonArgs,
    options: &Options,
    reporter: Reporter,
) -> Result<Outcome> {
    let outcome = render_file(&job.input, common, options, reporter)?;
    let html = &outcome.html;

    match &job.destination {
        Destination::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(html.as_bytes())
                .context("writing to standard output")?;
            stdout.flush().context("writing to standard output")?;
        }

        Destination::File(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating `{}`", parent.display()))?;
            }

            fs::write(path, html).with_context(|| format!("writing `{}`", path.display()))?;
        }
    }

    Ok(outcome)
}

/// Apply one `-a` argument, in any of the forms the Asciidoctor CLI accepts.
///
/// Command-line attributes are locked to the API: a document cannot override
/// what the caller asked for, which is the whole point of setting it here.
fn apply_attribute(parser: Parser, argument: &str) -> Parser {
    const CONTEXT: ModificationContext = ModificationContext::ApiOnly;

    // `!name` and `name!` both unset.
    if let Some(name) = argument.strip_prefix('!') {
        return parser.with_intrinsic_attribute_bool(name, false, CONTEXT);
    }

    match argument.split_once('=') {
        Some((name, value)) => match name.strip_suffix('!') {
            Some(name) => parser.with_intrinsic_attribute_bool(name, false, CONTEXT),
            None => parser.with_intrinsic_attribute(name, value, CONTEXT),
        },

        None => match argument.strip_suffix('!') {
            Some(name) => parser.with_intrinsic_attribute_bool(name, false, CONTEXT),
            None => parser.with_intrinsic_attribute_bool(argument, true, CONTEXT),
        },
    }
}

/// `foo.adoc` becomes `foo.html`, beside the original.
fn html_sibling(input: &Path) -> PathBuf {
    input.with_extension("html")
}

/// The bare `foo.html` name for an input, for use inside an output directory.
fn html_name(input: &Path) -> Result<PathBuf> {
    let stem = input
        .file_stem()
        .with_context(|| format!("`{}` has no file name", input.display()))?;

    Ok(PathBuf::from(stem).with_extension("html"))
}

/// Whether a path should be treated as a directory to render into.
fn is_directory_path(path: &Path) -> bool {
    // A path that already exists as a directory is unambiguous; otherwise a
    // trailing separator is the author saying "this is a directory".
    path.is_dir() || path.to_string_lossy().ends_with(std::path::MAIN_SEPARATOR)
}
