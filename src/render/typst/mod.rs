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
//! with it. [`inline`] translates that.
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

mod inline;

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
};

use crate::render::typst::inline::{
    string,
    typst as inline_markup,
};

/// Render `document` as a PDF.
///
/// `base` is the directory the document was read from, which is where the
/// images it names are looked for.
pub fn pdf(document: &Document<'_>, base: &Path) -> Result<Vec<u8>> {
    let (source, images) = markup(document, base);

    compile(&source, &images)
}

/// The Typst markup for a document, and the image files it refers to.
pub fn markup(document: &Document<'_>, base: &Path) -> (String, Vec<(String, Vec<u8>)>) {
    let mut out = Preamble::new(document).to_string();

    let mut emitter = Emitter {
        out: String::new(),
        base: base.to_path_buf(),
        images: Vec::new(),
        labels: std::collections::HashSet::new(),
    };

    emitter.blocks(document.child_blocks());
    emitter.footnotes(document);
    out.push_str(&prune_links(&emitter.out, &emitter.labels));

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

/// The page setup and title block that every rendered document opens with.
struct Preamble(String);

impl Preamble {
    fn new(document: &Document<'_>) -> Self {
        let mut out = String::new();

        let title = document
            .doctitle_sanitized()
            .unwrap_or_else(|| "Untitled".to_string());

        let _ = writeln!(
            out,
            "#set document(title: {})\n#set page(paper: \"a4\", margin: 2.2cm, numbering: \
             \"1\")\n#set text(size: 10.5pt)\n#set par(justify: true, leading: 0.62em)\n#show \
             heading: it => block(above: 1.4em, below: 0.7em, it)\n#show link: it => text(fill: \
             rgb(\"#1565a8\"), it)\n#show raw.where(block: true): it => block(\n\x20 width: 100%, \
             fill: rgb(\"#f5f6f8\"), inset: 8pt, radius: 3pt, it,\n)\n{}\n",
            string(&title),
            FITTED
        );

        if document.doctitle().is_some() {
            let _ = writeln!(
                out,
                "#align(center)[#text(size: 20pt, weight: \"bold\")[{}]]",
                inline_markup(&title)
            );

            let authors: Vec<String> = document
                .authors()
                .iter()
                .map(|author| inline_markup(author.name()))
                .collect();

            if !authors.is_empty() {
                let _ = writeln!(
                    out,
                    "#align(center)[#text(size: 10pt)[{}]]",
                    authors.join(", ")
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

    /// Every label emitted, so a reference to one that is not can be found.
    labels: std::collections::HashSet<String>,
}

impl Emitter {
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

                if section.section_type() != SectionType::Discrete
                    && let Some(id) = section.id()
                {
                    self.labels.insert(id.to_string());
                    let _ = writeln!(self.out, "#label({})", string(id));
                }

                let _ = writeln!(
                    self.out,
                    "{} {}\n",
                    "=".repeat(level),
                    inline_markup(section.section_title())
                );

                self.blocks(section.child_blocks());
            }

            Block::Simple(simple) => {
                let text = inline_markup(simple.content().rendered_html());

                if !text.trim().is_empty() {
                    let _ = writeln!(self.out, "{text}\n");
                }
            }

            Block::List(list) => self.list(list),
            Block::Table(table) => self.table(table),
            Block::Preamble(preamble) => self.blocks(preamble.child_blocks()),
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

    /// The `footnote:[]` definitions, gathered at the end of the document.
    ///
    /// A reference is rendered inline by the parser as the mark alone, so
    /// without these the numbers in the text stand for nothing.
    fn footnotes(&mut self, document: &Document<'_>) {
        let footnotes = document.catalog().footnotes();

        if footnotes.is_empty() {
            return;
        }

        self.out.push_str("#line(length: 100%)\n\n");

        for footnote in footnotes {
            let _ = writeln!(
                self.out,
                "#text(size: 9pt)[{}. {}]\n",
                inline_markup(&footnote.index),
                inline_markup(&footnote.text)
            );
        }
    }

    /// The `.A title` line a block can carry, with the caption the document
    /// numbers it with.
    fn title(&mut self, block: &Block<'_>) {
        let Some(title) = block.title() else {
            return;
        };

        let caption = block.caption().unwrap_or_default();

        let _ = writeln!(
            self.out,
            "#block(above: 1em, below: 0.5em)[#text(weight: \"bold\", size: 9.5pt)[{}{}]]\n",
            inline_markup(caption),
            inline_markup(title)
        );
    }

    /// A verbatim block: a listing, a literal, or something with no rendering
    /// of its own.
    fn raw(&mut self, raw: &asciidoc_parser::blocks::RawDelimitedBlock<'_>) {
        // Taken as text rather than unescaped, because a listing's callouts
        // reach here as `<b class="conum">` and would otherwise be shown.
        let content = inline::text(raw.content().rendered_html());

        if self.diagram(raw, &content) {
            return;
        }

        match raw.raw_context().as_ref() {
            // A comment reaches the page as nothing at all, and so does a
            // passthrough, which is HTML and has no meaning here.
            "comment" | "pass" => {}

            // Mathematics is shown as its source rather than typeset. Typst
            // has a mathematics mode of its own, but its syntax is not LaTeX's
            // and not AsciiMath's — `\sum_{i=1}^{n}` is `sum_(i=1)^n` there —
            // so handing an equation over unchanged fails outright. Showing
            // what the author wrote is the honest answer until it can be
            // translated properly.
            "stem" => {
                let _ = writeln!(
                    self.out,
                    "#align(center)[#raw({})]\n",
                    string(content.trim())
                );
            }

            _ => self.verbatim(&content),
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

        let Some(svg) = crate::render::mermaid::printable(content) else {
            return false;
        };

        let name = format!("/diagram-{}.svg", self.images.len());
        self.images.push((name.clone(), svg.into_bytes()));

        self.figure(&name, raw);

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
        // A raw block delimited by enough backticks to contain whatever is in
        // it, so the content needs no escaping of its own.
        let fence = "`".repeat(longest_backtick_run(content).max(2) + 1);

        let _ = writeln!(self.out, "{fence}\n{content}\n{fence}\n");
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

    /// A note, tip, warning and so on, set apart with its label.
    fn admonition(&mut self, admonition: &asciidoc_parser::blocks::AdmonitionBlock<'_>) {
        let label = admonition.label().to_string().to_uppercase();

        let _ = write!(
            self.out,
            "#block(width: 100%, inset: (left: 10pt), stroke: (left: 2pt + \
             rgb(\"#1565a8\")))[\n#text(weight: \"bold\", size: 9pt)[{label}]\n\n"
        );

        // The paragraph form — `TIP: text` — carries its text directly and has
        // no child blocks at all, so asking only for the children loses it.
        match admonition.content() {
            Some(content) => {
                self.out.push_str(&inline_markup(content.rendered_html()));
                self.out.push_str("\n\n");
            }

            None => self.blocks(admonition.child_blocks()),
        }

        self.out.push_str("]\n\n");
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
                self.out.push_str(&inline_markup(content.rendered_html()));
                self.out.push_str("\n\n");
            }

            None => self.blocks(quote.child_blocks()),
        }

        if let Some(attribution) = quote.attribution() {
            let _ = writeln!(
                self.out,
                "#text(size: 9pt, fill: rgb(\"#656d77\"))[— {}]",
                inline_markup(attribution)
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

        self.figure(&format!("/{target}"), media);
    }

    /// A picture, with its caption beneath it and held on the same page.
    fn figure<'src>(&mut self, name: &str, block: &impl IsBlock<'src>) {
        let caption = match block.title() {
            Some(title) => format!(
                ", caption: [{}{}], numbering: none",
                inline_markup(block.caption().unwrap_or_default()),
                inline_markup(title)
            ),

            None => String::new(),
        };

        let _ = writeln!(
            self.out,
            "#block(breakable: false, width: 100%)[#figure(fitted(image({})){caption})]\n",
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

        // A numbered list that starts somewhere other than one, or counts
        // down, says so once before its items and puts it back afterwards.
        let numbered = list.type_() == ListType::Ordered;
        let start = numbered.then(|| list.start()).flatten();
        let reversed = numbered && list.has_option("reversed");

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
                "#set enum(numbering: n => numbering(\"1.\", {highest} - n + 1))"
            );
        }

        for item in items {
            let marker = match list.type_() {
                ListType::Ordered => "+",
                ListType::Description => "/",
                _ => "-",
            };

            // A term list writes the term before the colon; every other kind
            // has only the item.
            let term = match (list.type_(), item.list_item_marker()) {
                (
                    ListType::Description,
                    asciidoc_parser::blocks::ListItemMarker::DefinedTerm { term, .. },
                ) => format!("{}: ", inline_markup(term.rendered_html())),

                (ListType::Description, _) => ": ".to_string(),
                _ => String::new(),
            };

            let body = self.item(item);
            let _ = writeln!(self.out, "{marker} {term}{body}");
        }

        if start.is_some() {
            self.out.push_str("#set enum(start: 1)\n");
        }

        if reversed {
            self.out.push_str("#set enum(numbering: \"1.\")\n");
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
                    inline_markup(content.rendered_html())
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

/// Turn a reference to a label that was never emitted back into plain text.
///
/// The HTML back end can point at anything, because a browser simply does
/// nothing with a broken fragment. Typst refuses to lay out a document that
/// links to a label it cannot find, so a cross reference to a section survives
/// and everything else — a footnote's mark, an anchor on a paragraph — becomes
/// the words it was written as.
fn prune_links(markup: &str, labels: &std::collections::HashSet<String>) -> String {
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

/// The longest run of backticks in a block, so a fence can be made longer.
fn longest_backtick_run(content: &str) -> usize {
    let mut longest = 0;
    let mut run = 0;

    for c in content.chars() {
        if c == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }

    longest
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

    fn render(source: &str) -> String {
        let mut parser = Parser::default();
        let document = parser.parse(source);

        markup(&document, Path::new(".")).0
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
    fn fences_a_listing_past_its_own_backticks() {
        let out = render("= T\n\n----\ncode with ``` in it\n----\n");

        assert!(out.contains("````"), "{out}");
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

        assert!(out.contains("TIP"), "{out}");
        assert!(out.contains("Worth knowing."), "{out}");
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
    fn compiles_to_a_pdf() {
        let mut parser = Parser::default();
        let document = parser.parse("= Title\n\n== Section\n\nProse with *bold*.\n\n* a\n* b\n");

        let bytes = pdf(&document, Path::new(".")).expect("compiles");

        assert!(bytes.starts_with(b"%PDF"), "should be a PDF");
        assert!(bytes.len() > 1000, "should have content");
    }
}
