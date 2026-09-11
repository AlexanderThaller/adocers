//! Keeping callouts working across a highlighted listing.
//!
//! A callout is written `<1>` at the end of a line and rendered as a numbered
//! marker the list beneath the block refers back to. The parser does that when
//! it renders a block's content — but a highlighted block is built from the
//! block's *original* text instead, so the markers arrive as literal `<1>` and
//! would be shown to the reader as such.
//!
//! So the markers are taken out before the source reaches the highlighter and
//! put back afterwards. The parser stays the authority on which `<1>` is a
//! callout and which is a comparison against a generic: this module only reads
//! back the decision it already made, and gives up — leaving the parser's own
//! rendering to be used — the moment the two do not agree.

use std::fmt::Write as _;

/// A callout marker, and the line it was found on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Callout {
    /// Zero-based index of the line the marker ended.
    pub line: usize,

    /// The number shown to the reader.
    pub number: String,
}

/// The markup the parser renders a callout as, up to its number.
const CONUM_OPEN: &str = "<b class=\"conum\">(";

/// Find the callouts in `source`, if they are exactly the ones the parser
/// found.
///
/// `rendered` is the parser's own rendering of the same content, read here only
/// for the callouts it recognized. Returns `None` when the two disagree — a
/// callout written in a form this module does not scan for, or a `<1>` in the
/// code that the parser did not take as a marker — which tells the caller to
/// leave the block to the parser rather than guess.
pub fn locate(source: &str, rendered: &str) -> Option<Vec<Callout>> {
    let found = scan(source);
    let expected = parsed(rendered);

    if found.len() != expected.len() {
        return None;
    }

    let agrees = found
        .iter()
        .zip(&expected)
        .all(|(callout, number)| &callout.number == number);

    agrees.then_some(found)
}

/// Remove the markers, so that the highlighter sees only code.
pub fn strip(source: &str, callouts: &[Callout]) -> String {
    if callouts.is_empty() {
        return source.to_string();
    }

    source
        .split('\n')
        .enumerate()
        .map(
            |(index, line)| match callouts.iter().find(|c| c.line == index) {
                // The marker is always the last thing on its line, so cutting it
                // off cannot disturb anything the highlighter needs.
                Some(callout) => {
                    let marker = format!("<{}>", callout.number);
                    line.strip_suffix(&marker).unwrap_or(line).to_string()
                }

                None => line.to_string(),
            },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// Put the markers back, as the markup the parser would have rendered.
///
/// A marker goes at the very end of its line, outside whatever spans the
/// highlighter opened, which is where it was in the source and where a reader
/// looks for it.
pub fn reapply(highlighted: &str, callouts: &[Callout]) -> String {
    if callouts.is_empty() {
        return highlighted.to_string();
    }

    highlighted
        .split('\n')
        .enumerate()
        .map(
            |(index, line)| match callouts.iter().find(|c| c.line == index) {
                Some(callout) => format!("{line}{CONUM_OPEN}{})</b>", callout.number),
                None => line.to_string(),
            },
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// Draw every callout marker in `html` as a numbered mark.
///
/// The markup is Asciidoctor's own for `:icons: font`: an empty element
/// carrying the number as data, followed by the text form it replaces. A
/// stylesheet draws the first and hides the second — this one with a circle of
/// its own, Asciidoctor's with a Font Awesome glyph — and a page with no
/// stylesheet at all still reads, because the text form is still there.
pub fn iconize(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(at) = rest.find(CONUM_OPEN) {
        out.push_str(&rest[..at]);
        rest = &rest[at..];

        let after = &rest[CONUM_OPEN.len()..];
        let number: String = after.chars().take_while(char::is_ascii_digit).collect();

        // Not a marker after all: leave the text as it stands and carry on
        // past it, so one oddity cannot stop the rest being drawn.
        if number.is_empty() {
            out.push_str(&rest[..CONUM_OPEN.len()]);
            rest = &rest[CONUM_OPEN.len()..];

            continue;
        }

        let _ = write!(out, "<i class=\"conum\" data-value=\"{number}\"></i>");
        out.push_str(&rest[..CONUM_OPEN.len()]);
        rest = &rest[CONUM_OPEN.len()..];
    }

    out.push_str(rest);
    out
}

/// The callout numbers the parser rendered, in document order.
fn parsed(rendered: &str) -> Vec<String> {
    rendered
        .match_indices(CONUM_OPEN)
        .filter_map(|(at, _)| {
            let rest = rendered.get(at + CONUM_OPEN.len()..)?;
            let number: String = rest.chars().take_while(char::is_ascii_digit).collect();

            (!number.is_empty()).then_some(number)
        })
        .collect()
}

/// The `<N>` markers ending a line in `source`, in order.
fn scan(source: &str) -> Vec<Callout> {
    source
        .split('\n')
        .enumerate()
        .filter_map(|(line, text)| {
            let inner = text.strip_suffix('>')?;
            let (_, number) = inner.rsplit_once('<')?;

            (!number.is_empty() && number.chars().all(|c| c.is_ascii_digit())).then(|| Callout {
                line,
                number: number.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "fn main() {\n    let x = 1; // <1>\n    dbg!(x); // <2>\n}";
    const RENDERED: &str = "fn main() {\n    let x = 1; // <b class=\"conum\">(1)</b>\n    \
                            dbg!(x); // <b class=\"conum\">(2)</b>\n}";

    #[test]
    fn finds_the_callouts_the_parser_found() {
        let callouts = locate(SOURCE, RENDERED).unwrap_or_default();

        assert_eq!(
            callouts,
            vec![
                Callout {
                    line: 1,
                    number: "1".to_string()
                },
                Callout {
                    line: 2,
                    number: "2".to_string()
                },
            ]
        );
    }

    #[test]
    fn strips_and_puts_back_what_it_took() {
        let callouts = locate(SOURCE, RENDERED).unwrap_or_default();
        let stripped = strip(SOURCE, &callouts);

        assert!(!stripped.contains("<1>"), "marker survived: {stripped}");
        assert!(stripped.contains("let x = 1; //"));

        let back = reapply(&stripped, &callouts);
        assert!(back.contains("<b class=\"conum\">(1)</b>"));
        assert!(back.contains("<b class=\"conum\">(2)</b>"));
    }

    #[test]
    fn a_block_with_no_callouts_is_untouched() {
        let source = "fn main() {}";

        assert_eq!(locate(source, source), Some(Vec::new()));
        assert_eq!(strip(source, &[]), source);
        assert_eq!(reapply(source, &[]), source);
    }

    #[test]
    fn gives_up_when_the_parser_disagrees() {
        // A generic comparison the parser did not treat as a callout: nothing
        // here should be rewritten on this module's own authority.
        let source = "let ok = a < b && c > d;\nlet v: Vec<1>";

        assert_eq!(
            locate(
                source,
                "let ok = a &lt; b &amp;&amp; c &gt; d;\nlet v: Vec&lt;1&gt;"
            ),
            None
        );
    }

    #[test]
    fn gives_up_when_the_numbers_differ() {
        assert_eq!(locate("x // <2>", "x // <b class=\"conum\">(1)</b>"), None);
    }
}
