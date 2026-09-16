//! Which header attributes are facts about the document, and how they read.
//!
//! A back end shows these under the title, so both of them have to agree on
//! which attributes are shown and what each is called — otherwise a page and a
//! PDF of the same document would carry different headers.

/// Header attributes shown to the reader as facts about the document.
///
/// Everything else a header sets — `sectnums`, `icons`, `source-highlighter` —
/// is an instruction to the renderer rather than something a reader wants to
/// read, so the list is an allowlist: an attribute nobody thought about is left
/// out rather than shown by accident.
const METADATA: &[&str] = &[
    "status",
    "date",
    "keywords",
    "category",
    "edition",
    "organization",
    "copyright",
    // What a review says about itself — the range it looked at, what it read
    // the changes against, how much it found — and what one of its findings
    // says: which axis, how bad, of what kind, where, and on whose authority.
    "fixed-point",
    "head",
    "diff",
    "spec",
    "standards",
    "findings",
    "axis",
    "severity",
    "kind",
    "where",
    "source",
];

/// Antora's namespace for page metadata. An attribute in it is shown with the
/// prefix dropped, so `:page-tags:` reads as `Tags`.
const PAGE_PREFIX: &str = "page-";

/// The name an attribute is shown under, or `None` if it is not shown at all.
pub fn displayed_as(name: &str) -> Option<&str> {
    if let Some(rest) = name.strip_prefix(PAGE_PREFIX) {
        return (!rest.is_empty()).then_some(rest);
    }

    METADATA.contains(&name).then_some(name)
}

/// The label for an attribute name: `page-last-reviewed` reads `Last reviewed`.
pub fn label_for(name: &str) -> String {
    let spaced = name.replace(['-', '_'], " ");
    let mut characters = spaced.chars();

    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => spaced,
    }
}

/// Whether an attribute's value is a comma-separated list of separate things.
///
/// A back end shows each of them as its own mark rather than running them
/// together into a sentence.
pub fn is_list(name: &str) -> bool {
    matches!(name, "tags" | "keywords" | "standards")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_allowlisted_attribute_is_shown_under_its_own_name() {
        assert_eq!(displayed_as("keywords"), Some("keywords"));
    }

    #[test]
    fn an_antora_attribute_is_shown_with_the_prefix_dropped() {
        assert_eq!(displayed_as("page-tags"), Some("tags"));
    }

    #[test]
    fn an_attribute_nobody_listed_is_not_shown() {
        assert_eq!(displayed_as("sectnums"), None);
        assert_eq!(displayed_as("page-"), None);
    }

    #[test]
    fn a_label_reads_as_a_sentence() {
        assert_eq!(label_for("last-reviewed"), "Last reviewed");
        assert_eq!(label_for("fixed_point"), "Fixed point");
    }
}
