//! Inline markup, from the HTML the parser produced to Typst.
//!
//! `asciidoc-parser` renders inline content — bold, links, cross references,
//! passthroughs — to HTML and stops there. The HTML back end can use that as it
//! stands; this one cannot, so the small set of elements AsciiDoc actually
//! produces is translated, and anything else is unwrapped to its text.
//!
//! Unwrapping rather than dropping is the point. A document that uses something
//! this does not know about still reads; it just reads plainly.

/// Turn one run of rendered inline HTML into Typst markup.
pub fn typst(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(at) = rest.find('<') {
        out.push_str(&escape(&rest[..at]));

        let Some(close) = rest[at..].find('>') else {
            // A stray `<` is text, not the start of anything.
            out.push_str(&escape(&rest[at..]));
            return out;
        };

        let tag = &rest[at + 1..at + close];
        rest = &rest[at + close + 1..];

        out.push_str(&element(tag, &mut rest));
    }

    out.push_str(&escape(rest));
    out
}

/// Translate one opening tag, consuming its contents and closing tag.
fn element(tag: &str, rest: &mut &str) -> String {
    let name = tag
        .split([' ', '/'])
        .next()
        .unwrap_or_default()
        .to_lowercase();

    // Empty elements carry nothing to translate, except a picture, which is
    // marked for the caller to place: reading it from disk needs more than this
    // knows about.
    match name.as_str() {
        "br" => return "\\\n".to_string(),
        "img" => return picture(tag),
        "hr" | "wbr" => return String::new(),
        _ => {}
    }

    // A closing tag reached out of order is the end of something this function
    // is not handling; it has no content of its own.
    if name.starts_with('/') || tag.starts_with('/') {
        return String::new();
    }

    let content = take_until_close(&name, rest);
    let inner = typst(&content);

    match name.as_str() {
        "strong" | "b" => format!("*{inner}*"),
        "em" | "i" => format!("_{inner}_"),
        "code" => format!("#raw({})", string(&text(&content))),
        "mark" => format!("#highlight[{inner}]"),

        // A footnote's mark, which the parser renders as a bracketed link down
        // to the definition. A PDF has somewhere better to put it.
        "sup" if is_footnote(tag) => footnote(&content).unwrap_or(format!("#super[{inner}]")),

        "sup" => format!("#super[{inner}]"),
        "sub" => format!("#sub[{inner}]"),
        "del" | "s" => format!("#strike[{inner}]"),
        "u" => format!("#underline[{inner}]"),
        "a" => link(tag, &inner),

        // Everything else contributes its text and nothing else.
        _ => inner,
    }
}

/// What a picture in a line of text is left as, for the caller to replace.
///
/// Placing one needs the directory the document was read from and somewhere to
/// keep the bytes; this knows only about markup. So the picture is written as a
/// marker — the source, the height it asked for and the words to fall back on,
/// between two of a character no document contains.
pub const PICTURE: &str = "\u{2}";

/// What separates the three things a picture's marker carries.
pub const FIELD: &str = "\u{1}";

/// One picture, as the marker the caller reads.
fn picture(tag: &str) -> String {
    let Some(source) = attribute(tag, "src") else {
        return String::new();
    };

    let words = attribute(tag, "alt")
        .map(|alt| escape(&alt))
        .unwrap_or_default();
    let height = attribute(tag, "height").unwrap_or_default();

    format!("{PICTURE}{source}{FIELD}{height}{FIELD}{words}{PICTURE}")
}

/// Whether a `<sup>` is a footnote's mark rather than an ordinary superscript.
fn is_footnote(tag: &str) -> bool {
    attribute(tag, "class").is_some_and(|class| class == "footnote" || class == "footnoteref")
}

/// One footnote reference, as a call to the helper the preamble defines.
///
/// The number is the one the parser assigned, which is also the position of the
/// definition in the document's catalogue, so the text can be looked up by it.
fn footnote(content: &str) -> Option<String> {
    let at = content.find("#_footnotedef_")?;
    let digits = &content[at + "#_footnotedef_".len()..];
    let end = digits
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(digits.len());

    let index: usize = digits[..end].parse().ok()?;

    Some(format!("#adocfootnote({index})"))
}

/// A link, which needs its target as well as its text.
fn link(tag: &str, inner: &str) -> String {
    match attribute(tag, "href") {
        // An internal reference points at a label in the same document.
        Some(href) if href.starts_with('#') => {
            format!("#link(label({}))[{inner}]", string(&href[1..]))
        }

        Some(href) => format!("#link({})[{inner}]", string(&href)),

        // An `[[id]]` anchor, which the parser renders as a link with nowhere
        // to go. A label needs something in front of it to attach to, so it is
        // given an empty box: no width, and a place in the line.
        None => match attribute(tag, "id") {
            Some(id) => format!("#box[]#label({}){inner}", string(&id)),
            None => inner.to_string(),
        },
    }
}

/// Read one attribute out of a tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}=\""))?;
    let value = &tag[at + name.len() + 2..];
    let end = value.find('"')?;

    Some(unescape(&value[..end]))
}

/// Consume everything up to the matching closing tag, allowing for nesting.
fn take_until_close(name: &str, rest: &mut &str) -> String {
    let open = format!("<{name}");
    let close = format!("</{name}>");

    let mut depth = 1usize;
    let mut at = 0usize;

    while depth > 0 {
        let next_open = rest[at..].find(&open).map(|i| at + i);
        let next_close = rest[at..].find(&close).map(|i| at + i);

        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                at = o + open.len();
            }

            (_, Some(c)) => {
                depth -= 1;

                if depth == 0 {
                    let content = rest[..c].to_string();
                    *rest = &rest[c + close.len()..];

                    return content;
                }

                at = c + close.len();
            }

            // Unbalanced markup: take the rest and stop.
            (_, None) => {
                let content = (*rest).to_string();
                *rest = "";

                return content;
            }
        }
    }

    String::new()
}

/// The text of a run of HTML, with the markup taken out.
///
/// Safe on verbatim content because the parser escapes the author's own angle
/// brackets: a `<` still standing is the start of markup the renderer added,
/// such as the `<b class="conum">` of a callout.
pub fn text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);

        match rest[at..].find('>') {
            Some(close) => rest = &rest[at + close + 1..],
            None => return unescape(&out),
        }
    }

    out.push_str(rest);
    unescape(&out)
}

/// Turn HTML entities back into the characters they stand for.
///
/// A page can leave `&#8594;` alone because a browser reads it; a PDF has no
/// such reader, so every numeric reference is resolved here, along with the
/// handful of named ones AsciiDoc emits.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];

        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            // Not an entity: an ampersand on its own.
            out.push('&');
            rest = &rest[1..];

            continue;
        };

        match entity(&rest[1..end]) {
            Some(character) => out.push_str(&character),
            None => out.push_str(&rest[..=end]),
        }

        rest = &rest[end + 1..];
    }

    out.push_str(rest);
    out
}

/// One entity body — what sits between the `&` and the `;`.
fn entity(name: &str) -> Option<String> {
    // A numeric reference says exactly which character it means.
    if let Some(digits) = name.strip_prefix('#') {
        let point = match digits.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => digits.parse().ok()?,
        };

        // A zero-width space would only widen the markup it lands in.
        if point == 0x200b {
            return Some(String::new());
        }

        return char::from_u32(point).map(String::from);
    }

    let character = match name {
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "\u{2019}",
        "amp" => "&",
        "nbsp" => "\u{a0}",
        "hellip" => "\u{2026}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        _ => return None,
    };

    Some(character.to_string())
}

/// Escape the characters Typst reads as markup.
fn escape(html: &str) -> String {
    let text = unescape(html);
    let mut out = String::with_capacity(text.len());

    for c in text.chars() {
        if matches!(
            c,
            '*' | '_' | '`' | '$' | '#' | '<' | '>' | '@' | '\\' | '[' | ']'
        ) {
            out.push('\\');
        }

        out.push(c);
    }

    out
}

/// Quote a value as a Typst string literal.
///
/// Line breaks are escaped rather than written, so a whole listing can be one
/// literal without its own shape ending it.
pub fn string(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('\t', "\\t");

    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_the_marks_asciidoc_emits() {
        assert_eq!(typst("<strong>a</strong>"), "*a*");
        assert_eq!(typst("<em>a</em>"), "_a_");
        assert_eq!(typst("plain"), "plain");
    }

    #[test]
    fn keeps_nesting() {
        assert_eq!(typst("<strong>a <em>b</em></strong>"), "*a _b_*");
    }

    #[test]
    fn turns_a_link_into_one() {
        assert_eq!(
            typst(r#"<a href="https://example.com">text</a>"#),
            "#link(\"https://example.com\")[text]"
        );
    }

    #[test]
    fn points_an_internal_reference_at_a_label() {
        assert_eq!(
            typst(r##"<a href="#tables">Tables</a>"##),
            "#link(label(\"tables\"))[Tables]"
        );
    }

    #[test]
    fn escapes_what_typst_would_read_as_markup() {
        assert_eq!(typst("2 * 3"), "2 \\* 3");
        assert_eq!(typst("a_b"), "a\\_b");
    }

    #[test]
    fn takes_the_markup_out_of_verbatim_content() {
        assert_eq!(
            text("let v: Vec&lt;String&gt;; // <b class=\"conum\">(1)</b>"),
            "let v: Vec<String>; // (1)"
        );
    }

    #[test]
    fn turns_a_footnote_mark_into_a_footnote() {
        let mark = r##"<sup class="footnote">[<a id="_footnoteref_2" class="footnote" href="#_footnotedef_2" title="View footnote.">2</a>]</sup>"##;

        assert_eq!(typst(mark), "#adocfootnote(2)");
    }

    #[test]
    fn leaves_an_ordinary_superscript_alone() {
        assert_eq!(typst("x<sup>2</sup>"), "x#super[2]");
    }

    #[test]
    fn gives_an_anchor_a_label_to_be_found_by() {
        assert_eq!(
            typst(r#"<a id="block"></a>block"#),
            "#box[]#label(\"block\")block"
        );
    }

    #[test]
    fn marks_a_picture_for_the_caller_to_place() {
        assert_eq!(
            typst(r#"<img src="tip.svg" alt="Tip" height="16">"#),
            "\u{2}tip.svg\u{1}16\u{1}Tip\u{2}"
        );
    }

    #[test]
    fn unwraps_what_it_does_not_know() {
        assert_eq!(typst("<span class=\"x\">text</span>"), "text");
    }

    #[test]
    fn turns_entities_back_into_characters() {
        assert_eq!(typst("a &amp; b"), "a & b");
        assert_eq!(typst("it&#8217;s"), "it\u{2019}s");
    }

    #[test]
    fn resolves_any_numeric_reference() {
        assert_eq!(typst("&#8594;"), "\u{2192}");
        assert_eq!(typst("&#x2192;"), "\u{2192}");
        assert_eq!(typst("&hellip;"), "\u{2026}");
    }

    #[test]
    fn leaves_an_ampersand_that_starts_nothing() {
        assert_eq!(typst("a & b"), "a & b");
        assert_eq!(typst("R&D; more"), "R&D; more");
    }
}
