//! Block-level rendering: the dispatch from a parsed [`Block`] to its markup.

use asciidoc_parser::{
    blocks::{
        AdmonitionBlock,
        Block,
        Break,
        BreakType,
        CompoundDelimitedBlock,
        CompoundDelimitedContext,
        IsBlock,
        Preamble,
        QuoteBlock,
        QuoteType,
        RawDelimitedBlock,
        SectionBlock,
        SectionType,
        SimpleBlock,
        SimpleBlockStyle,
    },
    content::Content,
};

use crate::render::{
    Renderer,
    callout,
    highlight,
    html::{
        escape_attr,
        escape_text,
    },
    icons,
};

impl<'src> Renderer<'src> {
    /// Render one block and everything beneath it.
    #[expect(
        clippy::match_same_arms,
        reason = "the arms that produce nothing do so for different reasons, and saying which is \
                  the point of listing them separately"
    )]
    pub(super) fn block(&mut self, block: &'src Block<'src>) {
        match block {
            Block::Simple(simple) => self.simple_block(block, simple),
            Block::Section(section) => self.section_block(block, section),
            Block::List(list) => self.list_block(block, list),
            Block::RawDelimited(raw) => self.raw_delimited_block(block, raw),
            Block::CompoundDelimited(compound) => self.compound_delimited_block(block, compound),
            Block::Admonition(admonition) => self.admonition_block(block, admonition),
            Block::Quote(quote) => self.quote_block(block, quote),
            Block::Media(media) => self.media_block(block, media),
            Block::Table(table) => self.table_block(block, table),
            Block::Preamble(preamble) => self.preamble_block(preamble),
            Block::Break(r#break) => self.break_block(r#break),
            Block::Toc(_) => self.toc_macro(),

            // A list item never appears on its own: its list renders it, along
            // with the marker that gives it meaning.
            Block::ListItem(_) => {}

            // An attribute entry in the body sets a value for the blocks that
            // follow it; the parser has already applied it and it has no
            // rendering of its own.
            Block::DocumentAttribute(_) => {}

            _ => {}
        }
    }

    /// Render a sequence of blocks in document order.
    pub(super) fn blocks(&mut self, blocks: impl Iterator<Item = &'src Block<'src>>) {
        for block in blocks {
            self.block(block);
        }
    }

    /// Emit a block's `<div class="title">`, including any caption prefix
    /// ("Example 1. ", "Figure 2. ") the parser assigned.
    pub(super) fn block_title(&mut self, block: &'src Block<'src>) {
        let Some(title) = block.title() else {
            return;
        };

        let caption = block.caption().unwrap_or_default();

        // Both halves are already inline-rendered; escaping here would show the
        // reader the markup instead of applying it.
        self.out
            .line(&format!("<div class=\"title\">{caption}{title}</div>"));
    }

    /// Open a block wrapper `<div>` with the block's id and class list.
    pub(super) fn open_wrapper(&mut self, block: &'src Block<'src>, context: &str) {
        let classes = wrapper_classes(block, context);
        let classes: Vec<&str> = classes.iter().map(String::as_str).collect();

        self.out.open("div", block.id(), &classes);
    }

    /// A paragraph, or a listing/literal block written without delimiters.
    fn simple_block(&mut self, block: &'src Block<'src>, simple: &'src SimpleBlock<'src>) {
        let content = simple.content().rendered_html();

        match simple.style() {
            SimpleBlockStyle::Paragraph => {
                self.open_wrapper(block, "paragraph");
                self.block_title(block);
                self.out.line(&format!("<p>{content}</p>"));
                self.out.close("div");
            }

            SimpleBlockStyle::Literal => self.literal(block, content),
            SimpleBlockStyle::Listing | SimpleBlockStyle::Source => {
                self.listing(block, simple.content());
            }
        }
    }

    /// A `----` listing, `....` literal, `++++` passthrough, `////` comment or
    /// stem block. Their content is verbatim, already substituted to the degree
    /// the block's substitution group calls for.
    fn raw_delimited_block(
        &mut self,
        block: &'src Block<'src>,
        raw: &'src RawDelimitedBlock<'src>,
    ) {
        let content = raw.content().rendered_html();

        match raw.raw_context().as_ref() {
            "listing" => self.listing(block, raw.content()),
            "literal" => self.literal(block, content),

            "stem" => {
                self.open_wrapper(block, "stemblock");
                self.block_title(block);
                self.out.open("div", None, &["content"]);
                self.out.line(content);
                self.out.close("div");
                self.out.close("div");
            }

            // A passthrough block is the author asking for their bytes to reach
            // the page untouched.
            "pass" => self.out.line(content),

            // A comment block is not part of the output at all.
            "comment" => {}

            other => {
                self.open_wrapper(block, other);
                self.block_title(block);
                self.out.line(content);
                self.out.close("div");
            }
        }
    }

    /// `<div class="listingblock">`, syntax highlighted when the block declares
    /// a language a compiled-in grammar covers.
    fn listing(&mut self, block: &'src Block<'src>, content: &'src Content<'src>) {
        let rendered = content.rendered_html();

        if self.options.mermaid.is_some() && is_mermaid(block) {
            self.mermaid_block(block, rendered);
            return;
        }

        self.open_wrapper(block, "listingblock");
        self.block_title(block);
        self.out.open("div", None, &["content"]);

        match source_language(block) {
            Some(language) => {
                let body = self
                    .highlighted(language, content)
                    .unwrap_or_else(|| rendered.to_string());
                let language = escape_attr(language);

                self.out.line(&format!(
                    "<pre class=\"highlight\"><code class=\"language-{language}\" \
                     data-lang=\"{language}\">{body}</code></pre>"
                ));
            }

            None => self.out.line(&format!("<pre>{rendered}</pre>")),
        }

        self.out.close("div");
        self.out.close("div");
    }

    /// Highlight a listing's source, if this build can and the block asked for
    /// a language it knows.
    ///
    /// The highlighter is given the block's original text, not the parser's
    /// rendering of it: the rendering is already escaped, and a highlighter
    /// needs the code as the author wrote it. Escaping is then the
    /// highlighter's job, which it does as it emits.
    fn highlighted(&self, language: &str, content: &'src Content<'src>) -> Option<String> {
        if !self.options.highlight {
            return None;
        }

        let source = content.original().data();

        // Callout markers are the parser's rendering, not the author's code, so
        // they come out before the highlighter sees them and go back after.
        // When the two readings of the source disagree, the block is left to
        // the parser entirely rather than highlighted with its callouts lost.
        let callouts = callout::locate(source, content.rendered_html())?;
        let stripped = callout::strip(source, &callouts);
        let highlighted = highlight::highlight(language, &stripped)?;

        Some(callout::reapply(&highlighted, &callouts))
    }

    /// `<div class="literalblock">`.
    fn literal(&mut self, block: &'src Block<'src>, content: &str) {
        if self.options.mermaid.is_some() && is_mermaid(block) {
            self.mermaid_block(block, content);
            return;
        }

        self.open_wrapper(block, "literalblock");
        self.block_title(block);
        self.out.open("div", None, &["content"]);
        self.out.line(&format!("<pre>{content}</pre>"));
        self.out.close("div");
        self.out.close("div");
    }

    /// A mermaid diagram, handed to the browser as its own source to draw.
    ///
    /// The wrapper carries `imageblock` so that a stylesheet written for
    /// Asciidoctor centres the diagram and puts its caption underneath, the way
    /// it would for a diagram that had been rendered to an image.
    fn mermaid_block(&mut self, block: &'src Block<'src>, content: &str) {
        self.diagrams = true;

        let classes = wrapper_classes(block, "imageblock");
        let mut classes: Vec<&str> = classes.iter().map(String::as_str).collect();
        classes.insert(1, "diagram");

        self.out.open("div", block.id(), &classes);
        self.out.open("div", None, &["content"]);

        // `content` is already escaped, which is what mermaid needs: the
        // browser turns `--&gt;` back into `-->` when the script reads the
        // element's text, and an unescaped `<` would have ended the element.
        self.out
            .line(&format!("<pre class=\"mermaid\">{content}</pre>"));

        self.out.close("div");

        // A diagram's caption sits below it, as an image's does.
        self.block_title(block);
        self.out.close("div");
    }

    /// A section heading and its body.
    ///
    /// Asciidoctor gives a level-1 section an extra `sectionbody` wrapper and
    /// deeper sections none, so this reproduces that asymmetry rather than
    /// inventing a uniform structure a stylesheet would not expect.
    fn section_block(&mut self, block: &'src Block<'src>, section: &'src SectionBlock<'src>) {
        let level = section.level();
        let heading = format!("h{}", (level + 1).min(6));
        let title = format!("{}{}", section_prefix(section), section.section_title());

        if section.section_type() == SectionType::Discrete {
            // A discrete heading is a heading and nothing else: it owns no body
            // and never enters the table of contents.
            let mut classes = vec!["discrete".to_string()];
            classes.extend(block.roles().into_iter().map(str::to_string));
            let classes: Vec<&str> = classes.iter().map(String::as_str).collect();

            self.out.element(&heading, section.id(), &classes, &title);
            return;
        }

        if level == 0 {
            // A level-0 heading in the body is not a section wrapper of its own.
            self.out.element("h1", section.id(), &["sect0"], &title);
            self.blocks(section.child_blocks());
            return;
        }

        // The id goes on the heading, which is what a link to the section
        // should scroll to; putting it on the wrapper as well would make the
        // document contain the same id twice.
        let classes = wrapper_classes(block, &format!("sect{level}"));
        let classes: Vec<&str> = classes.iter().map(String::as_str).collect();

        self.out.open("div", None, &classes);
        self.out.element(&heading, section.id(), &[], &title);

        if level == 1 {
            self.out.open("div", None, &["sectionbody"]);
            self.blocks(section.child_blocks());
            self.out.close("div");
        } else {
            self.blocks(section.child_blocks());
        }

        self.out.close("div");
    }

    /// The preamble: everything between the document header and the first
    /// section.
    fn preamble_block(&mut self, preamble: &'src Preamble<'src>) {
        self.out.open("div", Some("preamble"), &[]);
        self.out.open("div", None, &["sectionbody"]);
        self.blocks(preamble.child_blocks());
        self.out.close("div");
        self.out.close("div");
    }

    /// `'''` and `<<<`.
    fn break_block(&mut self, r#break: &'src Break<'src>) {
        match r#break.type_() {
            BreakType::Thematic => self.out.line("<hr>"),

            // There is no page break in HTML, only a hint for print styles.
            BreakType::Page => self
                .out
                .line("<div style=\"page-break-after: always;\"></div>"),
        }
    }

    /// `NOTE:`, `TIP:`, `IMPORTANT:`, `CAUTION:` and `WARNING:`, in both their
    /// paragraph and delimited forms.
    fn admonition_block(
        &mut self,
        block: &'src Block<'src>,
        admonition: &'src AdmonitionBlock<'src>,
    ) {
        let name = admonition.name();
        let label = admonition.label().to_string();

        let mut classes = vec!["admonitionblock".to_string(), name.to_string()];
        classes.extend(block.roles().into_iter().map(str::to_string));
        let classes: Vec<&str> = classes.iter().map(String::as_str).collect();

        self.out.open("div", block.id(), &classes);
        self.out.line("<table>");
        self.out.line("<tr>");
        self.out.line("<td class=\"icon\">");

        // The icon carries the label as its accessible name, so nothing is lost
        // by drawing one instead of writing the other. A variant with no icon
        // of its own falls back to the label rather than to an empty column.
        //
        // Asciidoctor's `icons=font` markup is not emitted: it names Font
        // Awesome classes, and a page this tool produced has no way to pull in
        // that webfont, so the column came out blank.
        let icon = self
            .options
            .icons
            .then(|| icons::admonition(name, &label))
            .flatten();

        match icon {
            Some(icon) => self.out.line(&icon),

            None => self.out.line(&format!(
                "<div class=\"title\">{}</div>",
                escape_text(&label)
            )),
        }

        self.out.line("</td>");
        self.out.line("<td class=\"content\">");

        self.block_title(block);

        match admonition.content() {
            Some(content) => self.out.line(content.rendered_html()),
            None => self.blocks(admonition.child_blocks()),
        }

        self.out.line("</td>");
        self.out.line("</tr>");
        self.out.line("</table>");
        self.out.close("div");
    }

    /// `[quote]` and `[verse]`, delimited or not.
    fn quote_block(&mut self, block: &'src Block<'src>, quote: &'src QuoteBlock<'src>) {
        let is_verse = quote.type_() == QuoteType::Verse;
        let wrapper = if is_verse { "verseblock" } else { "quoteblock" };

        self.open_wrapper(block, wrapper);
        self.block_title(block);

        if is_verse {
            // A verse keeps the author's line breaks, so its content is
            // preformatted rather than flowed.
            let content = quote
                .content()
                .map(asciidoc_parser::content::Content::rendered_html)
                .unwrap_or_default();
            self.out
                .line(&format!("<pre class=\"content\">{content}</pre>"));
        } else {
            self.out.line("<blockquote>");

            match quote.content() {
                Some(content) => self.out.line(content.rendered_html()),
                None => self.blocks(quote.child_blocks()),
            }

            self.out.line("</blockquote>");
        }

        self.attribution(quote);
        self.out.close("div");
    }

    /// The `— Author, Work` line beneath a quote or verse.
    fn attribution(&mut self, quote: &'src QuoteBlock<'src>) {
        let attribution = quote.attribution();
        let citetitle = quote.citetitle();

        if attribution.is_none() && citetitle.is_none() {
            return;
        }

        self.out.open("div", None, &["attribution"]);

        match (attribution, citetitle) {
            (Some(who), Some(what)) => {
                self.out.line(&format!("&#8212; {who}<br>"));
                self.out.line(&format!("<cite>{what}</cite>"));
            }

            (Some(who), None) => self.out.line(&format!("&#8212; {who}")),
            (None, Some(what)) => self.out.line(&format!("<cite>{what}</cite>")),
            (None, None) => unreachable!("guarded above"),
        }

        self.out.close("div");
    }

    /// `====` example, `****` sidebar and `--` open blocks — the three
    /// delimited forms whose content is itself a sequence of blocks.
    fn compound_delimited_block(
        &mut self,
        block: &'src Block<'src>,
        compound: &'src CompoundDelimitedBlock<'src>,
    ) {
        match compound.context_kind() {
            CompoundDelimitedContext::Example => {
                // `%collapsible` turns an example into a disclosure widget, and
                // its title becomes the summary rather than a heading above it.
                if block.has_option("collapsible") {
                    self.open_wrapper(block, "exampleblock");

                    let open = if block.has_option("open") {
                        " open"
                    } else {
                        ""
                    };
                    let summary = block.title().unwrap_or("Details");

                    self.out.line(&format!("<details{open}>"));
                    self.out
                        .line(&format!("<summary class=\"title\">{summary}</summary>"));
                    self.out.open("div", None, &["content"]);
                    self.blocks(compound.child_blocks());
                    self.out.close("div");
                    self.out.line("</details>");
                    self.out.close("div");
                    return;
                }

                self.open_wrapper(block, "exampleblock");
                self.block_title(block);
                self.out.open("div", None, &["content"]);
                self.blocks(compound.child_blocks());
                self.out.close("div");
                self.out.close("div");
            }

            CompoundDelimitedContext::Sidebar => {
                // A sidebar's title lives inside its content box, not above it.
                self.open_wrapper(block, "sidebarblock");
                self.out.open("div", None, &["content"]);
                self.block_title(block);
                self.blocks(compound.child_blocks());
                self.out.close("div");
                self.out.close("div");
            }

            CompoundDelimitedContext::Open => {
                self.open_wrapper(block, "openblock");
                self.block_title(block);
                self.out.open("div", None, &["content"]);
                self.blocks(compound.child_blocks());
                self.out.close("div");
                self.out.close("div");
            }
        }
    }
}

/// The class list for a block wrapper: the context class first, then any roles
/// the author attached.
pub(super) fn wrapper_classes<'src>(block: &'src Block<'src>, context: &str) -> Vec<String> {
    let mut classes = vec![context.to_string()];
    classes.extend(block.roles().into_iter().map(str::to_string));
    classes
}

/// The numbering that precedes a section title, if the document numbers
/// sections.
///
/// The table of contents shows the same prefix as the heading does, so both
/// come from here.
pub(super) fn section_prefix<'src>(section: &'src SectionBlock<'src>) -> String {
    // An appendix carries a full caption ("Appendix A: "); an ordinary numbered
    // section carries only its number.
    if let Some(caption) = section.caption() {
        return caption.to_string();
    }

    match section.section_number() {
        Some(number) => format!("{number}. "),
        None => String::new(),
    }
}

/// Whether a block was written as a mermaid diagram.
///
/// Both spellings in circulation are recognized: `[mermaid]`, which Antora and
/// `asciidoctor-diagram` use, and `[source,mermaid]`, which is what renders as
/// a diagram on GitHub. A document that has to serve both usually picks between
/// them with an attribute, so a renderer that took only one would show the
/// other as a wall of arrows.
fn is_mermaid<'src>(block: &'src Block<'src>) -> bool {
    let declared = block
        .declared_style()
        .is_some_and(|style| style.to_lowercase() == "mermaid");

    declared || source_language(block).is_some_and(|language| language.to_lowercase() == "mermaid")
}

/// The language a listing block declares, from `[source,rust]` or
/// `[source,language=rust]`.
fn source_language<'src>(block: &'src Block<'src>) -> Option<&'src str> {
    if block.declared_style() != Some("source") {
        return None;
    }

    let attrlist = block.attrlist()?;

    attrlist
        .named_attribute("language")
        .or_else(|| attrlist.nth_attribute(2))
        .map(asciidoc_parser::attributes::ElementAttribute::value)
        .filter(|language| !language.is_empty())
}
