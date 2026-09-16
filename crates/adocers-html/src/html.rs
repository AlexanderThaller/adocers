//! Small helpers for assembling HTML.
//!
//! The escaping these build on lives in [`adocers_render_core::escape`], which
//! both back ends share; it is re-exported here so that a caller assembling
//! markup has one place to reach for.

use std::fmt::Write as _;

pub use adocers_render_core::escape::{
    escape_attr,
    escape_text,
};

/// An HTML document under construction.
///
/// This is a `String` with two conveniences: it tracks indentation depth so the
/// output stays readable, and it knows how to write the `id`/`class` pair that
/// every AsciiDoc block wrapper carries.
#[derive(Debug, Default)]
pub(crate) struct Buffer {
    out: String,
}

impl Buffer {
    /// An empty buffer.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Append a raw fragment, which is assumed to already be valid markup.
    pub(crate) fn raw(&mut self, markup: &str) {
        self.out.push_str(markup);
    }

    /// Append a raw fragment followed by a newline.
    pub(crate) fn line(&mut self, markup: &str) {
        self.out.push_str(markup);
        self.out.push('\n');
    }

    /// Drop a trailing newline, so that what comes next continues the line the
    /// buffer already ended.
    pub(crate) fn unline(&mut self) {
        if self.out.ends_with('\n') {
            self.out.pop();
        }
    }

    /// Append a newline unless the buffer already ends with one.
    pub(crate) fn newline(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    /// Open a `<div>` (or any element) carrying an optional id and a class
    /// list.
    ///
    /// Empty class names are dropped, so callers can pass conditional classes
    /// without filtering them first.
    pub(crate) fn open(&mut self, tag: &str, id: Option<&str>, classes: &[&str]) {
        let tag = open_tag(tag, id, classes);
        self.line(&tag);
    }

    /// Write a complete element on one line.
    ///
    /// `content` is markup, not text: headings and titles have already been
    /// through inline substitution by the time they reach here.
    pub(crate) fn element(&mut self, tag: &str, id: Option<&str>, classes: &[&str], content: &str) {
        let open = open_tag(tag, id, classes);
        self.line(&format!("{open}{content}</{tag}>"));
    }

    /// Close an element opened with [`open`](Self::open).
    pub(crate) fn close(&mut self, tag: &str) {
        let _ = writeln!(self.out, "</{tag}>");
    }

    /// Consume the buffer and yield the markup.
    pub(crate) fn finish(self) -> String {
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
pub(crate) fn open_tag_with(tag: &str, id: Option<&str>, classes: &[&str], extra: &str) -> String {
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
