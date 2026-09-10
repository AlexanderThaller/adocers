//! Presentation of parse diagnostics with `ariadne`.
//!
//! Parsing AsciiDoc never fails — every UTF-8 string is a valid document — so
//! everything the parser has to say arrives as a [`Warning`] carrying the
//! [`Span`] it was detected at. That span indexes the *preprocessed* source
//! (the document after `include::` expansion), which is exactly the text we
//! hand to `ariadne`, so the underline always lands on the right bytes. When a
//! warning came from an included file, its true origin is added as a note.

use std::{
    io::Write,
    ops::Range,
};

use ariadne::{
    Config,
    IndexType,
    Label,
    Report,
    ReportKind,
    Source,
};
use asciidoc_parser::{
    Document,
    Span,
    parser::Fidelity,
    warnings::{
        Warning,
        WarningSeverity,
    },
};

/// How many diagnostics of each severity a render produced.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    /// Diagnostics at [`WarningSeverity::Warning`].
    pub warnings: usize,

    /// Diagnostics at [`WarningSeverity::Debug`], reported only when verbose.
    pub advice: usize,
}

/// Renders a document's warnings to stderr.
#[derive(Clone, Copy, Debug)]
pub struct Reporter {
    /// Emit ANSI color.
    pub color: bool,

    /// Include [`WarningSeverity::Debug`] diagnostics.
    pub verbose: bool,

    /// Emit nothing at all; counts are still returned.
    pub quiet: bool,
}

impl Reporter {
    /// Report every warning `document` produced, attributing spans to
    /// `display_name`, and return how many were of each severity.
    ///
    /// `display_name` is the path shown in the diagnostic gutter; it is used
    /// only for display, never to re-read the file.
    pub fn report(self, document: &Document<'_>, display_name: &str) -> Counts {
        let mut counts = Counts::default();

        // Every span is an offset into this text, so `ariadne` and the parser
        // agree on where a warning lands even after includes were expanded.
        let source_text = document.span().data();
        let mut cache = (display_name, Source::from(source_text));

        for warning in document.warnings() {
            match warning.severity {
                WarningSeverity::Debug => counts.advice += 1,
                _ => counts.warnings += 1,
            }

            if self.quiet || (warning.severity == WarningSeverity::Debug && !self.verbose) {
                continue;
            }

            let report = self.build(document, display_name, source_text, warning);
            let mut stderr = std::io::stderr().lock();

            // A broken pipe on stderr is not worth failing a render over.
            let _ = report.eprint(&mut cache);
            let _ = stderr.flush();
        }

        counts
    }

    /// Turn one warning into a laid-out report.
    fn build<'a>(
        self,
        document: &Document<'_>,
        display_name: &'a str,
        source_text: &str,
        warning: &Warning<'_>,
    ) -> Report<'a, (&'a str, Range<usize>)> {
        let kind = match warning.severity {
            WarningSeverity::Debug => ReportKind::Advice,
            _ => ReportKind::Warning,
        };

        let range = byte_range(&warning.source, source_text);
        let message = warning.warning.to_string();

        let mut report = Report::build(kind, (display_name, range.clone()))
            .with_config(
                Config::default()
                    .with_color(self.color)
                    // Parser spans are byte offsets, not character offsets.
                    .with_index_type(IndexType::Byte),
            )
            .with_code(code_for(&warning.warning))
            .with_message(&message)
            .with_label(Label::new((display_name, range)).with_message(label_for(&message)));

        if let Some(note) = origin_note(document, warning, display_name) {
            report = report.with_note(note);
        }

        report.finish()
    }
}

/// The byte range a warning's span covers, clamped to the source and widened to
/// at least one byte so that a zero-width span still has something to
/// underline.
fn byte_range(span: &Span<'_>, source_text: &str) -> Range<usize> {
    let len = source_text.len();
    let start = span.byte_offset().min(len);
    let end = start.saturating_add(span.data().len()).min(len);

    if start == end {
        // Extend to the next character boundary so the caret has width; at the
        // very end of the source, fall back to a zero-width span.
        let widened = (start + 1..=len).find(|&i| source_text.is_char_boundary(i));
        start..widened.unwrap_or(start)
    } else {
        start..end
    }
}

/// A short, stable identifier for a warning, taken from its variant name.
///
/// `WarningType`'s `Debug` impl prints `WarningType::Variant`, optionally with
/// a payload; both decorations are stripped so the code stays stable as
/// payloads change.
fn code_for(warning: &asciidoc_parser::warnings::WarningType) -> String {
    let debug = format!("{warning:?}");
    let name = debug.strip_prefix("WarningType::").unwrap_or(&debug);

    name.split(['(', ' ', '{'])
        .next()
        .unwrap_or(name)
        .to_string()
}

/// A terse label for the underlined span.
///
/// The full sentence is already the report's message; repeating it verbatim
/// under the source line just makes the diagnostic taller, so the label keeps
/// only the first clause.
fn label_for(message: &str) -> String {
    match message.split_once(" (") {
        Some((head, _)) => head.to_string(),
        None => message.to_string(),
    }
}

/// A note naming where a warning really came from, when that is not the primary
/// file at the position shown.
fn origin_note(
    document: &Document<'_>,
    warning: &Warning<'_>,
    display_name: &str,
) -> Option<String> {
    // A warning from privately-expanded content carries its own pre-resolved
    // origin, because no span in the document maps back to it.
    if let Some(origin) = &warning.origin {
        let file = origin.0.as_deref().unwrap_or(display_name);
        return Some(format!("originates in {file}:{}", origin.1));
    }

    let origin = document.origin_of(warning.source);
    let file = origin.file?;

    if file == display_name {
        return None;
    }

    let position = match origin.col {
        Some(col) => format!("{file}:{}:{col}", origin.line),
        None => format!("{file}:{}", origin.line),
    };

    Some(match origin.fidelity {
        Fidelity::Verbatim => format!("included from {position}"),
        _ => format!(
            "included from {position} (the line shown above was rewritten during preprocessing)"
        ),
    })
}
