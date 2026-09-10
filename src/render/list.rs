//! List rendering: unordered, ordered, description and callout lists.

use std::fmt::Write as _;

use asciidoc_parser::blocks::{
    Block,
    IsBlock,
    ListBlock,
    ListItem,
    ListItemMarker,
    ListType,
    SimpleBlockStyle,
};

use crate::render::{
    Renderer,
    block,
    html::escape_attr,
};

impl<'src> Renderer<'src> {
    /// Render a list and its items.
    pub(super) fn list_block(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        match list.type_() {
            ListType::Unordered => self.unordered_list(block, list),
            ListType::Ordered => self.ordered_list(block, list),
            ListType::Callout => self.callout_list(block, list),
            ListType::Description => self.description_list(block, list),
        }
    }

    /// `* item`, including its checklist and bibliography variants.
    fn unordered_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        // A checklist and a bibliography are ordinary unordered lists that
        // carry an extra class, which is what their stylesheet rules key on.
        let variant = if list.is_checklist() {
            Some("checklist")
        } else if list.is_bibliography() {
            Some("bibliography")
        } else {
            // `[square]`, `[circle]`, `[disc]` and `[none]` select a bullet.
            block
                .declared_style()
                .filter(|style| matches!(*style, "square" | "circle" | "disc" | "none"))
        };

        let wrapper = block::wrapper_classes(block, &["ulist", variant.unwrap_or_default()]);

        let wrapper: Vec<&str> = wrapper.iter().map(String::as_str).collect();
        self.out.open("div", block.id(), &wrapper);
        self.block_title(block);

        match variant {
            Some(variant) => self.out.open("ul", None, &[variant]),
            None => self.out.line("<ul>"),
        }

        for item in list_items(list) {
            self.out.line("<li>");
            self.list_item(item, checkbox_marker(item, list.is_checklist()));
            self.out.line("</li>");
        }

        self.out.close("ul");
        self.out.close("div");
    }

    /// `. item`, numbered in the style the marker implies.
    fn ordered_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        // A declared style says outright how the list is numbered; the marker's
        // depth is only the fallback for a list that did not say.
        let style = numeration_style(block)
            .or_else(|| list.marker_style())
            .unwrap_or("arabic");

        let wrapper = block::wrapper_classes(block, &["olist", style]);

        let wrapper: Vec<&str> = wrapper.iter().map(String::as_str).collect();
        self.out.open("div", block.id(), &wrapper);
        self.block_title(block);

        let mut attributes = format!(" class=\"{}\"", escape_attr(style));

        // The `type` attribute is what makes a browser render anything other
        // than arabic numerals; arabic is the default and needs none.
        if let Some(numbering) = html_list_type(style) {
            let _ = write!(attributes, " type=\"{numbering}\"");
        }

        if let Some(start) = list.start() {
            let _ = write!(attributes, " start=\"{start}\"");
        }

        if block.has_option("reversed") {
            attributes.push_str(" reversed");
        }

        self.out.line(&format!("<ol{attributes}>"));

        for item in list_items(list) {
            self.out.line("<li>");
            self.list_item(item, None);
            self.out.line("</li>");
        }

        self.out.close("ol");
        self.out.close("div");
    }

    /// `<1>` callouts, which annotate the listing block above them.
    fn callout_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        let wrapper = block::wrapper_classes(block, &["colist", "arabic"]);

        let wrapper: Vec<&str> = wrapper.iter().map(String::as_str).collect();
        self.out.open("div", block.id(), &wrapper);
        self.block_title(block);
        self.out.line("<ol>");

        for item in list_items(list) {
            self.out.line("<li>");
            self.list_item(item, None);
            self.out.line("</li>");
        }

        self.out.close("ol");
        self.out.close("div");
    }

    /// `term:: description`, in its default, horizontal and Q&A forms.
    fn description_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        match block.declared_style() {
            Some("horizontal") => self.horizontal_list(block, list),
            Some("qanda") => self.qanda_list(block, list),
            _ => self.definition_list(block, list),
        }
    }

    /// The default `<dl>` rendering of a description list.
    fn definition_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        self.open_wrapper(block, "dlist");
        self.block_title(block);
        self.out.line("<dl>");

        for item in list_items(list) {
            self.out
                .line(&format!("<dt class=\"hdlist1\">{}</dt>", term_of(item)));

            // A term with nothing after it is a legitimate entry — a glossary
            // stub, or a term whose description follows on a later line — and
            // an empty `<dd>` would be worse than none.
            if has_content(item) {
                self.out.line("<dd>");
                self.list_item(item, None);
                self.out.line("</dd>");
            }
        }

        self.out.close("dl");
        self.out.close("div");
    }

    /// `[horizontal]`, where terms and descriptions sit side by side.
    fn horizontal_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        self.open_wrapper(block, "hdlist");
        self.block_title(block);
        self.out.line("<table>");

        // `labelwidth` and `itemwidth` size the two columns. Either alone is
        // enough to fix the table's shape; the other column is then the
        // browser's to measure.
        let label = column_width(block, "labelwidth");
        let item = column_width(block, "itemwidth");

        if label.is_some() || item.is_some() {
            self.out.line("<colgroup>");
            self.out.line(&col(label.as_deref()));
            self.out.line(&col(item.as_deref()));
            self.out.close("colgroup");
        }

        for item in list_items(list) {
            self.out.line("<tr>");
            self.out.line("<td class=\"hdlist1\">");
            self.out.line(term_of(item).as_str());
            self.out.line("</td>");
            self.out.line("<td class=\"hdlist2\">");
            self.list_item(item, None);
            self.out.line("</td>");
            self.out.line("</tr>");
        }

        self.out.close("table");
        self.out.close("div");
    }

    /// `[qanda]`, where each term is a numbered question.
    fn qanda_list(&mut self, block: &'src Block<'src>, list: &'src ListBlock<'src>) {
        let wrapper = block::wrapper_classes(block, &["qlist", "qanda"]);

        let wrapper: Vec<&str> = wrapper.iter().map(String::as_str).collect();
        self.out.open("div", block.id(), &wrapper);
        self.block_title(block);
        self.out.line("<ol>");

        for item in list_items(list) {
            self.out.line("<li>");
            self.out.line(&format!("<p><em>{}</em></p>", term_of(item)));
            self.list_item(item, None);
            self.out.line("</li>");
        }

        self.out.close("ol");
        self.out.close("div");
    }

    /// The body of one list item: its principal text as a paragraph, followed
    /// by any blocks attached to it with a `+` continuation.
    fn list_item(&mut self, item: &'src ListItem<'src>, checkbox: Option<&str>) {
        let children: Vec<&'src Block<'src>> = item.child_blocks().collect();
        let (principal, rest) = split_principal(&children);

        if let Some(text) = principal {
            match checkbox {
                Some(checkbox) => self.out.line(&format!("<p>{checkbox} {text}</p>")),
                None => self.out.line(&format!("<p>{text}</p>")),
            }
        }

        self.blocks(rest.iter().copied());
    }
}

/// The list's items, skipping anything that is somehow not a list item.
fn list_items<'src>(list: &'src ListBlock<'src>) -> impl Iterator<Item = &'src ListItem<'src>> {
    list.child_blocks().filter_map(|block| match block {
        Block::ListItem(item) => Some(item),
        _ => None,
    })
}

/// Split a list item's children into its principal text and the blocks that
/// follow.
///
/// The principal text is the item's own words — the paragraph the parser builds
/// from the text after the marker. It renders inside the `<p>` that carries the
/// checkbox, so it has to be told apart from a block that was attached with a
/// `+` continuation, which renders normally beneath it.
fn split_principal<'a, 'src>(
    children: &'a [&'src Block<'src>],
) -> (Option<&'src str>, &'a [&'src Block<'src>]) {
    if let Some((first, tail)) = children.split_first()
        && let Block::Simple(simple) = *first
        && simple.style() == SimpleBlockStyle::Paragraph
        && simple.title().is_none()
    {
        return (Some(simple.content().rendered_html()), tail);
    }

    (None, children)
}

/// Whether a list item has anything to show beneath its term.
fn has_content(item: &ListItem<'_>) -> bool {
    item.child_blocks().next().is_some()
}

/// The rendered term of a description-list item.
///
/// The marker is handed back by value, so the term has to be copied out of it
/// rather than borrowed through it.
fn term_of(item: &ListItem<'_>) -> String {
    match &item.list_item_marker() {
        ListItemMarker::DefinedTerm { term, .. } => term.rendered_html().to_string(),
        _ => String::new(),
    }
}

/// The tick or ballot box that stands in for a checkbox in a checklist.
fn checkbox_marker(item: &ListItem<'_>, is_checklist: bool) -> Option<&'static str> {
    if !is_checklist {
        return None;
    }

    // Only an item that carries a checkbox gets a box drawn for it. An item
    // written without one is an ordinary item that happens to share the list,
    // and marking it unchecked would say something the author did not.
    match item.checkbox() {
        Some(true) => Some("&#10003;"),
        Some(false) => Some("&#10063;"),
        None => None,
    }
}

/// A `[horizontal]` list's column width, as a percentage without its sign.
///
/// The author may write either `labelwidth=20` or `labelwidth="20%"`; both mean
/// the same fifth of the table.
fn column_width<'src>(block: &'src Block<'src>, name: &str) -> Option<String> {
    let width = block
        .attrlist()?
        .named_attribute(name)
        .map(asciidoc_parser::attributes::ElementAttribute::value)
        .filter(|width| !width.is_empty())?;

    Some(width.trim_end_matches('%').to_string())
}

/// One `<col>`, sized when there is a width for it.
fn col(width: Option<&str>) -> String {
    match width {
        Some(width) => format!("<col style=\"width: {}%;\">", escape_attr(width)),
        None => "<col>".to_string(),
    }
}

/// The numbering an ordered list declared for itself, if it declared one.
fn numeration_style<'src>(block: &'src Block<'src>) -> Option<&'src str> {
    block.declared_style().filter(|style| {
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
}

/// The HTML `type` that reproduces an AsciiDoc numbering style.
fn html_list_type(style: &str) -> Option<&'static str> {
    match style {
        "loweralpha" => Some("a"),
        "upperalpha" => Some("A"),
        "lowerroman" => Some("i"),
        "upperroman" => Some("I"),
        _ => None,
    }
}
