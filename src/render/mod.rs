//! The HTML5 back end.
//!
//! `asciidoc-parser` renders *inline* content — bold, links, cross-references,
//! passthroughs — but deliberately stops there: turning the block tree into a
//! page is the back end's job, and this module is that back end. The markup it
//! emits follows Asciidoctor's HTML5 converter (the same wrapper `div`s and
//! class names), so stylesheets written for Asciidoctor apply unchanged.
//!
//! # A back end we could adopt instead
//!
//! [`asciidoc-html5`](https://github.com/asciidoc-rs/asciidoc-html5) is the
//! parser author's own HTML5 back end, and it covers what this module covers:
//! rendered against `resources/showcase.adoc` the two agree on every wrapper
//! and class name, and on constructs the showcase leaves out — video, audio,
//! bibliographies, roles, hard breaks, passthroughs — they agree byte for byte.
//! Both are bounded by the same parser, so neither is ahead on coverage.
//!
//! Adopting it would mean giving up what this module does *beyond* Asciidoctor,
//! with no supported way to add it back:
//!
//! - source blocks highlighted here rather than in the browser, which that
//!   crate rules out ("client-side syntax highlighters only");
//! - mermaid blocks turned into diagrams, which it renders as listings;
//! - admonitions marked with an icon rather than a label;
//! - the labelled document header ([`Renderer::details`]).
//!
//! Its README also rules out an extension mechanism before 1.0, so hooking
//! those back in would mean rewriting its output rather than configuring it.
//!
//! It is worth revisiting if that changes — or if the long tail of Asciidoctor
//! fidelity becomes more work than it is worth. That tail is now measured
//! rather than guessed at: `tests/doctest.rs` renders Asciidoctor's own test
//! corpus through this back end and compares the result against Asciidoctor's,
//! and `KNOWN_FAILURES` there is the whole of what does not match yet.

mod block;
mod callout;
mod css;
mod html;

#[cfg_attr(not(feature = "highlight"), path = "highlight_off.rs")]
mod highlight;
mod icons;
mod list;
#[cfg(feature = "math")]
mod math;
mod media;
#[cfg(feature = "mermaid")]
mod mermaid;
mod table;
mod toc;

use asciidoc_parser::{
    Document,
    blocks::FindBlocks,
    document::{
        InterpretedValue,
        TocMode,
    },
};

use crate::render::html::Buffer;
pub use crate::render::html::{
    escape_attr,
    escape_text,
};

/// How the rendered body should be wrapped.
#[derive(Clone, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "these mirror command line flags, and one field per flag is what reads clearly"
)]
pub struct Options {
    /// Emit only the body content, with no surrounding HTML page.
    pub fragment: bool,

    /// CSS to embed in the page, or `None` for an unstyled page.
    ///
    /// Ignored when [`fragment`](Self::fragment) is set, since a fragment has
    /// no `<head>` to put it in.
    pub stylesheet: Option<String>,

    /// Markup appended just before `</body>`, for scripts and the like.
    ///
    /// Ignored when [`fragment`](Self::fragment) is set, for the same reason.
    pub body_suffix: String,

    /// Whether an admonition is marked with an icon rather than its label.
    pub icons: bool,

    /// Whether a source block is syntax highlighted.
    ///
    /// A block still carries its `language-…` class either way, so a page can
    /// be highlighted in the browser instead.
    pub highlight: bool,

    /// Whether a mermaid block is drawn as a diagram, or left as the listing
    /// block it was written as.
    pub mermaid: bool,

    /// Whether an equation is converted to `MathML`, or left as the notation it
    /// was written in.
    pub math: bool,
}

/// What a render produced.
#[derive(Clone, Debug)]
pub struct Rendered {
    /// The markup.
    pub html: String,
}

/// The stylesheet embedded in a standalone page when the caller names no other.
pub fn default_stylesheet() -> String {
    css::DEFAULT.to_string()
}

/// Render `document` to HTML.
pub fn render<'src>(document: &'src Document<'src>, options: &'src Options) -> Rendered {
    let mut renderer = Renderer {
        document,
        options,
        out: Buffer::new(),
        toc_rendered: false,
        #[cfg(feature = "mermaid")]
        drawings: 0,
    };

    let body = renderer.body();

    // The parser marks an inline equation with the delimiters a typesetter used
    // to look for. Nothing looks for them now, so they are converted here, over
    // the whole body at once — an equation can sit in a paragraph, a heading, a
    // list item or a table cell, and this catches all of them in one place.
    let body = inline_equations(&body);

    let html = if options.fragment {
        body
    } else {
        document_page(document, options, &body)
    };

    Rendered { html }
}

/// Convert the inline equations in a rendered body.
#[cfg(feature = "math")]
fn inline_equations(body: &str) -> String {
    math::inline(body)
}

/// Leaves them as they are: no converter is compiled in.
#[cfg(not(feature = "math"))]
fn inline_equations(body: &str) -> String {
    body.to_string()
}

/// Walks the block tree, appending markup as it goes.
struct Renderer<'src> {
    /// The document being rendered; consulted for attributes and TOC settings.
    document: &'src Document<'src>,

    /// How the caller asked for the document to be rendered.
    options: &'src Options,

    /// Markup accumulated so far.
    out: Buffer,

    /// Whether a `toc::[]` macro has already emitted the table of contents.
    ///
    /// Asciidoctor renders the macro form at most once; a second `toc::[]` is
    /// ignored rather than duplicating the whole outline.
    toc_rendered: bool,

    /// How many diagrams have been drawn, so each can be given a name of its
    /// own. Two elements on a page may not share an id.
    #[cfg(feature = "mermaid")]
    drawings: usize,
}

impl Renderer<'_> {
    /// Render the document header, body and footer into a single body fragment.
    ///
    /// An embedded fragment drops the page furniture — the `#header` and
    /// `#footer` wrappers and the `#content` div — because the page it is
    /// pasted into supplies its own, and a "Last updated" line belongs to a
    /// standalone document rather than to a fragment of one. This matches what
    /// Asciidoctor emits with `--no-header-footer`.
    fn body(&mut self) -> String {
        if self.options.fragment {
            // The document title is normally the host page's job, so it appears
            // only when the document asks for it with `:showtitle:`.
            if self.document.show_title(false) {
                self.doctitle();
            }

            if toc_precedes_content(self.document.toc_mode()) {
                self.toc();
            }

            self.content();
            self.footnotes();
        } else {
            self.header();

            self.out.open("div", Some("content"), &[]);
            self.content();
            self.out.close("div");

            self.footnotes();
            self.footer();
        }

        std::mem::replace(&mut self.out, Buffer::new()).finish()
    }

    /// Emit the document title as an `<h1>`, with its subtitle if it has one.
    fn doctitle(&mut self) {
        let Some(title) = self.document.doctitle() else {
            return;
        };

        // `doctitle` is the whole title, subtitle included, so a title split
        // into two parts has to be rebuilt from the parts — appending the
        // subtitle to the whole would say it twice.
        let heading = match (
            self.document.header().main_title(),
            self.document.subtitle(),
        ) {
            (Some(main), Some(subtitle)) => {
                format!("{main}: <span class=\"subtitle\">{subtitle}</span>")
            }

            _ => title.to_string(),
        };

        // Every part is already inline-rendered by the parser.
        self.out.line(&format!("<h1>{heading}</h1>"));
    }

    /// Render `#header`: the document title, author line and, for most TOC
    /// placements, the table of contents.
    fn header(&mut self) {
        let show_title = self.document.show_title(true) && self.document.doctitle().is_some();
        let details = self.details();
        let toc_in_header = toc_precedes_content(self.document.toc_mode());

        if !show_title && details.is_empty() && !toc_in_header {
            return;
        }

        self.out.open("div", Some("header"), &[]);

        if show_title {
            self.doctitle();
        }

        if !details.is_empty() {
            self.out.open("div", None, &["details"]);
            self.out.raw(&details);
            self.out.close("div");
        }

        if toc_in_header {
            self.toc();
        }

        self.out.close("div");
    }

    /// The author and revision lines shown under the document title.
    ///
    /// Each is a labelled line — `Author: …`, `Version: …` — rather than the
    /// bare stack of values Asciidoctor emits, which leaves the reader to work
    /// out what a lone date on its own line is meant to be. The ids and classes
    /// are Asciidoctor's, so a stylesheet written for it still finds them.
    fn details(&self) -> String {
        let mut details = Buffer::new();

        if let Some(line) = self.authors() {
            details.raw(&line);
        }

        details.raw(&self.revision());

        details.raw(&self.metadata());

        // The remark describes the revision rather than naming part of it, so
        // it reads as its own sentence, set apart from the labels above.
        if let Some(remark) = self.attribute("revremark") {
            details.line(&format!(
                "<div class=\"remark\"><span id=\"revremark\">{}</span></div>",
                escape_text(&remark)
            ));
        }

        details.finish()
    }

    /// The `Author:` line, or `None` when the document names nobody.
    fn authors(&self) -> Option<String> {
        let authors = self.document.authors();

        if authors.is_empty() {
            return None;
        }

        let written: Vec<String> = authors
            .iter()
            .enumerate()
            .map(|(index, author)| {
                // Asciidoctor numbers every author after the first; the first
                // is plain `author`/`email` so existing
                // stylesheets keep working.
                let suffix = if index == 0 {
                    String::new()
                } else {
                    (index + 1).to_string()
                };

                let name = format!(
                    "<span id=\"author{suffix}\" class=\"author\">{}</span>",
                    escape_text(author.name())
                );

                // The angle brackets are the convention every reader already
                // knows from a commit or a mail header.
                match author.email() {
                    Some(email) => format!(
                        "{name} <span id=\"email{suffix}\" class=\"email\">&lt;<a \
                         href=\"mailto:{}\">{}</a>&gt;</span>",
                        escape_attr(email),
                        escape_text(email)
                    ),

                    None => name,
                }
            })
            .collect();

        let label = if written.len() == 1 {
            "Author"
        } else {
            "Authors"
        };

        Some(detail(label, &written.join(", ")))
    }

    /// The `Version:` and `Date:` lines.
    ///
    /// These read the attributes rather than the revision line, because an
    /// explicit `v1.0, 2026-09-10` line sets them too — so the two ways of
    /// writing a revision, the line and `:revnumber:`/`:revdate:`, are one case
    /// here rather than two. Either may appear without the other.
    fn revision(&self) -> String {
        let mut rows = String::new();

        if let Some(number) = self.attribute("revnumber") {
            rows.push_str(&detail(
                "Version",
                &format!("<span id=\"revnumber\">{}</span>", escape_text(&number)),
            ));
        }

        if let Some(date) = self.attribute("revdate") {
            rows.push_str(&detail(
                "Date",
                &format!("<span id=\"revdate\">{}</span>", escape_text(&date)),
            ));
        }

        rows
    }

    /// The rows for whatever else the header says about the document.
    ///
    /// A header holds two sorts of attribute: facts about the document, and
    /// instructions to the renderer. `:status:` is the first sort and
    /// `:sectnums:` the second, and only the first belongs in front of a
    /// reader — so this shows a known set of metadata names plus everything in
    /// Antora's `page-` namespace, which is defined as page metadata, and
    /// leaves every other attribute alone.
    fn metadata(&self) -> String {
        let mut rows = String::new();

        for attribute in self.document.header().attributes() {
            let name = attribute.name().data();

            let Some(shown) = displayed_as(name) else {
                continue;
            };

            // An attribute set without a value — `:sectnums:` — says something
            // to the renderer and nothing to a reader.
            let InterpretedValue::Value(value) = attribute.value() else {
                continue;
            };

            if value.is_empty() {
                continue;
            }

            rows.push_str(&detail(&label_for(shown), &value_for(shown, value)));
        }

        rows
    }

    /// Render every top-level block, honouring the TOC placements that fall
    /// inside the content area.
    fn content(&mut self) {
        let toc_mode = self.document.toc_mode();

        for block in self.document.child_blocks() {
            self.block(block);
        }

        if matches!(toc_mode, TocMode::Bottom) {
            self.toc();
        }
    }

    /// Render `#footnotes`: the definitions the `footnote:[]` references in the
    /// body link down to.
    ///
    /// The parser renders each reference as a link to `#_footnotedef_N`, so
    /// without this block every footnote in the document is a dead link.
    fn footnotes(&mut self) {
        let footnotes = self.document.catalog().footnotes();

        if footnotes.is_empty() {
            return;
        }

        self.out.open("div", Some("footnotes"), &[]);
        self.out.line("<hr>");

        for footnote in footnotes {
            let index = escape_attr(&footnote.index);

            // Written out rather than opened, because Asciidoctor puts the
            // class before the id here and nowhere else.
            self.out.line(&format!(
                "<div class=\"footnote\" id=\"_footnotedef_{index}\">"
            ));
            self.out.line(&format!(
                "<a href=\"#_footnoteref_{index}\">{}</a>. {}",
                escape_text(&footnote.index),
                footnote.text
            ));
            self.out.close("div");
        }

        self.out.close("div");
    }

    /// Render `#footer`: the document's revision, then the date it was last
    /// modified, each on a line of its own.
    ///
    /// A document with neither has no footer at all.
    fn footer(&mut self) {
        if self.document.is_attribute_set("nofooter") {
            return;
        }

        let mut lines: Vec<String> = Vec::new();

        if let Some(number) = self.attribute("revnumber") {
            let label = self
                .attribute("version-label")
                .unwrap_or_else(|| "Version".to_string());

            lines.push(format!("{} {}", escape_text(&label), escape_text(&number)));
        }

        if let Some(date) = self
            .attribute("docdatetime")
            .or_else(|| self.attribute("localdatetime"))
        {
            let label = self
                .attribute("last-update-label")
                .unwrap_or_else(|| "Last updated".to_string());

            lines.push(format!("{} {}", escape_text(&label), escape_text(&date)));
        }

        if lines.is_empty() {
            return;
        }

        self.out.open("div", Some("footer"), &[]);
        self.out.open("div", Some("footer-text"), &[]);
        self.out.line(&lines.join("<br>\n"));
        self.out.close("div");
        self.out.close("div");
    }

    /// Render something into a buffer of its own and hand back the markup.
    ///
    /// Used where a wrapper should only be written once its contents turn out
    /// to be worth wrapping.
    pub(super) fn aside(&mut self, render: impl FnOnce(&mut Self)) -> String {
        let outer = std::mem::replace(&mut self.out, Buffer::new());

        render(self);

        std::mem::replace(&mut self.out, outer).finish()
    }

    /// The value of a document attribute, if it is set to one.
    fn attribute(&self, name: &str) -> Option<String> {
        match self.document.attribute_value(name) {
            InterpretedValue::Value(value) => Some(value),
            _ => None,
        }
    }
}

/// Header attributes shown to the reader as facts about the document.
///
/// Everything else a header sets — `sectnums`, `icons`, `source-highlighter` —
/// is an instruction to the renderer rather than something a reader wants to
/// read, so the list is an allowlist: an attribute nobody thought about is left
/// out rather than shown by accident.
const METADATA: &[&str] = &[
    "status",
    "keywords",
    "category",
    "edition",
    "organization",
    "copyright",
];

/// Antora's namespace for page metadata. An attribute in it is shown with the
/// prefix dropped, so `:page-tags:` reads as `Tags`.
const PAGE_PREFIX: &str = "page-";

/// One labelled line of the document header.
fn detail(label: &str, value: &str) -> String {
    format!("<div class=\"detail\"><span class=\"label\">{label}:</span> {value}</div>\n")
}

/// The name an attribute is shown under, or `None` if it is not shown at all.
fn displayed_as(name: &str) -> Option<&str> {
    if let Some(rest) = name.strip_prefix(PAGE_PREFIX) {
        return (!rest.is_empty()).then_some(rest);
    }

    METADATA.contains(&name).then_some(name)
}

/// The label for an attribute name: `page-last-reviewed` reads `Last reviewed`.
fn label_for(name: &str) -> String {
    let spaced = name.replace(['-', '_'], " ");
    let mut characters = spaced.chars();

    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => spaced,
    }
}

/// The markup for an attribute's value.
///
/// A list of tags or keywords is a set of separate things that happens to be
/// written with commas, so each is shown as its own mark rather than run
/// together into a sentence.
fn value_for(name: &str, value: &str) -> String {
    if !matches!(name, "tags" | "keywords") {
        return escape_text(value);
    }

    value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(|tag| format!("<span class=\"tag\">{}</span>", escape_text(tag)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a TOC placement puts the outline above the document's content.
fn toc_precedes_content(mode: TocMode) -> bool {
    matches!(
        mode,
        TocMode::Auto | TocMode::Left | TocMode::Right | TocMode::Top
    )
}

/// Everything about a standalone page that is not its body markup.
///
/// This is public so that a caller which builds its own body — the server's
/// directory listing, say — can put it in the same page as a rendered document
/// and have it look the same.
#[derive(Clone, Debug, Default)]
pub struct Page<'a> {
    /// Value of the `lang` attribute on `<html>`.
    pub lang: &'a str,

    /// Plain-text `<title>`; markup here would be shown, not applied.
    pub title: &'a str,

    /// Value of the `description` meta tag, if there is one.
    pub description: Option<&'a str>,

    /// Class list for `<body>`.
    pub body_classes: &'a str,

    /// CSS to embed, or `None` for an unstyled page.
    pub stylesheet: Option<&'a str>,

    /// Markup appended just before `</body>`, for scripts and the like.
    pub body_suffix: &'a str,
}

/// Wrap body markup in a complete HTML page.
#[must_use]
pub fn page(page: &Page<'_>, body: &str) -> String {
    let mut out = Buffer::new();

    out.line("<!DOCTYPE html>");
    out.line(&format!("<html lang=\"{}\">", escape_attr(page.lang)));
    out.line("<head>");
    out.line("<meta charset=\"UTF-8\">");
    out.line("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">");
    out.line(&format!(
        "<meta name=\"generator\" content=\"adocers {}\">",
        env!("CARGO_PKG_VERSION")
    ));

    if let Some(description) = page.description {
        out.line(&format!(
            "<meta name=\"description\" content=\"{}\">",
            escape_attr(description)
        ));
    }

    out.line(&format!("<title>{}</title>", escape_text(page.title)));

    if let Some(css) = page.stylesheet {
        out.line("<style>");
        out.line(css);
        out.line("</style>");
    }

    out.line("</head>");
    out.line(&format!(
        "<body class=\"{}\">",
        escape_attr(page.body_classes)
    ));
    out.raw(body);
    out.newline();

    if !page.body_suffix.is_empty() {
        out.line(page.body_suffix);
    }

    out.line("</body>");
    out.line("</html>");

    out.finish()
}

/// Wrap a rendered document's body in a page built from its own metadata.
fn document_page(document: &Document<'_>, options: &Options, body: &str) -> String {
    let lang = match document.attribute_value("lang") {
        InterpretedValue::Value(lang) => lang,
        _ => "en".to_string(),
    };

    let description = match document.attribute_value("description") {
        InterpretedValue::Value(description) => Some(description),
        _ => None,
    };

    // The `<title>` is plain text, so any markup the inline renderer produced
    // for the doctitle has to come back out.
    let title = document
        .doctitle_sanitized()
        .unwrap_or_else(|| "Untitled".to_string());

    // Nothing is delivered to the page any more: diagrams are drawn here and
    // equations are converted here, so the only thing appended is whatever the
    // caller asked for.
    let body_suffix = options.body_suffix.clone();

    page(
        &Page {
            lang: &lang,
            title: &title,
            description: description.as_deref(),
            body_classes: &body_classes(document),
            stylesheet: options.stylesheet.as_deref(),
            body_suffix: body_suffix.trim_end(),
        },
        body,
    )
}

/// The `<body>` class list, which is how Asciidoctor stylesheets learn where
/// the table of contents was placed.
fn body_classes(document: &Document<'_>) -> String {
    let mut classes = vec!["article".to_string()];

    match document.toc_mode() {
        TocMode::Left => {
            classes.push("toc2".to_string());
            classes.push("toc-left".to_string());
        }

        TocMode::Right => {
            classes.push("toc2".to_string());
            classes.push("toc-right".to_string());
        }

        _ => {}
    }

    classes.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_is_not_shown_to_the_reader() {
        for name in [
            "sectnums",
            "icons",
            "toc",
            "source-highlighter",
            "experimental",
            "description",
            "revnumber",
            "author",
            "page-",
        ] {
            assert_eq!(displayed_as(name), None, "`{name}` should not be shown");
        }
    }

    #[test]
    fn a_fact_about_the_document_is() {
        assert_eq!(displayed_as("status"), Some("status"));
        assert_eq!(displayed_as("keywords"), Some("keywords"));
        assert_eq!(displayed_as("page-tags"), Some("tags"));
        assert_eq!(displayed_as("page-last-reviewed"), Some("last-reviewed"));
    }

    #[test]
    fn a_label_reads_as_a_phrase() {
        assert_eq!(label_for("status"), "Status");
        assert_eq!(label_for("last-reviewed"), "Last reviewed");
        assert_eq!(label_for("tags"), "Tags");
    }

    #[test]
    fn a_list_of_tags_becomes_separate_marks() {
        let markup = value_for("tags", "design, flux , ci,,");

        assert_eq!(markup.matches("class=\"tag\"").count(), 3);
        assert!(markup.contains(">design<"));
        assert!(markup.contains(">flux<"));
    }

    #[test]
    fn any_other_value_is_plain_escaped_text() {
        assert_eq!(value_for("status", "in <review>"), "in &lt;review&gt;");
        assert_eq!(value_for("status", "a, b"), "a, b");
    }
}
