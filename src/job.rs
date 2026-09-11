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

/// What a job produces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// A rendered HTML page.
    Html,

    /// A PDF, typeset by Typst.
    Pdf,
}

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

    /// What to produce, which the output path's extension decides.
    pub format: Format,

    /// Where to put the result.
    pub destination: Destination,
}

/// What one render produced.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// What was rendered: HTML as bytes, or a PDF.
    pub bytes: Vec<u8>,

    /// Diagnostics reported for the document.
    pub counts: Counts,

    /// Every file the render read: the document and its includes.
    ///
    /// This is what `--watch` watches; an include that appears only after the
    /// document is edited is picked up on the next pass.
    pub dependencies: Vec<PathBuf>,
}

/// Work out what each input becomes and where it should go.
pub fn plan(args: &RenderArgs) -> Result<Vec<Job>> {
    let single = args.inputs.len() == 1;

    if args.writes_to_stdout() && !single {
        bail!(
            "`--output -` writes one document to standard output, but {} were given",
            args.inputs.len()
        );
    }

    // The extension asked for decides what is produced. `--pdf` is not a flag
    // because `-o guide.pdf` already says it, and two ways of saying one thing
    // is one way too many.
    let format = match &args.output {
        Some(output) => format_for(output),
        None => Format::Html,
    };

    let extension = match format {
        Format::Html => "html",
        Format::Pdf => "pdf",
    };

    let mut jobs = Vec::with_capacity(args.inputs.len());

    for input in &args.inputs {
        let destination = match &args.output {
            None => Destination::File(input.with_extension(extension)),

            Some(output) if output == Path::new("-") => Destination::Stdout,

            Some(output) => {
                // With several inputs the destination can only sensibly be a
                // directory; with one it may be either, so an existing
                // directory (or a trailing separator) decides.
                if !single || is_directory_path(output) {
                    fs::create_dir_all(output).with_context(|| {
                        format!("creating output directory `{}`", output.display())
                    })?;

                    Destination::File(output.join(output_name(input, extension)?))
                } else {
                    Destination::File(output.clone())
                }
            }
        };

        jobs.push(Job {
            input: input.clone(),
            format,
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
    render_as(input, Format::Html, common, options, reporter)
}

/// Parse and render one document in the format asked for.
pub fn render_as(
    input: &Path,
    format: Format,
    common: &CommonArgs,
    options: &Options,
    reporter: Reporter,
) -> Result<Outcome> {
    let source = read(input)?;
    let mut parsed = Parsed::new(input, common);
    let document = parsed.parser.parse(&source);
    let counts = reporter.report(&document, &parsed.display_name, input.parent());

    let bytes = match format {
        Format::Html => render::render(&document, options).html.into_bytes(),
        Format::Pdf => typeset(&document, input.parent().unwrap_or(Path::new(".")), options)?,
    };

    Ok(Outcome {
        bytes,
        counts,
        dependencies: parsed.dependencies.snapshot(),
    })
}

/// Parse one document and report its diagnostics, producing nothing.
///
/// This is the front half of [`render_as`], for a `check` that wants to know
/// what is wrong with a document without paying for — or leaving behind — a
/// rendering of it.
pub fn check_file(input: &Path, common: &CommonArgs, reporter: Reporter) -> Result<Counts> {
    let source = read(input)?;
    let mut parsed = Parsed::new(input, common);
    let document = parsed.parser.parse(&source);

    Ok(reporter.report(&document, &parsed.display_name, input.parent()))
}

/// Read a document's source, naming the file in the error if that fails.
fn read(input: &Path) -> Result<String> {
    fs::read_to_string(input).with_context(|| format!("reading `{}`", input.display()))
}

/// A parser set up for one document, and what it was set up with.
struct Parsed {
    /// The parser, configured from the command line and pointed at the
    /// document's own directory for includes.
    parser: Parser,

    /// The path shown in diagnostics.
    display_name: String,

    /// Every file the parse reads; the document itself is already in it.
    dependencies: Dependencies,
}

impl Parsed {
    /// Configure a parser for `input` the way every command does.
    fn new(input: &Path, common: &CommonArgs) -> Self {
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
            ))
            // A reference to an attribute that is not set is left in the text
            // as written, `{name}` and all, and Asciidoctor says nothing about
            // it by default. Here it is a warning, because the reader is the
            // wrong person to find it. The document can still say
            // `:attribute-missing: skip` if the braces are meant literally,
            // and so can the command line.
            .with_intrinsic_attribute("attribute-missing", "warn", ModificationContext::Anywhere);

        for attribute in &common.attributes {
            parser = apply_attribute(parser, attribute);
        }

        Self {
            parser,
            display_name,
            dependencies,
        }
    }
}

/// Typeset a document as a PDF.
#[cfg(feature = "pdf")]
fn typeset(
    document: &asciidoc_parser::Document<'_>,
    base: &Path,
    options: &Options,
) -> Result<Vec<u8>> {
    render::typst::pdf(document, base, options)
}

/// Refuse politely: no typesetter is compiled in.
#[cfg(not(feature = "pdf"))]
fn typeset(
    _document: &asciidoc_parser::Document<'_>,
    _base: &Path,
    _options: &Options,
) -> Result<Vec<u8>> {
    anyhow::bail!("this build cannot write PDFs: it was built without the `pdf` feature")
}

/// Render one document and put it where the job says.
pub fn run(
    job: &Job,
    common: &CommonArgs,
    options: &Options,
    reporter: Reporter,
) -> Result<Outcome> {
    let outcome = render_as(&job.input, job.format, common, options, reporter)?;
    let bytes = &outcome.bytes;

    match &job.destination {
        Destination::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout
                .write_all(bytes)
                .context("writing to standard output")?;
            stdout.flush().context("writing to standard output")?;
        }

        Destination::File(path) => {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating `{}`", parent.display()))?;
            }

            fs::write(path, bytes).with_context(|| format!("writing `{}`", path.display()))?;
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

/// What an output path asks to be produced.
///
/// Only a file named `.pdf` is a PDF; everything else, a directory included, is
/// the HTML this tool has always produced.
fn format_for(output: &Path) -> Format {
    match output.extension().and_then(|e| e.to_str()) {
        Some(extension) if extension.to_lowercase() == "pdf" => Format::Pdf,
        _ => Format::Html,
    }
}

/// The bare `foo.html` or `foo.pdf` name for an input, for use inside an
/// output directory.
fn output_name(input: &Path, extension: &str) -> Result<PathBuf> {
    let stem = input
        .file_stem()
        .with_context(|| format!("`{}` has no file name", input.display()))?;

    Ok(PathBuf::from(stem).with_extension(extension))
}

/// Whether a path should be treated as a directory to render into.
fn is_directory_path(path: &Path) -> bool {
    // A path that already exists as a directory is unambiguous; otherwise a
    // trailing separator is the author saying "this is a directory".
    path.is_dir() || path.to_string_lossy().ends_with(std::path::MAIN_SEPARATOR)
}
