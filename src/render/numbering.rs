//! Which sections are numbered, and what number each one shows.
//!
//! A special section — a preface, a glossary, an index — is not part of the
//! numbered sequence at all. It carries no number of its own, nothing beneath
//! it carries one, and it does not advance the count: the section after a
//! glossary follows the section before it.
//!
//! The parser numbers every section but an appendix, so the numbers are worked
//! out again here. Whether a document numbers its sections at all, and how deep
//! it goes, are still the parser's answer — a section it left unnumbered is
//! left unnumbered here too, which is how `:sectnums:` and `:sectnumlevels:`
//! reach this without being read a second time.
//!
//! Appendices are the parser's throughout. It letters them and numbers what is
//! inside them from the letter, and reproducing that here would be a second
//! implementation of something already right.

use std::collections::HashMap;

use asciidoc_parser::{
    Document,
    HasSpan,
    blocks::{
        Block,
        FindBlocks,
        IsBlock,
        SectionBlock,
        SectionType,
    },
};

/// The section styles that stand outside the numbered sequence.
///
/// `appendix` is not among them: it is numbered, as a letter. `partintro` is a
/// block style rather than a section style, and is listed for the same reason
/// the others are — a document that puts it on a section should not have that
/// section counted.
const SPECIAL: &[&str] = &[
    "colophon",
    "dedication",
    "acknowledgments",
    "preface",
    "partintro",
    "glossary",
    "bibliography",
    "index",
];

/// `abstract` stands outside the sequence in an article and inside it in a
/// book, where it is a chapter like any other and is numbered as one.
const ABSTRACT: &str = "abstract";

/// The number each section shows, worked out once for the whole document.
///
/// Keyed by where a section begins in the source, which is a thing every
/// section has exactly one of.
pub(super) struct Numbering(HashMap<usize, String>);

/// What the sections at one level of the walk are numbered from.
enum From {
    /// The numbers this back end works out, counting up from the prefix.
    Sequence(String),

    /// Nothing: inside a special section, where nothing is numbered.
    Nothing,

    /// The parser's own, which is right inside an appendix.
    Parser,
}

impl Numbering {
    /// Work out the numbering for a whole document.
    pub(super) fn of(document: &Document<'_>) -> Self {
        let mut numbers = HashMap::new();

        let book = matches!(
            document.attribute_value("doctype"),
            asciidoc_parser::document::InterpretedValue::Value(value) if value == "book"
        );

        assign(
            document.child_blocks(),
            &From::Sequence(String::new()),
            book,
            &mut 0,
            &mut numbers,
        );

        Self(numbers)
    }

    /// The text that goes in front of one section's title.
    ///
    /// Empty for a section that shows no number, which is most of them in most
    /// documents.
    pub(super) fn prefix(&self, section: &SectionBlock<'_>) -> String {
        self.0
            .get(&section.span().byte_offset())
            .cloned()
            // A section the walk did not reach is the parser's to number, as
            // it was before this existed.
            .unwrap_or_else(|| parser_prefix(section))
    }
}

/// Number one level of sections, and everything beneath them.
///
/// `counter` counts the sections at this level. It is handed in rather than
/// started here because the chapters of a book run on from one part to the
/// next: they are one sequence broken across several lists of siblings.
fn assign<'src>(
    blocks: impl Iterator<Item = &'src Block<'src>>,
    from: &From,
    book: bool,
    counter: &mut usize,
    numbers: &mut HashMap<usize, String>,
) {
    // The chapters inside the parts at this level, which carry on across them.
    let mut chapters = 0usize;

    for block in blocks {
        let Block::Section(section) = block else {
            continue;
        };

        // A discrete heading is styled like a section and is not one: it is
        // never numbered, and never counted.
        if section.section_type() == SectionType::Discrete {
            numbers.insert(section.span().byte_offset(), String::new());

            continue;
        }

        let (prefix, beneath) = shape(section, from, book, counter);

        numbers.insert(section.span().byte_offset(), prefix);

        // A part holds chapters, which are numbered across the whole book
        // rather than from one within each part.
        let mut own = 0usize;
        let beneath_counter = if is_part(section, book) {
            &mut chapters
        } else {
            &mut own
        };

        assign(
            section.child_blocks(),
            &beneath,
            book,
            beneath_counter,
            numbers,
        );
    }
}

/// Whether a section is one of a book's parts.
///
/// A `=` section in a book is a part; in an article it is the document title
/// and never reaches here as a section.
fn is_part(section: &SectionBlock<'_>, book: bool) -> bool {
    book && section.level() == 0
}

/// What one section shows, and what its own sections count from.
fn shape(
    section: &SectionBlock<'_>,
    from: &From,
    book: bool,
    counter: &mut usize,
) -> (String, From) {
    // An appendix is lettered, and what is inside it is numbered from the
    // letter. Both are the parser's, here and below.
    if section.section_type() == SectionType::Appendix {
        return (parser_prefix(section), From::Parser);
    }

    if is_special(section, book) {
        return (String::new(), From::Nothing);
    }

    match from {
        From::Nothing => (String::new(), From::Nothing),
        From::Parser => (parser_prefix(section), From::Parser),

        From::Sequence(prefix) => {
            // The parser says whether this section is numbered at all, which is
            // where `:sectnums:` and `:sectnumlevels:` are answered.
            if section.section_number().is_none() {
                return (String::new(), From::Sequence(prefix.clone()));
            }

            *counter += 1;
            let number = format!("{prefix}{counter}.");

            (format!("{number} "), From::Sequence(number))
        }
    }
}

/// Whether a section carries one of the styles that stand outside the sequence.
fn is_special(section: &SectionBlock<'_>, book: bool) -> bool {
    section
        .declared_style()
        .is_some_and(|style| SPECIAL.contains(&style) || (style == ABSTRACT && !book))
}

/// What the parser would have put in front of the title.
///
/// An appendix carries a full caption — `Appendix A: ` — and an ordinary
/// numbered section carries only its number.
fn parser_prefix(section: &SectionBlock<'_>) -> String {
    if let Some(caption) = section.caption() {
        return caption.to_string();
    }

    match section.section_number() {
        Some(number) => format!("{number}. "),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use asciidoc_parser::Parser;

    use crate::render::{
        Options,
        render,
    };

    /// The headings one document renders, number and all.
    fn headings(source: &str) -> Vec<String> {
        let mut parser = Parser::default();
        let document = parser.parse(source);

        let options = Options {
            fragment: true,
            ..Options::default()
        };

        render(&document, &options)
            .html
            .lines()
            .filter(|line| line.starts_with("<h"))
            .map(|line| {
                let text = line.split_once('>').map_or(line, |(_, rest)| rest);

                text.split_once("</")
                    .map_or(text, |(text, _)| text)
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn a_special_section_is_not_numbered_and_is_not_counted() {
        let numbers = headings("= D\n:sectnums:\n\n== One\n\n[glossary]\n== Glossary\n\n== Two\n");

        assert_eq!(numbers, ["1. One", "Glossary", "2. Two"]);
    }

    #[test]
    fn every_special_style_stands_outside_the_sequence() {
        for style in [
            "colophon",
            "dedication",
            "acknowledgments",
            "preface",
            "glossary",
            "bibliography",
            "index",
            "abstract",
        ] {
            let numbers = headings(&format!(
                "= D\n:sectnums:\n\n[{style}]\n== Special\n\n== One\n"
            ));

            assert_eq!(numbers, ["Special", "1. One"], "`{style}` should stand out");
        }
    }

    #[test]
    fn nothing_beneath_a_special_section_is_numbered() {
        let numbers =
            headings("= D\n:sectnums:\n\n[preface]\n== Preface\n\n=== Inside\n\n== One\n");

        assert_eq!(numbers, ["Preface", "Inside", "1. One"]);
    }

    #[test]
    fn a_special_section_stands_out_at_any_level() {
        let numbers =
            headings("= D\n:sectnums:\n\n== One\n\n=== A\n\n[glossary]\n=== Gloss\n\n=== B\n");

        assert_eq!(numbers, ["1. One", "1.1. A", "Gloss", "1.2. B"]);
    }

    #[test]
    fn an_appendix_is_lettered_and_its_sections_numbered_from_the_letter() {
        let numbers =
            headings("= D\n:sectnums:\n\n== One\n\n[appendix]\n== App\n\n=== Inside\n\n== Two\n");

        assert_eq!(
            numbers,
            ["1. One", "Appendix A: App", "A.1. Inside", "2. Two"]
        );
    }

    #[test]
    fn a_books_chapters_run_on_across_its_parts() {
        let numbers = headings(
            "= B\n:doctype: book\n:sectnums:\n\n= Part One\n\n== A\n\n== B\n\n= Part Two\n\n== C\n",
        );

        assert_eq!(numbers, ["Part One", "1. A", "2. B", "Part Two", "3. C"]);
    }

    #[test]
    fn a_books_abstract_is_a_chapter_and_an_articles_is_not() {
        let book =
            headings("= B\n:doctype: book\n:sectnums:\n\n[abstract]\n== Abstract\n\n== One\n");

        assert_eq!(book, ["1. Abstract", "2. One"]);

        let article = headings("= A\n:sectnums:\n\n[abstract]\n== Abstract\n\n== One\n");

        assert_eq!(article, ["Abstract", "1. One"]);
    }

    #[test]
    fn a_document_that_numbers_nothing_shows_nothing() {
        let numbers = headings("= D\n\n== One\n\n[glossary]\n== Glossary\n\n== Two\n");

        assert_eq!(numbers, ["One", "Glossary", "Two"]);
    }

    #[test]
    fn numbering_stops_where_sectnumlevels_says() {
        let numbers = headings("= D\n:sectnums:\n:sectnumlevels: 1\n\n== One\n\n=== A\n\n== Two\n");

        assert_eq!(numbers, ["1. One", "A", "2. Two"]);
    }
}
