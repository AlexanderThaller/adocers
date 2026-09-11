//! Checks the parser does not make.
//!
//! The parser warns when a document is ambiguous or broken. A [`Lint`] is for
//! a document that parsed exactly as written and still probably does not say
//! what its author meant — the shape of the source gives away the intent, and
//! the parse result quietly disagrees with it. Each check here names one such
//! shape. A lint is reported like a parser warning, with the same source
//! anchoring, and counts as one for `--deny-warnings`.

use std::path::{
    Path,
    PathBuf,
};

use asciidoc_parser::{
    Document,
    HasSpan,
    Span,
    blocks::{
        Block,
        BlockSelector,
        FindBlocks,
        IsBlock,
        MediaType,
    },
    document::InterpretedValue,
    inlines::InlineNode,
};

/// One thing adocers has to say about a document beyond what the parser said.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lint<'src> {
    /// Where the problem is; indexes the preprocessed document source, like a
    /// parser warning's span.
    pub source: Span<'src>,

    /// A short, stable identifier, shown in brackets before the message.
    pub code: &'static str,

    /// What is wrong, as a sentence.
    pub message: String,

    /// What to do about it.
    pub help: String,
}

/// Run every check over `document`, in source order.
///
/// `base` is the directory the document lives in, which the checks that look
/// at the file system resolve relative paths against. A document parsed from
/// a string has no directory; pass `None` and those checks are skipped.
pub fn check<'src>(document: &'src Document<'src>, base: Option<&Path>) -> Vec<Lint<'src>> {
    let mut lints: Vec<Lint<'src>> = attributes_set_after_the_header(document).collect();

    if let Some(base) = base {
        lints.extend(images_that_are_not_there(document, base));
    }

    lints.sort_by_key(|lint| lint.source.byte_offset());
    lints
}

/// Attribute entries that sit at the top of the body, just after the header
/// ended.
///
/// The header ends at the first blank line, so an entry written after one is
/// a body attribute: it still sets its value, but the header — and everything
/// that reads the header, such as the details shown under the title — no
/// longer has it. A document with a header and then a blank line and then
/// `:status: draft` almost always meant the entry to be part of the header,
/// and nothing about the rendered page says why it is missing. Entries that
/// come later, between blocks, are left alone: set between two paragraphs, an
/// attribute is where its author put it on purpose.
fn attributes_set_after_the_header<'src>(
    document: &'src Document<'src>,
) -> impl Iterator<Item = Lint<'src>> {
    // Only a title makes the blank line matter. Without one the parser keeps
    // reading attribute entries into the header past blank lines, as
    // Asciidoctor does, so a title-less document never has a leading body
    // attribute that was meant for the header.
    let has_header = document.header().title().is_some();

    // With sections in the document, everything before the first one is
    // wrapped in a preamble, and the entries in question are its first
    // children rather than the document's.
    let leading = match document.child_blocks().next() {
        Some(preamble @ Block::Preamble(_)) => preamble.child_blocks(),
        _ => document.child_blocks(),
    };

    leading
        .take_while(|block| matches!(block, Block::DocumentAttribute(_)))
        .filter(move |_| has_header)
        .filter_map(|block| match block {
            Block::DocumentAttribute(attribute) => Some(attribute),
            _ => None,
        })
        .map(|attribute| Lint {
            source: attribute.span(),
            code: "AttributeSetAfterHeader",
            message: format!(
                "`:{}:` is set after the document header ended, so it is not a header attribute",
                attribute.name().data()
            ),
            help: "the header ends at the first blank line; remove the blank line between the \
                   header and this entry to make it part of the header"
                .to_string(),
        })
}

/// Images whose file is not where the document says it is.
///
/// Asciidoctor writes the `<img>` and never looks, so a mistyped target is
/// found by whoever opens the page. The target is resolved the way the
/// renderer resolves it — relative to `:imagesdir:` when that is set, and
/// relative to the document otherwise — and anything that is not a local file
/// (a URL, a `data:` URI, an `:imagesdir:` that is itself a URL) is left
/// alone. Icons are too: `icon:` names a glyph, not a file.
///
/// Table cells are entered, since a picture in a table is as easy to mistype
/// as one anywhere else. An image nested inside styled inline text is not
/// seen, which is rare enough to live with.
fn images_that_are_not_there<'src>(document: &'src Document<'src>, base: &Path) -> Vec<Lint<'src>> {
    let images_dir = match document.attribute_value("imagesdir") {
        InterpretedValue::Value(dir) if !dir.is_empty() => Some(dir),
        _ => None,
    };

    // Pictures on the web are not the file system's business.
    if images_dir.as_deref().is_some_and(is_remote) {
        return Vec::new();
    }

    let mut lints = Vec::new();

    let mut look = |target: &str, source: Span<'src>| {
        if is_remote(target) {
            return;
        }

        let path = locate(base, images_dir.as_deref(), target);

        if path.is_file() {
            return;
        }

        lints.push(Lint {
            source,
            code: "ImageNotFound",
            message: format!(
                "image `{target}` was not found (looked at `{}`)",
                path.display()
            ),
            help: match &images_dir {
                Some(dir) => format!(
                    "the target is relative to `:imagesdir:`, which is `{dir}`; check the name, \
                     or the directory"
                ),
                None => "the target is relative to the document's directory; check the name, or \
                         set `:imagesdir:` if the pictures live somewhere else"
                    .to_string(),
            },
        });
    };

    let everything = BlockSelector::new().traverse_documents(true);

    for block in document.find_blocks(&everything) {
        if let Block::Media(media) = block
            && media.type_() == MediaType::Image
        {
            let source = media.target().copied().unwrap_or_else(|| block.span());
            look(media.resolved_target(), source);
        }

        for inline in block.inlines().unwrap_or_default() {
            if let InlineNode::Image(image) = inline
                && !image.is_icon
            {
                look(&image.target, image.location);
            }
        }
    }

    lints
}

/// Whether a target is fetched rather than opened: a URL or a `data:` URI.
fn is_remote(target: &str) -> bool {
    target.contains("://") || target.starts_with("data:")
}

/// Where an image target points on disk, the way the renderer would place it.
///
/// An absolute target is used as it is. Anything else is relative to
/// `:imagesdir:`, which is itself relative to the document unless absolute;
/// with no `:imagesdir:` the target is relative to the document.
fn locate(base: &Path, images_dir: Option<&str>, target: &str) -> PathBuf {
    let target = Path::new(target);

    if target.is_absolute() {
        return target.to_path_buf();
    }

    match images_dir {
        Some(dir) => base.join(dir).join(target),
        None => base.join(target),
    }
}

#[cfg(test)]
mod tests {
    use asciidoc_parser::Parser;

    use super::*;

    fn codes(source: &str) -> Vec<(&'static str, usize)> {
        let document = Parser::default().parse(source);

        check(&document, None)
            .into_iter()
            .map(|lint| (lint.code, lint.source.line()))
            .collect()
    }

    #[test]
    fn an_entry_after_the_blank_line_that_ends_the_header_is_reported() {
        let source = "= Title\n\n:axis: spec\n:status: draft\n\nBody.\n";

        assert_eq!(
            codes(source),
            vec![
                ("AttributeSetAfterHeader", 3),
                ("AttributeSetAfterHeader", 4)
            ]
        );
    }

    #[test]
    fn the_message_names_the_attribute() {
        let document = Parser::default().parse("= Title\n\n:axis: spec\n");
        let lints = check(&document, None);

        assert_eq!(lints.len(), 1);
        assert_eq!(lints[0].source.data(), ":axis: spec");
        assert!(
            lints[0].message.contains("`:axis:`"),
            "{}",
            lints[0].message
        );
    }

    #[test]
    fn an_entry_is_found_inside_the_preamble_a_section_makes() {
        assert_eq!(
            codes("= Title\n\n:axis: spec\n\nBody.\n\n== Section\n\nMore.\n"),
            vec![("AttributeSetAfterHeader", 3)]
        );
    }

    #[test]
    fn an_entry_in_the_header_is_not() {
        assert_eq!(codes("= Title\n:axis: spec\n\nBody.\n"), vec![]);
    }

    #[test]
    fn an_entry_between_blocks_is_where_its_author_put_it() {
        assert_eq!(codes("= Title\n\nBody.\n\n:axis: spec\n\nMore.\n"), vec![]);
    }

    #[test]
    fn an_entry_at_the_top_of_a_document_without_a_header_is_the_header() {
        assert_eq!(codes(":axis: spec\n\nBody.\n"), vec![]);
    }

    #[test]
    fn a_header_without_a_title_keeps_reading_past_a_blank_line() {
        assert_eq!(codes(":toc:\n\n:axis: spec\n\nBody.\n"), vec![]);
    }

    /// A scratch directory holding `files`, empty, so that a test can say
    /// which pictures exist.
    fn pictures(name: &str, files: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("adocers-lint-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("pics")).expect("scratch directory is writable");

        for file in files {
            std::fs::write(dir.join(file), b"").expect("scratch file is writable");
        }

        dir
    }

    fn image_lints(base: &Path, source: &str) -> Vec<(usize, String)> {
        let document = Parser::default().parse(source);

        check(&document, Some(base))
            .into_iter()
            .filter(|lint| lint.code == "ImageNotFound")
            .map(|lint| (lint.source.line(), lint.source.data().to_string()))
            .collect()
    }

    #[test]
    fn a_picture_that_is_there_is_fine_and_one_that_is_not_is_reported() {
        let base = pictures("plain", &["here.png"]);
        let source = "= T\n\nimage::here.png[]\n\nimage::gone.png[]\n";

        assert_eq!(
            image_lints(&base, source),
            vec![(5, "gone.png".to_string())]
        );
    }

    #[test]
    fn an_inline_picture_is_looked_for_too() {
        let base = pictures("inline", &["here.png"]);
        let source = "= T\n\nSee image:here.png[] and image:gone.png[] here.\n";

        assert_eq!(image_lints(&base, source).len(), 1);
    }

    #[test]
    fn a_picture_in_a_table_cell_is_looked_for_too() {
        let base = pictures("table", &[]);
        let source = "= T\n\n|===\na|image::gone.png[]\n|===\n";

        assert_eq!(image_lints(&base, source).len(), 1);
    }

    #[test]
    fn imagesdir_says_where_to_look() {
        let base = pictures("dir", &["pics/here.png", "here.png"]);

        // With `:imagesdir:` only the picture inside it is found.
        let source = "= T\n:imagesdir: pics\n\nimage::here.png[]\n\nimage::gone.png[]\n";
        assert_eq!(image_lints(&base, source).len(), 1);
    }

    #[test]
    fn what_is_fetched_is_not_looked_for() {
        let base = pictures("remote", &[]);
        let source = "= T\n\nimage::https://example.com/x.png[]\n\n\
                      image::data:image/png;base64,AAAA[]\n\nicon:heart[] image:gone.png[]\n";

        assert_eq!(image_lints(&base, source).len(), 1);
    }

    #[test]
    fn a_remote_imagesdir_switches_the_check_off() {
        let base = pictures("remote-dir", &[]);
        let source = "= T\n:imagesdir: https://cdn.example.com/img\n\nimage::gone.png[]\n";

        assert_eq!(image_lints(&base, source), vec![]);
    }

    #[test]
    fn without_a_directory_nothing_is_looked_for() {
        assert_eq!(codes("= T\n\nimage::gone.png[]\n"), vec![]);
    }
}
