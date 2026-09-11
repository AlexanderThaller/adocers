//! Presentation of diagnostics with `ariadne`.
//!
//! Parsing AsciiDoc never fails — every UTF-8 string is a valid document — so
//! everything the parser has to say arrives as a [`Warning`] carrying the
//! [`Span`] it was detected at. That span indexes the *preprocessed* source
//! (the document after `include::` expansion), which is exactly the text we
//! hand to `ariadne`, so the underline always lands on the right bytes. When a
//! warning came from an included file, its true origin is added as a note.
//!
//! adocers has a few things of its own to say about a document the parser
//! accepted as written (see [`crate::lint`]); a [`Lint`] is shown the same way
//! and counted as a warning.

use std::{
    io::Write,
    ops::Range,
    path::Path,
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

use crate::lint::{
    self,
    Lint,
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
    /// Report every warning `document` produced, and every lint adocers has
    /// about it, attributing spans to `display_name`, and return how many
    /// were of each severity.
    ///
    /// `display_name` is the path shown in the diagnostic gutter; it is used
    /// only for display, never to re-read the file. `base` is the directory
    /// the document was read from, for the lints that look beside it; `None`
    /// for a document that came from nowhere in particular.
    pub fn report(
        self,
        document: &Document<'_>,
        display_name: &str,
        base: Option<&Path>,
    ) -> Counts {
        let mut counts = Counts::default();

        // Every span is an offset into this text, so `ariadne` and the parser
        // agree on where a warning lands even after includes were expanded.
        let source_text = document.span().data();

        // Built on the first diagnostic that is actually shown, not before.
        // `Source::from` indexes every line of the document, which is a whole
        // pass over it, and most renders have nothing to report — or were asked
        // to be quiet about what they do.
        let mut cache: Option<(&str, Source<&str>)> = None;

        let warnings = document.warnings().map(|warning| {
            let severity = warning.severity;
            let diagnostic = Diagnostic::warning(document, display_name, warning);
            (severity, diagnostic)
        });

        let lints = lint::check(document, base)
            .into_iter()
            .map(|lint| (WarningSeverity::Warning, Diagnostic::lint(lint)));

        for (severity, diagnostic) in warnings.chain(lints) {
            match severity {
                WarningSeverity::Debug => counts.advice += 1,
                _ => counts.warnings += 1,
            }

            if self.quiet || (severity == WarningSeverity::Debug && !self.verbose) {
                continue;
            }

            let cache = cache.get_or_insert_with(|| (display_name, Source::from(source_text)));
            let report = self.build(display_name, source_text, severity, &diagnostic);
            let mut stderr = std::io::stderr().lock();

            // A broken pipe on stderr is not worth failing a render over.
            let _ = report.eprint(&mut *cache);
            let _ = stderr.flush();
        }

        counts
    }

    /// Turn one diagnostic into a laid-out report.
    fn build<'a>(
        self,
        display_name: &'a str,
        source_text: &str,
        severity: WarningSeverity,
        diagnostic: &Diagnostic<'_>,
    ) -> Report<'a, (&'a str, Range<usize>)> {
        let kind = match severity {
            WarningSeverity::Debug => ReportKind::Advice,
            _ => ReportKind::Warning,
        };

        let range = byte_range(&diagnostic.source, source_text);

        let mut report = Report::build(kind, (display_name, range.clone()))
            .with_config(
                Config::default()
                    .with_color(self.color)
                    // Parser spans are byte offsets, not character offsets.
                    .with_index_type(IndexType::Byte),
            )
            .with_code(&diagnostic.code)
            .with_message(&diagnostic.message)
            .with_label(Label::new((display_name, range)).with_message(&diagnostic.label));

        if let Some(help) = &diagnostic.help {
            report = report.with_help(help);
        }

        if let Some(note) = &diagnostic.note {
            report = report.with_note(note);
        }

        report.finish()
    }
}

/// What a report is built from, whichever side of the parser it came from.
struct Diagnostic<'src> {
    /// The span to underline.
    source: Span<'src>,

    /// The short identifier shown in brackets.
    code: String,

    /// The sentence shown as the report's headline.
    message: String,

    /// The terse text shown under the underline.
    label: String,

    /// What to do about it, if there is advice to give.
    help: Option<String>,

    /// Where the span really came from, when that is not the file shown.
    note: Option<String>,
}

impl<'src> Diagnostic<'src> {
    /// A parser warning, with its origin note if it came from an include.
    fn warning(document: &Document<'_>, display_name: &str, warning: &Warning<'src>) -> Self {
        let message = warning.warning.to_string();

        Self {
            source: warning.source,
            code: code_for(&warning.warning),
            label: label_for(&message),
            message,
            help: None,
            note: origin_note(document, warning, display_name),
        }
    }

    /// One of adocers' own checks.
    fn lint(lint: Lint<'src>) -> Self {
        Self {
            source: lint.source,
            code: lint.code.to_string(),
            label: label_for(&lint.message),
            message: lint.message,
            help: Some(lint.help),
            note: None,
        }
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
