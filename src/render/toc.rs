//! The table of contents.
//!
//! The outline is built from the section tree rather than the document's
//! reference catalog, because the tree is what carries nesting, ordering and
//! the section numbers that appear in the entries.

use asciidoc_parser::blocks::{
    Block,
    FindBlocks,
    IsBlock,
    SectionBlock,
    SectionType,
};

use crate::render::{
    Renderer,
    block,
    html::escape_attr,
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

impl Renderer<'_> {
    /// Render the `#toc` container, if the document has any sections to list.
    pub(super) fn toc(&mut self) {
        let depth = self.document.toc_levels();
        let entries = entries(self.document.child_blocks(), 1, depth);

        if entries.is_empty() {
            return;
        }

        let class = self.document.toc_class().to_string();
        let title = self.document.toc_title().to_string();

        self.out.open("div", Some("toc"), &[&class]);
        self.out
            .line(&format!("<div id=\"toctitle\">{title}</div>"));
        self.render_entries(&entries, 1);
        self.out.close("div");
    }

    /// Render a `toc::[]` macro.
    ///
    /// The macro only places the outline when the document asked for the macro
    /// placement, and only the first one does so — a second `toc::[]` would
    /// otherwise repeat the entire outline.
    pub(super) fn toc_macro(&mut self) {
        if self.toc_rendered
            || self.document.toc_mode() != asciidoc_parser::document::TocMode::Macro
        {
            return;
        }

        self.toc_rendered = true;
        self.toc();
    }

    /// Emit one `<ul class="sectlevelN">` and everything under it.
    fn render_entries(&mut self, entries: &[Entry], level: usize) {
        if entries.is_empty() {
            return;
        }

        self.out.open("ul", None, &[&format!("sectlevel{level}")]);

        for entry in entries {
            match &entry.id {
                Some(id) => self.out.line(&format!(
                    "<li><a href=\"#{}\">{}</a>",
                    escape_attr(id),
                    entry.title
                )),

                // Without an id there is nothing to link to, but the section
                // still belongs in the outline.
                None => self.out.line(&format!("<li>{}", entry.title)),
            }

            self.render_entries(&entry.children, level + 1);
            self.out.line("</li>");
        }

        self.out.close("ul");
    }
}

/// Collect the sections at one level of the tree, descending until `depth`.
fn entries<'src>(
    blocks: impl Iterator<Item = &'src Block<'src>>,
    level: usize,
    depth: usize,
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
                title: format!(
                    "{}{}",
                    block::section_prefix(section),
                    section.section_title()
                ),
                children: entries(section.child_blocks(), level + 1, depth),
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
