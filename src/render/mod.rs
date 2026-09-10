//! The HTML5 back end.
//!
//! `asciidoc-parser` renders *inline* content — bold, links, cross-references,
//! passthroughs — but deliberately stops there: turning the block tree into a
//! page is the back end's job, and this module is that back end. The markup it
//! emits follows Asciidoctor's HTML5 converter (the same wrapper `div`s and
//! class names), so stylesheets written for Asciidoctor apply unchanged.

mod block;
mod css;
mod html;
mod list;
mod media;
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

use crate::render::html::{
    Buffer,
    escape_attr,
    escape_text,
};

/// How the rendered body should be wrapped.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Emit only the body content, with no surrounding HTML page.
    pub fragment: bool,

    /// CSS to embed in the page, or `None` for an unstyled page.
    ///
    /// Ignored when [`fragment`](Self::fragment) is set, since a fragment has
    /// no `<head>` to put it in.
    pub stylesheet: Option<String>,
}

/// The stylesheet embedded in a standalone page when the caller names no other.
pub fn default_stylesheet() -> String {
    css::DEFAULT.to_string()
}

/// Render `document` to HTML.
pub fn render<'src>(document: &'src Document<'src>, options: &Options) -> String {
    let mut renderer = Renderer {
        document,
        out: Buffer::new(),
        fragment: options.fragment,
        toc_rendered: false,
    };

    let body = renderer.body();

    if options.fragment {
        body
    } else {
        page(document, options, &body)
    }
}

/// Walks the block tree, appending markup as it goes.
struct Renderer<'src> {
    /// The document being rendered; consulted for attributes and TOC settings.
    document: &'src Document<'src>,

    /// Markup accumulated so far.
    out: Buffer,

    /// Whether the output is destined for embedding in another page.
    fragment: bool,

    /// Whether a `toc::[]` macro has already emitted the table of contents.
    ///
    /// Asciidoctor renders the macro form at most once; a second `toc::[]` is
    /// ignored rather than duplicating the whole outline.
    toc_rendered: bool,
}

impl<'src> Renderer<'src> {
    /// Render the document header, body and footer into a single body fragment.
    ///
    /// An embedded fragment drops the page furniture — the `#header` and
    /// `#footer` wrappers and the `#content` div — because the page it is
    /// pasted into supplies its own, and a "Last updated" line belongs to a
    /// standalone document rather than to a fragment of one. This matches what
    /// Asciidoctor emits with `--no-header-footer`.
    fn body(&mut self) -> String {
        if self.fragment {
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

        let subtitle = match self.document.subtitle() {
            Some(subtitle) => format!(": <span class=\"subtitle\">{subtitle}</span>"),
            None => String::new(),
        };

        // Both halves are already inline-rendered by the parser.
        self.out.line(&format!("<h1>{title}{subtitle}</h1>"));
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
    fn details(&self) -> String {
        let mut details = Buffer::new();

        for (index, author) in self.document.authors().iter().enumerate() {
            // Asciidoctor numbers every author after the first; the first is
            // plain `author`/`email` so existing stylesheets keep working.
            let suffix = if index == 0 {
                String::new()
            } else {
                (index + 1).to_string()
            };

            details.line(&format!(
                "<span id=\"author{suffix}\" class=\"author\">{}</span><br>",
                escape_text(author.name())
            ));

            if let Some(email) = author.email() {
                details.line(&format!(
                    "<span id=\"email{suffix}\" class=\"email\"><a \
                     href=\"mailto:{0}\">{1}</a></span><br>",
                    escape_attr(email),
                    escape_text(email)
                ));
            }
        }

        if let Some(revision) = self.document.header().revision_line() {
            if let Some(number) = revision.revnumber() {
                details.line(&format!(
                    "<span id=\"revnumber\">version {},</span>",
                    escape_text(number)
                ));
            }

            let date = revision.revdate();
            if !date.is_empty() {
                details.line(&format!(
                    "<span id=\"revdate\">{}</span>",
                    escape_text(date)
                ));
            }

            if let Some(remark) = revision.revremark() {
                details.line(&format!(
                    "<br><span id=\"revremark\">{}</span>",
                    escape_text(remark)
                ));
            }
        }

        details.finish()
    }

    /// Render every top-level block, honouring the TOC placements that fall
    /// inside the content area.
    fn content(&mut self) {
        let toc_mode = self.document.toc_mode();

        for block in self.document.child_blocks() {
            let is_preamble = matches!(block, asciidoc_parser::blocks::Block::Preamble(_));

            self.block(block);

            if is_preamble && toc_mode == TocMode::Preamble {
                self.toc();
            }
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

            self.out
                .open("div", Some(&format!("_footnotedef_{index}")), &["footnote"]);
            self.out.line(&format!(
                "<a href=\"#_footnoteref_{index}\">{}</a>. {}",
                escape_text(&footnote.index),
                footnote.text
            ));
            self.out.close("div");
        }

        self.out.close("div");
    }

    /// Render `#footer`, which carries the `last-updated-label` line when the
    /// document has a modification date to show.
    fn footer(&mut self) {
        if self.document.is_attribute_set("nofooter") {
            return;
        }

        let Some(date) = self
            .attribute("docdatetime")
            .or_else(|| self.attribute("localdatetime"))
        else {
            return;
        };

        let label = self
            .attribute("last-update-label")
            .unwrap_or_else(|| "Last updated".to_string());

        self.out.open("div", Some("footer"), &[]);
        self.out.open("div", Some("footer-text"), &[]);
        self.out
            .line(&format!("{} {}", escape_text(&label), escape_text(&date)));
        self.out.close("div");
        self.out.close("div");
    }

    /// The value of a document attribute, if it is set to one.
    fn attribute(&self, name: &str) -> Option<String> {
        match self.document.attribute_value(name) {
            InterpretedValue::Value(value) => Some(value),
            _ => None,
        }
    }
}

/// Whether a TOC placement puts the outline above the document's content.
fn toc_precedes_content(mode: TocMode) -> bool {
    matches!(
        mode,
        TocMode::Auto | TocMode::Left | TocMode::Right | TocMode::Top
    )
}

/// Wrap a rendered body in a complete HTML page.
fn page(document: &Document<'_>, options: &Options, body: &str) -> String {
    let mut out = Buffer::new();

    let lang = match document.attribute_value("lang") {
        InterpretedValue::Value(lang) => lang,
        _ => "en".to_string(),
    };

    out.line("<!DOCTYPE html>");
    out.line(&format!("<html lang=\"{}\">", escape_attr(&lang)));
    out.line("<head>");
    out.line("<meta charset=\"UTF-8\">");
    out.line("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">");
    out.line(&format!(
        "<meta name=\"generator\" content=\"adocers {}\">",
        env!("CARGO_PKG_VERSION")
    ));

    if let InterpretedValue::Value(description) = document.attribute_value("description") {
        out.line(&format!(
            "<meta name=\"description\" content=\"{}\">",
            escape_attr(&description)
        ));
    }

    // The `<title>` is plain text, so any markup the inline renderer produced
    // for the doctitle has to come back out.
    let title = document
        .doctitle_sanitized()
        .unwrap_or_else(|| "Untitled".to_string());

    out.line(&format!("<title>{}</title>", escape_text(&title)));

    if let Some(css) = &options.stylesheet {
        out.line("<style>");
        out.line(css);
        out.line("</style>");
    }

    out.line("</head>");
    out.line(&format!(
        "<body class=\"{}\">",
        escape_attr(&body_classes(document))
    ));
    out.raw(body);
    out.newline();
    out.line("</body>");
    out.line("</html>");

    out.finish()
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
