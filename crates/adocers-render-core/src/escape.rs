//! Escaping text for markup.
//!
//! Note the split between *text* and *markup*. Much of what the parser hands
//! back — a block title, a section title, a paragraph's content — has already
//! been through inline substitution and is HTML; escaping it again would show
//! the tags to the reader. Only values that came from the source untouched
//! (a URL, a language name, an author's e-mail) go through [`escape_text`] or
//! [`escape_attr`].
//!
//! Both back ends need this, not only the HTML one: an admonition icon is an
//! SVG with the label written into an attribute, and SVG is markup wherever it
//! ends up.

/// Escape the characters that would otherwise start markup in element content.
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }

    out
}

/// Escape a value destined for a double-quoted attribute.
pub fn escape_attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }

    out
}
