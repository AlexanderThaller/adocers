//! The PDF back end: a document becomes Typst markup, and Typst makes the PDF.
//!
//! [Typst](https://typst.app) is a typesetting system written in Rust, so it
//! compiles in beside everything else and a PDF needs no LaTeX installation, no
//! headless browser and nothing fetched. Its fonts are the ones Typst itself
//! embeds, so the result does not depend on what the machine happens to have.
//!
//! This is a second back end rather than a variation on the first. The HTML one
//! walks the block tree emitting markup, and so does this, but Typst is not
//! HTML and almost nothing is shared between them — the exception being inline
//! content, which the parser hands over as HTML whatever is going to be done
//! with it. `inline` translates that. What the two back ends genuinely agree
//! on — section numbers, header metadata, admonition icons, mermaid diagrams —
//! lives in [`adocers_render_core`], which neither of them owns.
//!
//! What it covers is the shape of an ordinary document: headings, paragraphs,
//! lists, tables, listings, quotes, admonitions, images, diagrams, footnotes
//! and breaks. A block it has no rendering for becomes its own text rather than
//! a gap.
//!
//! Four things stop short of the page and are documented as such: an equation
//! is shown as its source, because Typst's mathematics syntax is neither
//! LaTeX's nor `AsciiMath`'s; a table cell written as AsciiDoc comes out empty;
//! a passthrough is dropped, being HTML; and a cross reference to anything but
//! a section keeps its words and loses its link, because Typst will not lay out
//! a document that points at a label it cannot find.

#![warn(clippy::print_stderr, clippy::print_stdout)]

#[cfg(feature = "math")]
mod ascii;
mod inline;
mod math;

use std::{
    fmt::Write as _,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Context as _,
    Result,
};
use asciidoc_parser::{
    Document,
    blocks::{
        Block,
        BreakType,
        CompoundDelimitedContext,
        FindBlocks,
        IsBlock,
        ListType,
        QuoteType,
        SectionType,
    },
    document::TocMode,
};

use adocers_render_core::numbering::Numbering;

use crate::inline::{
    string,
    typst as inline_markup,
};

/// What a PDF shows beyond the document's own content.
///
/// These are the switches the HTML back end reads too, minus the ones that only
/// a web page has: a PDF has no stylesheet to embed and is never a fragment of
/// something larger.
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "these mirror command line flags, and one field per flag is what reads clearly"
)]
pub struct Options {
    /// Whether an admonition is marked with an icon rather than its label, and
    /// a callout with a circled number rather than the parser's `(1)`.
    pub icons: bool,

    /// Whether a source block is syntax highlighted, by Typst's own
    /// highlighter.
    pub highlight: bool,

    /// Whether a mermaid block is drawn as a diagram, or left as the listing
    /// block it was written as.
    pub mermaid: bool,

    /// Whether an equation is typeset, or shown as the notation it was written
    /// in.
    pub math: bool,
}

/// A typeset PDF, and anything the caller ought to be told about how it was
/// made.
#[derive(Clone, Debug)]
pub struct Pdf {
    /// The PDF itself.
    pub bytes: Vec<u8>,

    /// Why the document was not typeset the way it was asked for, if it was
    /// not.
    ///
    /// Set when an equation could not be typeset and every equation was shown
    /// as its source instead — a page of equations as their source being a far
    /// better answer than no page at all. A caller with somewhere to put a
    /// diagnostic should say so; one without can drop it.
    pub fallback: Option<String>,
}

/// Render `document` as a PDF.
///
/// `base` is the directory the document was read from, which is where the
/// images it names are looked for.
pub fn pdf(document: &Document<'_>, base: &Path, options: &Options) -> Result<Pdf> {
    let (source, images) = markup(document, base, options);

    let error = match compile(&source, &images) {
        Ok(bytes) => {
            return Ok(Pdf {
                bytes,
                fallback: None,
            });
        }

        Err(error) => error,
    };

    // Converted mathematics is the one part of this that can fail on content
    // rather than on a mistake here: `mitex` covers a great deal of LaTeX but
    // not all of it, and a command it renders as a handler this does not define
    // stops the whole document. A page of equations shown as their source is a
    // far better answer than no page at all.
    if !options.math {
        return Err(error);
    }

    let plain = Options {
        math: false,
        ..*options
    };

    let (source, images) = markup(document, base, &plain);

    match compile(&source, &images) {
        Ok(bytes) => Ok(Pdf {
            bytes,
            fallback: Some(format!(
                "an equation could not be typeset, so every equation is shown as its source \
                 ({error:#})"
            )),
        }),

        // The mathematics was not the trouble; report what actually went wrong.
        Err(_) => Err(error),
    }
}

/// The Typst markup for a document, and the image files it refers to.
pub fn markup(
    document: &Document<'_>,
    base: &Path,
    options: &Options,
) -> (String, Vec<(String, Vec<u8>)>) {
    let mut out = Preamble::new(document, *options).to_string();

    let mut emitter = Emitter {
        out: String::new(),
        base: base.to_path_buf(),
        images: Vec::new(),
        toc: Toc::of(document),
        options: *options,
        stem: attribute(document, "stem"),
        figure: attribute(document, "figure-caption"),
        figures: 0,
        numbering: Numbering::of(document),
    };

    // An outline placed above the content goes between the title block and the
    // first thing the author wrote, which is where `:toc:` puts it on the page.
    if emitter.toc.above() {
        emitter.outline(emitter.toc.levels, &emitter.toc.title.clone());
    }

    emitter.blocks(document.child_blocks());

    if emitter.toc.mode == Some(TocMode::Bottom) {
        emitter.outline(emitter.toc.levels, &emitter.toc.title.clone());
    }

    out.push_str(&prune_links(&emitter.out));

    (out, emitter.images)
}

/// Compile Typst markup to a PDF.
fn compile(source: &str, images: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    use typst_as_lib::{
        TypstEngine,
        typst_kit_options::TypstKitFontOptions,
    };

    let engine = TypstEngine::builder()
        .main_file(source.to_string())
        // Typst's own fonts and no others: scanning the machine's font
        // directories costs half a second and makes the output depend on what
        // happens to be installed.
        .search_fonts_with(TypstKitFontOptions::default().include_system_fonts(false))
        // Typst has no file system of its own here, so every picture the
        // document names is handed over as bytes.
        .with_static_file_resolver(
            images
                .iter()
                .map(|(path, bytes)| (path.as_str(), bytes.as_slice())),
        )
        .build();

    let document = engine
        .compile()
        .output
        .map_err(|error| anyhow::anyhow!("{error}"))
        .context("laying the document out")?;

    typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
        .map_err(|errors| anyhow::anyhow!(describe(&errors)))
        .context("writing the PDF")
}

/// Turn Typst's diagnostics into one message.
fn describe(errors: &[typst::diag::SourceDiagnostic]) -> String {
    errors
        .iter()
        .map(|error| error.message.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// Scale a picture down to the column when it is wider, and leave it alone
/// when it is not.
///
/// An AsciiDoc image is measured in pixels and a page is measured in
/// millimetres, so a figure drawn for a browser is usually wider than the text
/// it sits in. Typst does not fit a picture to its container by itself: it
/// draws it at its natural size and lets it run off the page.
const FITTED: &str = "#let fitted(body) = layout(size => {\n\x20 let natural = \
                      measure(body).width\n\x20 let factor = size.width / natural * 100%\n\x20 if \
                      natural > size.width {\n\x20   scale(body, x: factor, y: factor, reflow: \
                      true, origin: top + left)\n\x20 } else { body }\n})";

/// Mark a callout list's item the way the listing above it is marked.
///
/// The same circled characters, so an item and the line it annotates read as
/// the same mark; above twenty there is none, and the plain number stands.
const CALLOUT: &str = "#let adoccallout(n) = if n <= 10 { str.from-unicode(0x2775 + n) } else if \
                       n <= 20 { str.from-unicode(0x24E0 + n) } else { numbering(\"(1)\", n) \
                       }\n#show regex(\"[\\u{2776}-\\u{277F}\\u{24EB}-\\u{24F4}]\"): it => \
                       text(fill: rgb(\"#1565a8\"), size: 1.35em, baseline: 0.12em)[#it]";

/// The `footnote:[]` definitions, and the helper that places one.
///
/// A reference is rendered by the parser as its mark alone, with the text left
/// in the document's catalogue, so the texts are written out once here and each
/// reference calls for the one it wants. Typst then puts it at the foot of
/// whichever page the reference landed on and numbers it itself.
fn footnotes(document: &Document<'_>, options: Options) -> String {
    let texts: Vec<String> = document
        .catalog()
        .footnotes()
        .iter()
        .map(|footnote| format!("[{}]", prose(&footnote.text, options.math)))
        .collect();

    // One item needs the trailing comma that tells an array from a parenthesis,
    // and none needs no comma at all.
    let array = match texts.len() {
        0 => String::new(),
        1 => format!("{},", texts[0]),
        _ => texts.join(", "),
    };

    // A reference with no definition would be a lookup past the end, which
    // Typst reports rather than ignores, so it is guarded rather than trusted.
    format!(
        "#let adocfootnotes = ({array})\n#let adocfootnote(n) = if n <= adocfootnotes.len() {{ \
         footnote(adocfootnotes.at(n - 1)) }}"
    )
}

/// What the document says about itself, as the properties a PDF carries.
///
/// A reader's viewer shows these in its document-properties panel, and a search
/// index reads them, so the header's facts are worth writing down there as well
/// as on the first page.
fn properties(document: &Document<'_>, title: &str) -> String {
    let mut out = format!("title: {}", string(title));

    let authors: Vec<String> = document
        .authors()
        .iter()
        .map(|author| string(author.name()))
        .collect();

    if !authors.is_empty() {
        let _ = write!(out, ", author: ({},)", authors.join(", "));
    }

    if let Some(description) = attribute(document, "description") {
        let _ = write!(out, ", description: {}", string(&description));
    }

    let keywords: Vec<String> = ["keywords", "page-tags"]
        .iter()
        .filter_map(|name| attribute(document, name))
        .flat_map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|word| !word.is_empty())
                .map(string)
                .collect::<Vec<_>>()
        })
        .collect();

    if !keywords.is_empty() {
        let _ = write!(out, ", keywords: ({},)", keywords.join(", "));
    }

    // A date Typst cannot read is left out rather than guessed at: `auto` would
    // silently stamp the PDF with the day it was built.
    match attribute(document, "revdate").as_deref().and_then(date) {
        Some(date) => {
            let _ = write!(out, ", date: {date}");
        }

        None => out.push_str(", date: none"),
    }

    out
}

/// A `YYYY-MM-DD` date, as a Typst `datetime`.
fn date(value: &str) -> Option<String> {
    let mut parts = value.trim().splitn(3, '-');

    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.trim().parse().ok()?;

    (1..=12).contains(&month).then_some(())?;
    (1..=31).contains(&day).then_some(())?;

    Some(format!(
        "datetime(year: {year}, month: {month}, day: {day})"
    ))
}

/// One of the document header's attributes, if it was set to something.
fn attribute(document: &Document<'_>, name: &str) -> Option<String> {
    match document.attribute_value(name) {
        asciidoc_parser::document::InterpretedValue::Value(value) if !value.is_empty() => {
            Some(value.clone())
        }

        _ => None,
    }
}

/// The labelled lines under the title: who wrote it, which revision it is, and
/// whatever else the header says about the document.
///
/// The same facts the page shows, read the same way — including Antora's
/// `page-` namespace — so the two headers cannot drift apart.
fn details(document: &Document<'_>) -> String {
    let mut out = String::new();

    let authors: Vec<String> = document
        .authors()
        .iter()
        .map(|author| match author.email() {
            // The angle brackets are the convention a reader knows from a
            // commit or a mail header, escaped because Typst reads a bare pair
            // of them as a label.
            Some(email) => format!(
                "{} #link(\"mailto:{email}\")[\\<{}\\>]",
                inline_markup(author.name()),
                inline_markup(email)
            ),

            None => inline_markup(author.name()),
        })
        .collect();

    if !authors.is_empty() {
        let label = if authors.len() == 1 {
            "Author"
        } else {
            "Authors"
        };

        let _ = writeln!(out, "*{label}:* {}\\", authors.join(", "));
    }

    for (name, label) in [("revnumber", "Version"), ("revdate", "Date")] {
        if let Some(value) = attribute(document, name) {
            let _ = writeln!(out, "*{label}:* {}\\", inline_markup(&value));
        }
    }

    for attribute in document.header().attributes() {
        let name = attribute.name().data();

        let Some(shown) = adocers_render_core::metadata::displayed_as(name) else {
            continue;
        };

        let asciidoc_parser::document::InterpretedValue::Value(value) = attribute.value() else {
            continue;
        };

        if value.is_empty() {
            continue;
        }

        let _ = writeln!(
            out,
            "*{}:* {}\\",
            adocers_render_core::metadata::label_for(shown),
            inline_markup(value)
        );
    }

    let mut out = out.trim_end().trim_end_matches('\\').to_string();

    // The remark is a sentence about the revision rather than another value
    // belonging to it, so it is set apart below the labels rather than given
    // one of its own — italic, as the page sets it, and not in the same face as
    // the labels or it reads as a label with the value left off.
    if let Some(remark) = attribute(document, "revremark") {
        let _ = write!(
            out,
            "\n#v(0.4em)\n#text(style: \"italic\")[{}]",
            inline_markup(&remark)
        );
    }

    out
}

/// Inline content, with any equation in it typeset rather than written out.
///
/// The parser hands an inline `stem:[…]` to the page as the delimiters
/// `MathJax` used to look for — `\(…\)` for LaTeX, `\$…\$` for `AsciiMath`.
/// Nothing looks for them here either, so a line would otherwise carry
/// `\(x^2\)` as written.
///
/// LaTeX is typeset; `AsciiMath` is shown as its source, without the
/// delimiters, for the reason [`math`] gives.
fn prose(html: &str, math: bool) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some((at, open, close, latex)) = equation(rest) {
        out.push_str(&inline_markup(&rest[..at]));

        let body = &rest[at + open.len()..];

        // An opening delimiter with no closing one is not an equation.
        let Some(end) = body.find(close) else {
            out.push_str(&inline_markup(&rest[at..at + open.len()]));
            rest = &rest[at + open.len()..];

            continue;
        };

        let source = inline::unescape(&body[..end]);

        match typeset(&source, math, latex) {
            Some(converted) => out.push_str(&converted),
            None => {
                let _ = write!(out, "#raw({})", string(source.trim()));
            }
        }

        rest = &body[end + close.len()..];
    }

    out.push_str(&inline_markup(rest));
    out
}

/// The next delimited equation, and whether it is written in LaTeX.
fn equation(html: &str) -> Option<(usize, &'static str, &'static str, bool)> {
    let latex = html.find("\\(").map(|at| (at, "\\(", "\\)", true));
    let ascii = html.find("\\$").map(|at| (at, "\\$", "\\$", false));

    match (latex, ascii) {
        (Some(l), Some(a)) if a.0 < l.0 => Some(a),
        (Some(l), _) => Some(l),
        (None, found) => found,
    }
}

/// One equation, as the Typst mathematics of a line of text.
fn typeset(source: &str, math: bool, latex: bool) -> Option<String> {
    math.then(|| converted(source, latex))
        .flatten()
        .map(|converted| format!("${converted}$"))
}

/// One equation as Typst mathematics, whichever notation it is written in.
fn converted(source: &str, latex: bool) -> Option<String> {
    if latex {
        return math::typst(source);
    }

    asciimath(source)
}

/// `AsciiMath`, which is converted by the same crate the page's `MathML` comes
/// from and so needs the feature that brings it.
#[cfg(feature = "math")]
fn asciimath(source: &str) -> Option<String> {
    ascii::typst(source)
}

/// Never converts: no parser for it is compiled in.
#[cfg(not(feature = "math"))]
fn asciimath(_source: &str) -> Option<String> {
    None
}

/// The page setup and title block that every rendered document opens with.
struct Preamble(String);

impl Preamble {
    fn new(document: &Document<'_>, options: Options) -> Self {
        let mut out = String::new();

        let title = document
            .doctitle_sanitized()
            .unwrap_or_else(|| "Untitled".to_string());

        let _ = writeln!(
            out,
            "#set document({})\n#set page(paper: \"a4\", margin: 2.2cm, numbering: \"1\")\n#set \
             text(size: 10.5pt)\n#set par(justify: true, leading: 0.62em)\n#show heading: it => \
             block(above: 1.4em, below: 0.7em, it)\n#show link: it => text(fill: \
             rgb(\"#1565a8\"), it)\n#show raw.where(block: true): it => block(\n\x20 width: 100%, \
             fill: rgb(\"#f5f6f8\"), inset: 8pt, radius: 3pt, it,\n)\n{}\n{CALLOUT}\n{}\n{}\n",
            properties(document, &title),
            FITTED,
            footnotes(document, options),
            math::PRELUDE
        );

        if document.doctitle().is_some() {
            // A title split at a colon is set in two, the way a title page is:
            // the title on its own line and the subtitle beneath it, smaller
            // and lighter in weight but in the same ink. On the page the two
            // share a line, because a page has no title page to give them.
            let (heading, subtitle) = match (document.header().main_title(), document.subtitle()) {
                (Some(main), Some(subtitle)) => {
                    (inline_markup(main), Some(inline_markup(subtitle)))
                }

                _ => (inline_markup(&title), None),
            };

            let _ = writeln!(
                out,
                "#align(center)[#text(size: 20pt, weight: \"bold\")[{heading}]]"
            );

            if let Some(subtitle) = subtitle {
                let _ = writeln!(
                    out,
                    "#v(-0.6em)\n#align(center)[#text(size: 14pt)[{subtitle}]]"
                );
            }

            let details = details(document);

            if !details.is_empty() {
                // The same labelled lines the page carries, set small and grey
                // so the document's own first words are what the eye lands on.
                let _ = writeln!(
                    out,
                    "#v(0.4em)\n#align(center)[#block(width: 80%)[#set text(size: 9pt, fill: \
                     rgb(\"#656d77\"))\n#set align(center)\n{details}]]"
                );
            }

            out.push('\n');
        }

        Self(out)
    }
}

impl std::fmt::Display for Preamble {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Walks the block tree, appending Typst markup as it goes.
struct Emitter {
    /// Markup accumulated so far.
    out: String,

    /// The directory the document came from, which images are relative to.
    base: PathBuf,

    /// The images referred to, with their contents, for the compiler.
    images: Vec<(String, Vec<u8>)>,

    /// How the document asked for its outline, and whether one has been placed.
    toc: Toc,

    /// What the command line asked for. The flags that say what a document
    /// carries — icons, highlighting, diagrams — mean the same here as they do
    /// on a page; the ones about the page itself mean nothing.
    options: Options,

    /// What `:stem:` was set to, which says which notation a plain `[stem]`
    /// block is written in.
    stem: Option<String>,

    /// What a figure's caption is labelled with, and how many have been
    /// captioned so far.
    figure: Option<String>,
    figures: usize,

    /// What each section shows in front of its title, so a PDF and a page
    /// number the same document the same way.
    numbering: Numbering,
}

/// The table of contents a document asked for.
#[derive(Clone, Debug)]
struct Toc {
    /// Where it goes, or `None` when the document wants none.
    mode: Option<TocMode>,

    /// The heading above it, empty for no heading at all.
    title: String,

    /// How many levels of section it lists.
    levels: usize,

    /// Whether one has been placed, so a second `toc::[]` is ignored the way
    /// Asciidoctor ignores it.
    placed: bool,
}

impl Toc {
    /// What a document asked for, read the way the page reads it.
    fn of(document: &Document<'_>) -> Self {
        let mode = document.toc_mode();

        Self {
            mode: (mode != TocMode::Disabled).then_some(mode),
            title: document.toc_title().to_string(),
            levels: document.toc_levels(),
            placed: false,
        }
    }

    /// Whether the outline goes above the document's content.
    ///
    /// The three side placements are a page's idea — a column beside the text —
    /// and a page of paper has no room for one, so all of them become an
    /// outline at the front.
    fn above(&self) -> bool {
        matches!(
            self.mode,
            Some(TocMode::Auto | TocMode::Left | TocMode::Right | TocMode::Top)
        )
    }
}

impl Emitter {
    /// Inline content, with its equations typeset and its pictures placed.
    fn prose(&mut self, html: &str) -> String {
        let converted = prose(html, self.options.math);

        self.pictures(&converted)
    }

    /// Put the pictures a line of text carries into the document.
    ///
    /// `inline` leaves a marker where it found one, because placing a picture
    /// needs the directory the document was read from and a place to keep the
    /// bytes, and neither reaches a function that only knows about markup.
    ///
    /// A picture that cannot be read — a URL, a path that does not resolve —
    /// keeps the words the author gave it instead, which is what the marker
    /// carries along with the name.
    fn pictures(&mut self, markup: &str) -> String {
        let mut out = String::with_capacity(markup.len());
        let mut rest = markup;

        while let Some(at) = rest.find(inline::PICTURE) {
            out.push_str(&rest[..at]);
            rest = &rest[at + inline::PICTURE.len()..];

            let Some((marker, tail)) = rest.split_once(inline::PICTURE) else {
                out.push_str(inline::PICTURE);
                break;
            };

            rest = tail;

            // The marker is the source, the height it asked for and the words
            // to fall back on, in that order.
            let mut parts = marker.splitn(3, inline::FIELD);
            let (Some(target), Some(height), Some(words)) =
                (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };

            match self.picture(target, height) {
                Some(placed) => out.push_str(&placed),
                None => out.push_str(words),
            }
        }

        out.push_str(rest);
        out
    }

    /// One picture in a line of text, if it can be read from disk.
    fn picture(&mut self, target: &str, height: &str) -> Option<String> {
        let name = format!("/{target}");

        if !self.images.iter().any(|(known, _)| *known == name) {
            let bytes = std::fs::read(self.base.join(target)).ok()?;

            self.images.push((name.clone(), bytes));
        }

        // A picture in a line of text is an icon or a mark, so it is set to the
        // height of the line unless the author asked for another — theirs is in
        // pixels, which is three quarters of a point.
        let height = height.parse::<f64>().map_or_else(
            |_| "1em".to_string(),
            |pixels| format!("{}pt", pixels * 0.75),
        );

        Some(format!("#box(image({}, height: {height}))", string(&name)))
    }

    /// Write the outline, once.
    ///
    /// Typst builds it from the headings themselves and makes every entry a
    /// link, so this is the whole of it.
    fn outline(&mut self, levels: usize, title: &str) {
        if self.toc.placed {
            return;
        }

        self.toc.placed = true;

        let title = if title.is_empty() {
            "none".to_string()
        } else {
            format!("[{}]", inline_markup(title))
        };

        let _ = writeln!(
            self.out,
            "#outline(title: {title}, depth: {levels})\n#v(0.5em)\n"
        );
    }

    /// Render a sequence of blocks.
    fn blocks<'src>(&mut self, blocks: impl Iterator<Item = &'src Block<'src>>) {
        for block in blocks {
            self.block(block);
        }
    }

    /// Render one block and everything beneath it.
    fn block(&mut self, block: &Block<'_>) {
        // A section's title is its heading, and a picture's is its caption and
        // is set beneath it; every other kind carries its own above it.
        if !matches!(block, Block::Section(_) | Block::Media(_)) && !is_diagram(block) {
            self.title(block);
        }

        match block {
            Block::Section(section) => {
                let level = section.level().clamp(1, 6);

                // The number, if the document numbers its sections, comes from
                // the same place the page's does — so an appendix is captioned
                // and the two agree on every number.
                let title = format!(
                    "{}{}",
                    self.prose(&self.numbering.prefix(section)),
                    self.prose(section.section_title())
                );

                // The label goes after the heading, not before it: Typst
                // attaches one to the element in front of it, so a label
                // written above a heading belongs to whatever came before —
                // which is where a reference to the section would have landed.
                let label = section.id().map(anchor).unwrap_or_default();

                // A discrete heading is styled like one but is not a section,
                // so it does not belong in the outline.
                if section.section_type() == SectionType::Discrete {
                    let _ = writeln!(
                        self.out,
                        "#heading(level: {level}, outlined: false)[{title}]{label}\n"
                    );
                } else {
                    let _ = writeln!(self.out, "{} {title}{label}\n", "=".repeat(level));
                }

                self.blocks(section.child_blocks());
            }

            Block::Simple(simple) => {
                let text = self.prose(simple.content().rendered_html());

                if !text.trim().is_empty() {
                    let _ = writeln!(self.out, "{text}\n");
                }
            }

            Block::List(list) => self.list(list),
            Block::Table(table) => self.table(table),
            Block::Preamble(preamble) => {
                self.blocks(preamble.child_blocks());

                if self.toc.mode == Some(TocMode::Preamble) {
                    self.outline(self.toc.levels, &self.toc.title.clone());
                }
            }

            // `toc::[]` places the outline where it stands, and may carry a
            // title and a depth of its own.
            Block::Toc(toc) => {
                if self.toc.mode == Some(TocMode::Macro) {
                    let attrlist = toc.macro_attrlist();

                    let named = |name: &str| {
                        attrlist
                            .named_attribute(name)
                            .map(asciidoc_parser::attributes::ElementAttribute::value)
                            .filter(|value| !value.is_empty())
                    };

                    let title =
                        named("title").map_or_else(|| self.toc.title.clone(), str::to_string);
                    let levels = named("levels")
                        .and_then(|levels| levels.parse().ok())
                        .unwrap_or(self.toc.levels);

                    self.outline(levels, &title);
                }
            }
            Block::Admonition(admonition) => self.admonition(admonition),
            Block::Quote(quote) => self.quote(quote),
            Block::Media(media) => self.media(media),
            Block::RawDelimited(raw) => self.raw(raw),
            Block::CompoundDelimited(compound) => self.compound(compound),
            // A page break is only a hint to a print stylesheet on a page; here
            // it is the one place it can be taken literally.
            Block::Break(r#break) => match r#break.type_() {
                BreakType::Thematic => self.out.push_str("#line(length: 100%)\n\n"),
                BreakType::Page => self.out.push_str("#pagebreak()\n\n"),
            },

            // A list item is rendered by its list; an attribute entry has
            // already been applied by the parser.
            _ => {}
        }
    }

    /// The caption for the next figure, and the number that goes in it.
    ///
    /// A picture and a drawn diagram share one sequence, which the parser
    /// cannot count: to it a diagram is a listing. The rules are the ones it
    /// follows for a picture — only a titled figure is numbered, and
    /// `:figure-caption!:` leaves the title to stand on its own — so both
    /// outputs number the same figures the same way.
    fn figure_caption(&mut self, titled: bool) -> String {
        if !titled {
            return String::new();
        }

        let Some(label) = self.figure.clone().filter(|label| !label.is_empty()) else {
            return String::new();
        };

        self.figures += 1;

        format!("{label} {}. ", self.figures)
    }

    /// The `.A title` line a block can carry, with the caption the document
    /// numbers it with.
    fn title(&mut self, block: &Block<'_>) {
        let Some(title) = block.title() else {
            return;
        };

        let caption = block.caption().unwrap_or_default();

        let caption = self.prose(caption);
        let title = self.prose(title);

        let _ = writeln!(
            self.out,
            "#block(above: 1em, below: 0.5em)[#text(weight: \"bold\", size: \
             9.5pt)[{caption}{title}]]\n"
        );
    }

    /// A verbatim block: a listing, a literal, or something with no rendering
    /// of its own.
    fn raw(&mut self, raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>) {
        // Taken as text rather than unescaped, because a listing's callouts
        // reach here as `<b class="conum">` and would otherwise be shown.
        let content = inline::text(&self.marks(raw.content().rendered_html()));

        if self.diagram(raw, &content) {
            return;
        }

        match raw.raw_context().as_ref() {
            // A comment reaches the page as nothing at all, and so does a
            // passthrough, which is HTML and has no meaning here.
            "comment" | "pass" => {}

            "stem" => self.equation(raw, content.trim()),

            // A listing names its language, which Typst highlights with the
            // syntaxes it carries; a literal block names none and is set plain.
            _ => {
                let language = (raw.raw_context().as_ref() == "listing")
                    .then(|| source_language(raw))
                    .flatten();

                self.verbatim_as(&content, language.as_deref());
            }
        }
    }

    /// A displayed equation, typeset if it can be and shown as written if not.
    ///
    /// Only LaTeX is translated; see [`math`] for why `AsciiMath` is not, and
    /// `--no-math` translates neither.
    fn equation(&mut self, raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>, source: &str) {
        if let Some(typeset) = self.mathematics(raw, source) {
            let _ = writeln!(self.out, "$ {typeset} $\n");

            return;
        }

        let _ = writeln!(self.out, "#align(center)[#raw({})]\n", string(source));
    }

    /// One equation as Typst mathematics, if this run converts them.
    fn mathematics(
        &self,
        raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>,
        source: &str,
    ) -> Option<String> {
        self.options
            .math
            .then(|| converted(source, self.is_latex(raw)))
            .flatten()
    }

    /// Whether an equation is written in LaTeX.
    ///
    /// `[latexmath]` says so outright; a plain `[stem]` takes whatever `:stem:`
    /// was set to, which is `AsciiMath` unless it says otherwise. Read the way
    /// the page reads it, so one block cannot be two notations.
    fn is_latex(&self, raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>) -> bool {
        match raw.declared_style() {
            Some("latexmath") => true,
            Some("asciimath") => false,
            _ => self.stem.as_deref() == Some("latexmath"),
        }
    }

    /// Draw a mermaid block as a diagram, saying whether it was one.
    ///
    /// `merman` produces SVG, which Typst places as a vector image, so a
    /// diagram is drawn in the PDF at whatever resolution it is read or printed
    /// at. A diagram `merman` declines falls through to being shown as source,
    /// which is what it looked like before this could draw at all.
    #[cfg(feature = "mermaid")]
    fn diagram(
        &mut self,
        raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>,
        content: &str,
    ) -> bool {
        if !is_mermaid(raw) {
            return false;
        }

        let Some(svg) = adocers_render_core::mermaid::printable(content) else {
            return false;
        };

        let name = format!("/diagram-{}.svg", self.images.len());
        self.images.push((name.clone(), svg.into_bytes()));

        // A diagram is drawn to the size it needs and no other.
        self.figure(&name, raw, "");

        true
    }

    /// Never draws: `merman` is not compiled in.
    #[cfg(not(feature = "mermaid"))]
    #[expect(
        clippy::unused_self,
        reason = "the other half of this pair collects the drawing, and both are called the same \
                  way"
    )]
    fn diagram(
        &mut self,
        _raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>,
        _content: &str,
    ) -> bool {
        false
    }

    /// A block of text set exactly as written.
    fn verbatim(&mut self, content: &str) {
        self.verbatim_as(content, None);
    }

    /// The same, highlighted as `language` when one is named.
    ///
    /// Typst carries syntect and its syntaxes, so highlighting a listing costs
    /// nothing but naming the language. The colours are therefore Typst's
    /// rather than the tree-sitter ones the page uses — the same code, read by
    /// a different highlighter — and a language neither knows is simply set
    /// plain.
    fn verbatim_as(&mut self, content: &str, language: Option<&str>) {
        let language = match language {
            Some(language) => format!(", lang: {}", string(language)),
            None => String::new(),
        };

        let _ = writeln!(
            self.out,
            "#raw({}, block: true{language})\n",
            string(content)
        );
    }

    /// `====` example, `****` sidebar and `--` open blocks.
    fn compound(&mut self, compound: &asciidoc_parser::blocks::CompoundDelimitedBlock<'_>) {
        // An abstract is the summary of a document rather than a block of its
        // own, and is set apart the way a quotation is — which is also what the
        // page does with it.
        let abstracted = compound.declared_style() == Some("abstract");

        let framed = !abstracted
            && matches!(
                compound.context_kind(),
                CompoundDelimitedContext::Example | CompoundDelimitedContext::Sidebar
            );

        if framed {
            self.out.push_str(
                "#block(width: 100%, fill: rgb(\"#f2f4f7\"), inset: 10pt, radius: 3pt)[\n",
            );
        }

        if abstracted {
            self.out
                .push_str("#block(inset: (left: 12pt), stroke: (left: 2pt + rgb(\"#dcdfe4\")))[\n");
        }

        self.blocks(compound.child_blocks());

        if framed || abstracted {
            self.out.push_str("]\n\n");
        }
    }

    /// A note, tip, warning and so on, set beside its icon.
    ///
    /// Laid out the way the page lays it out: the mark in a column of its own,
    /// and the text beside it behind a rule.
    fn admonition(&mut self, admonition: &asciidoc_parser::blocks::AdmonitionBlock<'_>) {
        let name = admonition.name().to_string();
        let label = admonition.label().to_string().to_uppercase();
        let colour = admonition_colour(&name);

        let mark = match self.icon(&name, &label) {
            Some(path) => format!("#image({}, width: 20pt)", string(&path)),

            // A variant with no icon of its own, or a build asked for none,
            // falls back to the label rather than to an empty column.
            None => format!("#text(weight: \"bold\", size: 8pt, fill: rgb(\"{colour}\"))[{label}]"),
        };

        let _ = write!(
            self.out,
            "#grid(\n  columns: (34pt, 1fr),\n  align: (center + horizon, left + horizon),\n  \
             inset: (x, y) => if x == 1 {{ (left: 11pt, y: 2pt) }} else {{ (right: 10pt) }},\n  \
             stroke: (x, y) => if x == 1 {{ (left: 1pt + rgb(\"#dcdfe4\")) }} else {{ none }},\n  \
             [{mark}],\n  [\n"
        );

        // The rule is the grid's own rather than a border on the text: a line
        // drawn around the content is as short as the content, and beside a
        // one-line note that is shorter than the mark next to it.
        //
        // The paragraph form — `TIP: text` — carries its text directly and has
        // no child blocks at all, so asking only for the children loses it.
        match admonition.content() {
            Some(content) => {
                let converted = self.prose(content.rendered_html());
                self.out.push_str(&converted);
                self.out.push_str("\n\n");
            }

            None => self.blocks(admonition.child_blocks()),
        }

        self.out.push_str("],\n)\n\n");
    }

    /// Draw a listing's callout markers, if this run draws marks at all.
    ///
    /// A circled number is a character here rather than something drawn, which
    /// means it sits in the line of code exactly where the marker was without
    /// anything being laid over the text. Above twenty there is no such
    /// character, and the `(21)` the parser wrote stands.
    fn marks(&self, html: &str) -> String {
        if !self.options.icons {
            return html.to_string();
        }

        let mut out = String::with_capacity(html.len());
        let mut rest = html;

        while let Some(at) = rest.find(CONUM) {
            out.push_str(&rest[..at]);
            rest = &rest[at + CONUM.len()..];

            let number: String = rest.chars().take_while(char::is_ascii_digit).collect();

            let Some(mark) = number.parse().ok().and_then(circled) else {
                out.push_str(CONUM);
                continue;
            };

            let Some(end) = rest.find("</b>") else {
                out.push_str(CONUM);
                continue;
            };

            out.push(mark);
            rest = &rest[end + "</b>".len()..];
        }

        out.push_str(rest);
        out
    }

    /// The icon for an admonition, as a file the compiler can place.
    ///
    /// The page's own SVG, with its colour written in: it is drawn in
    /// `currentColor` so that one rule themes it and it follows the reader's
    /// colour scheme, and a PDF has neither a rule nor a reader.
    fn icon(&mut self, name: &str, label: &str) -> Option<String> {
        if !self.options.icons {
            return None;
        }

        let path = format!("/icon-{name}.svg");

        if !self.images.iter().any(|(known, _)| *known == path) {
            let svg = adocers_render_core::icons::admonition(name, label)?
                .replace("currentColor", admonition_colour(name));

            self.images.push((path.clone(), svg.into_bytes()));
        }

        Some(path)
    }

    /// A quotation, with its attribution below.
    fn quote(&mut self, quote: &asciidoc_parser::blocks::QuoteBlock<'_>) {
        self.out
            .push_str("#block(inset: (left: 12pt), stroke: (left: 2pt + rgb(\"#dcdfe4\")))[\n");

        match quote.content() {
            // A verse is written in lines and has to keep them; ordinary quoted
            // prose flows like any other paragraph.
            Some(content) if quote.type_() == QuoteType::Verse => {
                self.verbatim(&inline::text(content.rendered_html()));
            }

            Some(content) => {
                let converted = self.prose(content.rendered_html());
                self.out.push_str(&converted);
                self.out.push_str("\n\n");
            }

            None => self.blocks(quote.child_blocks()),
        }

        if let Some(attribution) = quote.attribution() {
            let attribution = self.prose(attribution);

            let _ = writeln!(
                self.out,
                "#text(size: 9pt, fill: rgb(\"#656d77\"))[— {attribution}]"
            );
        }

        self.out.push_str("]\n\n");
    }

    /// An image, or a video and audio reference that a page cannot play.
    fn media(&mut self, media: &asciidoc_parser::blocks::MediaBlock<'_>) {
        let target = media.resolved_target();

        // A picture has to be readable from disk to be placed. One that is not
        // — a URL, or a video, or a path that does not resolve — is named
        // instead, so the reader knows what was meant.
        let Ok(bytes) = std::fs::read(self.base.join(target)) else {
            let _ = writeln!(
                self.out,
                "#text(style: \"italic\", fill: rgb(\"#656d77\"))[[{}]]\n",
                inline_markup(target)
            );

            return;
        };

        self.images.push((format!("/{target}"), bytes));

        // The size the author asked for, in the pixels AsciiDoc measures in.
        // A picture that says nothing is placed at its own size, and either way
        // one wider than the column is brought down to fit.
        let width = attrlist_size(media, "width", 2);
        let height = attrlist_size(media, "height", 3);

        self.figure(&format!("/{target}"), media, &size(width, height));
    }

    /// A picture, with its caption beneath it and held on the same page.
    fn figure<'src>(&mut self, name: &str, block: &impl IsBlock<'src>, size: &str) {
        let caption = match block.title() {
            Some(title) => {
                let label = self.figure_caption(block.title().is_some());

                format!(
                    ", caption: [{}{}], numbering: none",
                    self.prose(&label),
                    self.prose(title)
                )
            }

            None => String::new(),
        };

        let _ = writeln!(
            self.out,
            "#block(breakable: false, width: 100%)[#figure(fitted(image({}{size})){caption})]\n",
            string(name)
        );
    }

    /// Any of the list kinds.
    fn list(&mut self, list: &asciidoc_parser::blocks::ListBlock<'_>) {
        let items: Vec<_> = list
            .child_blocks()
            .filter_map(|block| match block {
                Block::ListItem(item) => Some(item),
                _ => None,
            })
            .collect();

        // A checklist writes a box in place of a bullet, the way the page does.
        // The set rule is scoped to a block of its own rather than put back
        // afterwards: Typst's bullet changes with the depth of the list, and
        // there is no one value to restore.
        let checklist = list.type_() == ListType::Unordered && list.is_checklist();

        if checklist {
            self.out.push_str("#[\n#set list(marker: none)\n");
        }

        // A numbered list that is lettered, starts somewhere other than one, or
        // counts down says so once before its items and puts it back after.
        let numbered = list.type_() == ListType::Ordered;
        let start = numbered.then(|| list.start()).flatten();
        let reversed = numbered && list.has_option("reversed");

        let pattern = numbered
            .then(|| numbering(list))
            .flatten()
            .filter(|pattern| *pattern != ARABIC);

        if let Some(pattern) = pattern {
            let _ = writeln!(self.out, "#set enum(numbering: {})", string(pattern));
        }

        // A callout list is marked the way the listing above it is, so an item
        // and the line it annotates carry the same mark.
        let marked = list.type_() == ListType::Callout && self.options.icons;

        if marked {
            self.out
                .push_str("#set enum(numbering: n => adoccallout(n))\n");
        }

        if let Some(start) = start {
            let _ = writeln!(self.out, "#set enum(start: {start})");
        }

        if reversed {
            // Counting down means the first item carries the highest number,
            // which is wherever the list starts plus one for every item after
            // the first. Typst has no reversed enumeration, so the numbering is
            // written out instead.
            let highest = start.unwrap_or(1) + i64::try_from(items.len()).unwrap_or(1) - 1;

            let _ = writeln!(
                self.out,
                "#set enum(numbering: n => numbering({}, {highest} - n + 1))",
                string(pattern.unwrap_or(ARABIC))
            );
        }

        for item in items {
            let bullet = match list.type_() {
                ListType::Ordered | ListType::Callout => "+",
                ListType::Description => "/",
                ListType::Unordered => "-",
            };

            // A term list writes the term before the colon; every other kind
            // has only the item.
            let term = match (list.type_(), item.list_item_marker()) {
                (
                    ListType::Description,
                    asciidoc_parser::blocks::ListItemMarker::DefinedTerm { term, .. },
                ) => format!("{}: ", self.prose(term.rendered_html())),

                (ListType::Description, _) => ": ".to_string(),
                _ => String::new(),
            };

            let body = self.item(item);
            let _ = writeln!(self.out, "{bullet} {term}{}{body}", box_of(item, checklist));
        }

        if start.is_some() {
            self.out.push_str("#set enum(start: 1)\n");
        }

        if reversed || marked || pattern.is_some() {
            let _ = writeln!(self.out, "#set enum(numbering: {})", string(ARABIC));
        }

        if checklist {
            self.out.push_str("]\n");
        }

        self.out.push('\n');
    }

    /// Everything one list item holds, as markup that can follow its marker.
    ///
    /// An item is a paragraph, and may be several blocks — a nested list, a
    /// listing, a note. Typst reads a line that starts in the first column as
    /// the end of the item, so everything after the first line is indented to
    /// stay part of it.
    fn item(&mut self, item: &asciidoc_parser::blocks::ListItem<'_>) -> String {
        let taken = std::mem::take(&mut self.out);
        self.blocks(item.child_blocks());
        let body = std::mem::replace(&mut self.out, taken);

        let mut lines = body.trim_end().lines();
        let mut out = lines.next().unwrap_or_default().to_string();

        for line in lines {
            out.push('\n');

            if !line.is_empty() {
                out.push_str("  ");
            }

            out.push_str(line);
        }

        out
    }

    /// A table, as Typst's own.
    fn table(&mut self, table: &asciidoc_parser::blocks::TableBlock<'_>) {
        let columns = table.columns().len().max(1);

        let _ = writeln!(
            self.out,
            "#table(\n  columns: {columns},\n  stroke: 0.5pt + rgb(\"#dcdfe4\"),"
        );

        if let Some(header) = table.header_row() {
            self.out.push_str("  table.header(");
            self.cells(header, true);
            self.out.push_str("),\n");
        }

        for row in table.body_rows() {
            self.out.push_str("  ");
            self.cells(row, false);
            self.out.push('\n');
        }

        // A footer is set like a header, because it repeats the header's
        // headings and is read the same way.
        if let Some(footer) = table.footer_row() {
            self.out.push_str("  table.footer(");
            self.cells(footer, true);
            self.out.push_str("),\n");
        }

        self.out.push_str(")\n\n");
    }

    /// One row's cells, as Typst content blocks.
    fn cells(&mut self, row: &asciidoc_parser::blocks::TableRow<'_>, heading: bool) {
        for cell in row.cells() {
            let text = match cell.content() {
                asciidoc_parser::blocks::TableCellContent::Simple(content) => {
                    self.prose(content.rendered_html())
                }

                // A cell holding whole blocks is reduced to its text: a table
                // within a table is more structure than a page wants.
                asciidoc_parser::blocks::TableCellContent::AsciiDoc(_) => String::new(),
            };

            let body = if heading {
                format!("[#text(weight: \"bold\")[{text}]]")
            } else {
                format!("[{text}]")
            };

            // A cell that reaches across columns or down rows has to say so,
            // or the grid puts everything after it in the wrong place.
            let (columns, rows) = (cell.colspan(), cell.rowspan());

            if columns > 1 || rows > 1 {
                let _ = write!(
                    self.out,
                    "table.cell(colspan: {columns}, rowspan: {rows}){body}, "
                );

                continue;
            }

            let _ = write!(self.out, "{body}, ");
        }
    }
}

/// Every label the markup defines, as against the ones it refers to.
///
/// A definition is written either as `<name>` after a heading or as a
/// `#label("name")` call; a reference is always `#link(label("name"))`, which
/// is what tells the two apart.
fn defined(markup: &str) -> std::collections::HashSet<String> {
    let mut labels = std::collections::HashSet::new();
    let mut rest = markup;

    while let Some(at) = rest.find("#label(\"") {
        let before = &rest[..at];
        let after = &rest[at + "#label(\"".len()..];

        if let Some(quote) = after.find("\")")
            && !before.ends_with("#link(")
        {
            labels.insert(after[..quote].to_string());
        }

        rest = after;
    }

    // A heading's own label, which is the last thing on its line.
    for line in markup.lines() {
        let Some(name) = line
            .strip_suffix('>')
            .and_then(|line| line.rsplit_once(" <"))
        else {
            continue;
        };

        // `\>` is an angle bracket the author wrote, not a label.
        if !name.0.ends_with('\\') {
            labels.insert(name.1.to_string());
        }
    }

    labels
}

/// Turn a reference to a label that was never emitted back into plain text.
///
/// The HTML back end can point at anything, because a browser simply does
/// nothing with a broken fragment. Typst refuses to lay out a document that
/// links to a label it cannot find, so a cross reference to a section survives
/// and everything else — a footnote's mark, an anchor on a paragraph — becomes
/// the words it was written as.
fn prune_links(markup: &str) -> String {
    let labels = defined(markup);

    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;

    while let Some(at) = rest.find("#link(label(\"") {
        out.push_str(&rest[..at]);

        let after = &rest[at + "#link(label(\"".len()..];

        let Some(quote) = after.find("\")") else {
            out.push_str(&rest[at..]);
            return out;
        };

        let name = &after[..quote];
        let body = &after[quote + "\")".len()..];

        if labels.contains(name) {
            // Keep it whole; the label is there to be reached.
            out.push_str(&rest[at..at + "#link(label(\"".len() + quote + "\")".len()]);
            rest = body;
            continue;
        }

        // Keep the text, drop the link around it — including the paren that
        // closed the `#link(` this is unwrapping.
        let body = body.strip_prefix(')').unwrap_or(body);

        match (body.strip_prefix('['), body.find(']')) {
            (Some(_), Some(end)) => {
                out.push_str(&body[1..end]);
                rest = &body[end + 1..];
            }

            _ => {
                rest = body;
            }
        }
    }

    out.push_str(rest);
    out
}

/// Whether a block is a diagram that will be drawn, and so captioned beneath.
#[cfg(feature = "mermaid")]
fn is_diagram(block: &Block<'_>) -> bool {
    matches!(block, Block::RawDelimited(raw) if is_mermaid(raw))
}

/// Never: no block is drawn as a diagram in a build without `merman`.
#[cfg(not(feature = "mermaid"))]
fn is_diagram(_block: &Block<'_>) -> bool {
    false
}

/// One of a picture's size attributes, named or in its place in the macro.
fn attrlist_size(
    media: &asciidoc_parser::blocks::MediaBlock<'_>,
    name: &str,
    index: usize,
) -> Option<f64> {
    let attrlist = media.macro_attrlist();

    let value = attrlist
        .named_attribute(name)
        .or_else(|| {
            attrlist
                .nth_attribute(index)
                .filter(|attribute| attribute.name().is_none())
        })
        .map(asciidoc_parser::attributes::ElementAttribute::value)
        .filter(|value| !value.is_empty())?;

    value.trim().trim_end_matches('%').parse().ok()
}

/// What a picture's declared size is as an argument to Typst's `image`.
///
/// AsciiDoc measures in pixels and Typst in points, which are three quarters as
/// many. A picture that declares neither is placed at its own size.
fn size(width: Option<f64>, height: Option<f64>) -> String {
    let mut out = String::new();

    if let Some(width) = width {
        let _ = write!(out, ", width: {}pt", width * 0.75);
    }

    if let Some(height) = height {
        let _ = write!(out, ", height: {}pt", height * 0.75);
    }

    out
}

/// A label, written so that Typst attaches it to what comes before it.
///
/// The angle form is the one Typst reads as a section's own name, which is
/// what puts "Section" in front of a reference in a PDF viewer's own display
/// of the link. An id it cannot read that way keeps its label all the same,
/// written as a call.
fn anchor(id: &str) -> String {
    let readable = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'));

    if readable {
        return format!(" <{id}>");
    }

    format!(" #label({})", string(id))
}

/// The box an item of a checklist is marked with.
///
/// The characters the page uses, so the two read alike. Only an item that
/// carries a checkbox gets one: an item written without is an ordinary item
/// that happens to share the list, and marking it unfinished would say
/// something its author did not.
fn box_of(item: &asciidoc_parser::blocks::ListItem<'_>, checklist: bool) -> &'static str {
    if !checklist {
        return "";
    }

    match item.checkbox() {
        Some(true) => "\u{2713} ",
        Some(false) => "\u{274f} ",
        None => "",
    }
}

/// The markup the parser renders a callout marker as, up to its number.
const CONUM: &str = "<b class=\"conum\">(";

/// A number in a filled circle, for the callouts a listing and its list share.
///
/// The character is the mark: a disc with the numeral knocked out of it, which
/// is the page's mark as well — there a circle drawn by the stylesheet, here a
/// glyph, and both blue with the paper showing through the number.
///
/// Unicode has these for one to twenty and no further, in two runs, so anything
/// past twenty has no mark and keeps the number the parser wrote.
fn circled(number: u32) -> Option<char> {
    let point = match number {
        1..=10 => 0x2775 + number,
        11..=20 => 0x24e0 + number,
        _ => return None,
    };

    char::from_u32(point)
}

/// The colour an admonition's mark is drawn in.
///
/// Asciidoctor's own, which is where the page's palette takes them from; the
/// light values, a printed page being light.
fn admonition_colour(name: &str) -> &'static str {
    match name {
        "tip" => "#b58900",
        "important" => "#bf6900",
        "caution" => "#bf3400",
        "warning" => "#bf0000",
        _ => "#19407c",
    }
}

/// Typst's pattern for ordinary numbering, which is also what a list is put
/// back to once one that numbered itself differently has ended.
const ARABIC: &str = "1.";

/// How an ordered list is numbered, as a Typst numbering pattern.
///
/// A declared style says outright; otherwise the marker's depth decides, which
/// is how AsciiDoc gives a nested list letters without being told to. Both are
/// read the way the HTML back end reads them, so a list is numbered the same
/// way in both.
fn numbering(list: &asciidoc_parser::blocks::ListBlock<'_>) -> Option<&'static str> {
    let style = list
        .declared_style()
        .filter(|style| {
            matches!(
                *style,
                "arabic"
                    | "decimal"
                    | "loweralpha"
                    | "upperalpha"
                    | "lowerroman"
                    | "upperroman"
                    | "lowergreek"
            )
        })
        .or_else(|| list.marker_style())?;

    let pattern = match style {
        "loweralpha" => "a.",
        "upperalpha" => "A.",
        "lowerroman" => "i.",
        "upperroman" => "I.",

        // `lowergreek` has no pattern of its own here, and numbers rather than
        // the wrong alphabet is the better of the two wrong answers.
        _ => ARABIC,
    };

    Some(pattern)
}

/// The language a listing declares, under the name Typst's highlighter knows
/// it by.
///
/// The aliases are the ones `render::highlight` resolves, so a `[source,rs]`
/// block is highlighted here as well as on the page. A name neither of them
/// knows is passed through: Typst matches on syntax names and file extensions
/// too, and anything it cannot place is simply set plain.
fn source_language(raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>) -> Option<String> {
    if raw.declared_style()? != "source" {
        return None;
    }

    let declared = raw
        .attrlist()?
        .named_attribute("language")
        .or_else(|| raw.attrlist()?.nth_attribute(2))
        .map(asciidoc_parser::attributes::ElementAttribute::value)
        .filter(|language| !language.is_empty())?
        .to_lowercase();

    let resolved = match declared.as_str() {
        "sh" | "shell" | "zsh" | "console" | "terminal" => "bash",
        "js" | "mjs" | "cjs" | "node" => "javascript",
        "ts" => "typescript",
        "py" | "python3" => "python",
        "rs" => "rust",
        "golang" => "go",
        "yml" => "yaml",
        "htm" | "xhtml" => "html",
        "jsonc" => "json",
        _ => return Some(declared),
    };

    Some(resolved.to_string())
}

/// Whether a verbatim block was written as a mermaid diagram.
///
/// The same two spellings the page recognises: `[mermaid]`, which
/// `asciidoctor-diagram` uses, and `[source,mermaid]`, which GitHub draws.
#[cfg(feature = "mermaid")]
fn is_mermaid(raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>) -> bool {
    match raw.declared_style() {
        Some(style) if style.to_lowercase() == "mermaid" => true,

        Some(style) if style.to_lowercase() == "source" => raw
            .attrlist()
            .and_then(|attrlist| {
                attrlist
                    .named_attribute("language")
                    .or_else(|| attrlist.nth_attribute(2))
            })
            .is_some_and(|language| language.value().to_lowercase() == "mermaid"),

        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asciidoc_parser::Parser;

    /// The defaults a render runs with: marks drawn, diagrams drawn.
    fn options() -> Options {
        Options {
            icons: true,
            highlight: true,
            mermaid: true,
            math: true,
        }
    }

    fn render(source: &str) -> String {
        let mut parser = Parser::default();
        let document = parser.parse(source);

        markup(&document, Path::new("."), &options()).0
    }

    #[test]
    fn emits_a_heading_and_a_paragraph() {
        let out = render("= Title\n\n== A section\n\nSome *bold* prose.\n");

        assert!(out.contains("= A section"), "{out}");
        assert!(out.contains("*bold*"), "{out}");
    }

    #[test]
    fn emits_a_list() {
        let out = render("= T\n\n* one\n* two\n");

        assert!(out.contains("- one"), "{out}");
        assert!(out.contains("- two"), "{out}");
    }

    #[test]
    fn emits_a_table() {
        let out = render("= T\n\n|===\n|a |b\n|===\n");

        assert!(out.contains("#table("), "{out}");
        assert!(out.contains("columns: 2"), "{out}");
    }

    #[test]
    fn hands_a_listing_its_language_to_highlight() {
        let out = render("= T\n\n[source,rs]\n----\nfn main() {}\n----\n");

        assert!(out.contains("lang: \"rust\""), "{out}");
    }

    #[test]
    fn numbers_a_nested_list_the_way_its_depth_says() {
        let out = render("= T\n\n. one\n.. two\n");

        assert!(out.contains("#set enum(numbering: \"a.\")"), "{out}");
    }

    #[test]
    fn places_an_outline_when_the_document_asks_for_one() {
        let out = render("= T\n:toc:\n:toclevels: 3\n\n== A section\n");

        assert!(
            out.contains("#outline(title: [Table of Contents], depth: 3)"),
            "{out}"
        );
    }

    #[test]
    fn places_no_outline_when_it_was_not_asked_for() {
        let out = render("= T\n\n== A section\n");

        assert!(!out.contains("#outline"), "{out}");
    }

    #[test]
    fn keeps_a_discrete_heading_out_of_the_outline() {
        let out = render("= T\n:toc:\n\n[discrete]\n== Aside\n\n== A section\n");

        assert!(out.contains("outlined: false"), "{out}");
    }

    #[test]
    fn keeps_an_items_own_blocks_inside_it() {
        let out = render("= T\n\n* one\n+\nMore about one.\n* two\n");

        // Typst reads a line in the first column as the end of the item, so
        // everything after the first line has to be indented.
        assert!(out.contains("- one\n\n  More about one."), "{out}");
    }

    #[test]
    fn numbers_a_list_from_where_it_says() {
        let out = render("= T\n\n[start=7]\n. seven\n. eight\n");

        assert!(out.contains("#set enum(start: 7)"), "{out}");
        assert!(
            out.contains("#set enum(start: 1)"),
            "should be put back: {out}"
        );
    }

    #[test]
    fn keeps_the_text_of_a_paragraph_admonition() {
        let out = render("= T\n\nTIP: Worth knowing.\n");

        assert!(out.contains("/icon-tip.svg"), "{out}");
        assert!(out.contains("Worth knowing."), "{out}");
    }

    #[test]
    fn labels_an_admonition_when_it_draws_no_icon() {
        let mut parser = Parser::default();
        let document = parser.parse("= T\n\nTIP: Worth knowing.\n");

        let plain = Options {
            icons: false,
            ..options()
        };

        let out = markup(&document, Path::new("."), &plain).0;

        assert!(out.contains("TIP"), "{out}");
        assert!(!out.contains("icon-tip"), "{out}");
    }

    #[test]
    fn marks_a_callout_and_its_item_the_same_way() {
        let out = render("= T\n\n----\ncode <1>\n----\n<1> Why.\n");

        assert!(
            out.contains('\u{2776}'),
            "the listing keeps its mark: {out}"
        );
        assert!(out.contains("adoccallout"), "and so does its list: {out}");
    }

    #[test]
    fn takes_a_page_break_literally() {
        let out = render("= T\n\none\n\n<<<\n\ntwo\n");

        assert!(out.contains("#pagebreak()"), "{out}");
    }

    #[test]
    fn keeps_a_cross_reference_and_unwraps_a_mark() {
        let out = render("= T\n\n[[here]]\n== Here\n\nSee <<here>>, and a note.footnote:[Why.]\n");

        assert!(out.contains("#link(label(\"here\"))"), "{out}");

        // The footnote's mark points at a definition that is not a label here,
        // so it keeps its number and loses the link.
        assert!(!out.contains("_footnotedef_"), "{out}");
        assert!(out.contains("Why."), "{out}");
    }

    #[test]
    fn a_sections_label_belongs_to_its_heading() {
        // Typst attaches a label to what comes before it, so one written above
        // a heading names whatever preceded the heading instead.
        let out = render("= T\n\nProse.\n\n[[here]]\n== Here\n");

        assert!(out.contains("= Here <here>"), "{out}");
        assert!(!out.contains("#label(\"here\")\n= Here"), "{out}");
    }

    #[test]
    fn an_anchored_term_can_be_reached() {
        let out =
            render("= T\n\nSee <<block,a block>>.\n\n[glossary]\n[[block]]block:: A shape.\n");

        assert!(out.contains("#box[]#label(\"block\")"), "the anchor: {out}");
        assert!(
            out.contains("#link(label(\"block\"))[a block]"),
            "and the reference to it: {out}"
        );
    }

    #[test]
    fn a_reference_to_nothing_keeps_its_words() {
        let out = render("= T\n\nSee <<nowhere>>.\n");

        assert!(!out.contains("#link"), "{out}");
        assert!(out.contains("nowhere"), "{out}");
    }

    #[test]
    fn typesets_both_notations() {
        let out = render("= T\n:stem: latexmath\n\n[stem]\n++++\n\\frac{a}{b}\n++++\n");

        assert!(out.contains("$ frac(a ,b ) $"), "{out}");

        // `AsciiMath` needs the parser the `math` feature brings; without it
        // the equation is shown as it was written.
        let ascii = render("= T\n\n[stem]\n++++\nsqrt(4)\n++++\n");

        if cfg!(feature = "math") {
            assert!(ascii.contains("$ sqrt(4) $"), "{ascii}");
        } else {
            assert!(ascii.contains("#raw(\"sqrt(4)\")"), "{ascii}");
        }
    }

    #[test]
    fn typesets_an_equation_in_a_line_of_text() {
        let out = render("= T\n:stem: latexmath\n\nA line with stem:[x^2] in it.\n");

        assert!(out.contains("$x ^(2 )$"), "{out}");
    }

    #[test]
    fn falls_back_to_the_source_when_an_equation_will_not_typeset() {
        // `\hbar` converts to a symbol this Typst no longer carries, so the
        // document only lays out once the equations are shown as written.
        let mut parser = Parser::default();
        let document = parser.parse("= T\n:stem: latexmath\n\n[stem]\n++++\n\\hbar\\omega\n++++\n");

        let pdf = pdf(&document, Path::new("."), &options()).expect("still makes a PDF");

        assert!(pdf.bytes.starts_with(b"%PDF"), "should be a PDF");

        // The caller has to be able to tell that it did not get what it asked
        // for; this back end has nowhere to say so but its answer.
        assert!(
            pdf.fallback.is_some_and(|said| said.contains("equation")),
            "the fallback should be reported"
        );
    }

    #[test]
    fn compiles_to_a_pdf() {
        let mut parser = Parser::default();
        let document = parser.parse("= Title\n\n== Section\n\nProse with *bold*.\n\n* a\n* b\n");

        let pdf = pdf(&document, Path::new("."), &options()).expect("compiles");

        assert!(pdf.bytes.starts_with(b"%PDF"), "should be a PDF");
        assert!(pdf.bytes.len() > 1000, "should have content");
        assert!(pdf.fallback.is_none(), "nothing should have been given up");
    }
}
