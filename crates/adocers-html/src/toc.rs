//! The table of contents.
//!
//! The outline is built from the section tree rather than the document's
//! reference catalog, because the tree is what carries nesting, ordering and
//! the section numbers that appear in the entries.

use asciidoc_parser::{
    attributes::{
        Attrlist,
        ElementAttribute,
    },
    blocks::{
        Block,
        FindBlocks,
        IsBlock,
        SectionBlock,
        SectionType,
        TocBlock,
    },
};

use adocers_render_core::numbering::Numbering;

use crate::{
    Renderer,
    html::{
        Buffer,
        escape_attr,
    },
};

/// One line of the outline, plus the lines nested beneath it.
struct Entry {
    /// The anchor to link to, absent for a section with no id.
    id: Option<String>,

    /// The rendered heading text, including any section number.
    title: String,

    /// Subsections, already filtered to the configured depth.
    children: Vec<Entry>,
}

/// What a `toc::[]` macro asked for, where it differs from the document.
///
/// Everything here is `None` for the outline a document places by attribute;
/// only the macro form can be told to differ.
#[derive(Default)]
struct Overrides<'src> {
    /// The id of the container, and the stem of the title's id.
    id: Option<&'src str>,

    /// The class on the container, which stands in place of `toc` rather than
    /// joining it.
    class: Option<&'src str>,

    /// The heading above the outline.
    title: Option<&'src str>,

    /// How deep the outline goes.
    levels: Option<usize>,

    /// Whether the heading carries the `title` class. Asciidoctor's macro
    /// template writes it and its document template does not.
    title_class: bool,
}

impl<'src> Overrides<'src> {
    /// Read what the macro's own attribute list asked for.
    fn from_macro(attrlist: &'src Attrlist<'src>) -> Self {
        let named = |name: &str| {
            attrlist
                .named_attribute(name)
                .map(ElementAttribute::value)
                .filter(|value| !value.is_empty())
        };

        Self {
            id: named("id"),
            class: named("role"),
            title: named("title"),
            levels: named("levels").and_then(|levels| levels.parse().ok()),
            title_class: true,
        }
    }
}

impl Renderer<'_> {
    /// Render the `#toc` container, if the document has any sections to list.
    pub(super) fn toc(&mut self) {
        if let Some(markup) = self.toc_markup(&Overrides::default()) {
            self.out.raw(&markup);
        }
    }

    /// The outline as markup of its own, for a host that places it itself.
    ///
    /// `None` when the document has no sections to list, which is the same
    /// question [`toc`](Self::toc) answers by writing nothing.
    pub(super) fn outline(&mut self) -> Option<String> {
        self.toc_markup(&Overrides::default())
    }

    /// Render the outline into a buffer of its own, leaving `out` as it was.
    ///
    /// The outline is wanted in two places that cannot share one pass: inside
    /// the body where the document asks for it, and on its own for a host that
    /// lays out its own page. Rendering it aside and copying it in is what
    /// keeps those the same markup rather than two renderers to keep in step.
    fn toc_markup(&mut self, overrides: &Overrides<'_>) -> Option<String> {
        let held = std::mem::replace(&mut self.out, Buffer::new());
        let placed = self.toc_with(overrides);
        let markup = std::mem::replace(&mut self.out, held).finish();

        placed.then_some(markup)
    }

    /// Render the outline, and say whether there turned out to be one.
    fn toc_with(&mut self, overrides: &Overrides<'_>) -> bool {
        let depth = overrides
            .levels
            .or(self.options.toc_levels)
            .unwrap_or_else(|| self.document.toc_levels());
        let entries = entries(self.document.child_blocks(), 1, depth, &self.numbering);

        if entries.is_empty() {
            return false;
        }

        let id = overrides.id.unwrap_or("toc");
        // A docked outline is positioned by a class on `<body>`, which a
        // fragment has not got, so a fragment's outline is always the plain
        // inline one however the document asked for it to be placed.
        let class = overrides.class.map_or_else(
            || {
                if self.options.fragment {
                    "toc".to_string()
                } else {
                    self.document.toc_class().to_string()
                }
            },
            str::to_string,
        );
        let title = overrides
            .title
            .map_or_else(|| self.document.toc_title().to_string(), str::to_string);

        let title_class = if overrides.title_class {
            " class=\"title\""
        } else {
            ""
        };

        self.out.open("div", Some(id), &[&class]);
        self.out.line(&format!(
            "<div id=\"{}title\"{title_class}>{title}</div>",
            escape_attr(id)
        ));
        self.render_entries(&entries, 1);
        self.out.close("div");

        true
    }

    /// Render a `toc::[]` macro.
    ///
    /// The macro only places the outline when the document asked for the macro
    /// placement, and only the first one does so — a second `toc::[]` would
    /// otherwise repeat the entire outline. A macro that places nothing still
    /// leaves a note saying so, which is what Asciidoctor does and is a good
    /// deal easier to debug than an empty space.
    pub(super) fn toc_macro(&mut self, toc: &TocBlock<'_>) {
        let markup = (!self.toc_rendered
            && self.document.toc_mode() == asciidoc_parser::document::TocMode::Macro)
            .then(|| self.toc_markup(&Overrides::from_macro(toc.macro_attrlist())))
            .flatten();

        if let Some(markup) = markup {
            self.out.raw(&markup);
            self.toc_rendered = true;
        } else {
            self.out.line("<!-- toc disabled -->");
        }
    }

    /// Emit one `<ul class="sectlevelN">` and everything under it.
    fn render_entries(&mut self, entries: &[Entry], level: usize) {
        if entries.is_empty() {
            return;
        }

        self.out.open("ul", None, &[&format!("sectlevel{level}")]);

        for entry in entries {
            let item = match &entry.id {
                Some(id) => format!("<li><a href=\"#{}\">{}</a>", escape_attr(id), entry.title),

                // Without an id there is nothing to link to, but the section
                // still belongs in the outline.
                None => format!("<li>{}", entry.title),
            };

            // An entry with nothing under it closes on its own line; one with
            // subsections closes after the list of them.
            if entry.children.is_empty() {
                self.out.line(&format!("{item}</li>"));
            } else {
                self.out.line(&item);
                self.render_entries(&entry.children, level + 1);
                self.out.line("</li>");
            }
        }

        self.out.close("ul");
    }
}

/// Collect the sections at one level of the tree, descending until `depth`.
fn entries<'src>(
    blocks: impl Iterator<Item = &'src Block<'src>>,
    level: usize,
    depth: usize,
    numbering: &Numbering,
) -> Vec<Entry> {
    if level > depth {
        return Vec::new();
    }

    let mut outline = Vec::new();

    for block in blocks {
        // A preamble sits between the header and the first section, so the
        // sections after it are siblings of the preamble, not of its contents;
        // nothing else can contain a section.
        if let Block::Section(section) = block
            && !is_excluded(section)
        {
            outline.push(Entry {
                id: section.id().map(str::to_string),
                title: format!("{}{}", numbering.prefix(section), section.section_title()),
                children: entries(section.child_blocks(), level + 1, depth, numbering),
            });
        }
    }

    outline
}

/// Whether a section is kept out of the outline.
fn is_excluded(section: &SectionBlock<'_>) -> bool {
    // A discrete heading is styled like a section but is not one: it owns no
    // body, so there is nothing for an outline entry to point at.
    section.section_type() == SectionType::Discrete
}
