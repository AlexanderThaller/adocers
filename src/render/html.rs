//! Small helpers for assembling HTML.
//!
//! Note the split between *text* and *markup*. Much of what the parser hands
//! back — a block title, a section title, a paragraph's content — has already
//! been through inline substitution and is HTML; escaping it again would show
//! the tags to the reader. Only values that came from the source untouched
//! (a URL, a language name, an author's e-mail) go through [`escape_text`] or
//! [`escape_attr`].

use std::fmt::Write as _;

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

/// An HTML document under construction.
///
/// This is a `String` with two conveniences: it tracks indentation depth so the
/// output stays readable, and it knows how to write the `id`/`class` pair that
/// every AsciiDoc block wrapper carries.
#[derive(Debug, Default)]
pub struct Buffer {
    out: String,
}

impl Buffer {
    /// An empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a raw fragment, which is assumed to already be valid markup.
    pub fn raw(&mut self, markup: &str) {
        self.out.push_str(markup);
    }

    /// Append a raw fragment followed by a newline.
    pub fn line(&mut self, markup: &str) {
        self.out.push_str(markup);
        self.out.push('\n');
    }

    /// Drop a trailing newline, so that what comes next continues the line the
    /// buffer already ended.
    pub fn unline(&mut self) {
        if self.out.ends_with('\n') {
            self.out.pop();
        }
    }

    /// Append a newline unless the buffer already ends with one.
    pub fn newline(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    /// Open a `<div>` (or any element) carrying an optional id and a class
    /// list.
    ///
    /// Empty class names are dropped, so callers can pass conditional classes
    /// without filtering them first.
    pub fn open(&mut self, tag: &str, id: Option<&str>, classes: &[&str]) {
        let tag = open_tag(tag, id, classes);
        self.line(&tag);
    }

    /// Write a complete element on one line.
    ///
    /// `content` is markup, not text: headings and titles have already been
    /// through inline substitution by the time they reach here.
    pub fn element(&mut self, tag: &str, id: Option<&str>, classes: &[&str], content: &str) {
        let open = open_tag(tag, id, classes);
        self.line(&format!("{open}{content}</{tag}>"));
    }

    /// Close an element opened with [`open`](Self::open).
    pub fn close(&mut self, tag: &str) {
        let _ = writeln!(self.out, "</{tag}>");
    }

    /// Consume the buffer and yield the markup.
    pub fn finish(self) -> String {
        self.out
    }
}

/// Build an opening tag with an optional id and class list.
fn open_tag(tag: &str, id: Option<&str>, classes: &[&str]) -> String {
    open_tag_with(tag, id, classes, "")
}

/// Build an opening tag that carries `extra` — already-formed markup for the
/// attributes an id and a class list cannot express, such as a bare boolean —
/// after the ones it shares with [`open_tag`].
pub(super) fn open_tag_with(tag: &str, id: Option<&str>, classes: &[&str], extra: &str) -> String {
    let mut out = format!("<{tag}");

    if let Some(id) = id {
        let _ = write!(out, " id=\"{}\"", escape_attr(id));
    }

    let classes: Vec<&str> = classes.iter().filter(|c| !c.is_empty()).copied().collect();
    if !classes.is_empty() {
        let _ = write!(out, " class=\"{}\"", escape_attr(&classes.join(" ")));
    }

    out.push_str(extra);
    out.push('>');
    out
}

impl std::fmt::Write for Buffer {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.out.push_str(s);
        Ok(())
    }
}
