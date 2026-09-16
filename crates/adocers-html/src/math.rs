//! Mathematics, as `MathML`.
//!
//! An equation is converted here, while the page is rendered, and reaches the
//! reader as `MathML` — which every current browser draws itself. A page with
//! equations on it therefore fetches nothing to show them and runs nothing.
//!
//! What it used to do instead was load `MathJax`, 2.2 MB of JavaScript, and
//! have it rewrite the equations once that arrived. `MathML` is a few hundred
//! bytes an equation and needs no fonts, because the browser uses the ones it
//! already has.
//!
//! Two notations, because AsciiDoc has two. `[latexmath]` — and `[stem]` under
//! `:stem: latexmath` — goes through `math_core`, which is thorough: sums,
//! integrals, matrices and limits all come out right. A plain `:stem:` means
//! `AsciiMath`, which goes through `asciimath_rs` and is rougher: `sqrt(4)`
//! keeps the parentheses a reader would expect it to drop. Anything neither can
//! read is left as the source it was written as, which is more use than a gap.

use asciimath_rs::format::mathml::ToMathML as _;
use math_core::{
    LatexToMathML,
    MathCoreConfig,
    MathDisplay,
};

/// Which notation an equation is written in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Notation {
    /// What a plain `:stem:` means, and what `[asciimath]` says outright.
    AsciiMath,

    /// What `[latexmath]` says, and what `:stem: latexmath` makes the default.
    Latex,
}

/// Convert one equation to `MathML`, or `None` if it cannot be read.
///
/// The result is a complete `<math>` element, laid out as a displayed equation
/// rather than one sitting in a line of text.
pub(crate) fn mathml(source: &str, notation: Notation) -> Option<String> {
    let source = source.trim();

    if source.is_empty() {
        return None;
    }

    convert(source, notation, MathDisplay::Block)
}

/// Convert one equation, displayed or in a line of text.
fn convert(source: &str, notation: Notation, display: MathDisplay) -> Option<String> {
    let source = source.trim();

    if source.is_empty() {
        return None;
    }

    match notation {
        Notation::Latex => latex(source, display),
        Notation::AsciiMath => ascii(source, display),
    }
}

/// Rewrite the inline equations in a run of rendered markup.
///
/// The parser hands an inline `stem:[…]` to the page as the delimiters
/// `MathJax` used to look for — `\(…\)` for LaTeX, `\$…\$` for `AsciiMath`.
/// Nothing looks for them now, so they are found and converted here, and a
/// reader is spared a line with `\$sqrt(4)\$` sitting in it.
///
/// Verbatim blocks are left alone. A listing showing `\(` means it, and
/// rewriting it would be changing what the author wrote.
pub(crate) fn inline(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;

    while !rest.is_empty() {
        // Anything up to the next verbatim block can be converted; the block
        // itself is copied across untouched.
        let (prose, verbatim) = match rest.find("<pre") {
            Some(at) => match rest[at..].find("</pre>") {
                Some(end) => (&rest[..at], &rest[at..at + end + "</pre>".len()]),
                None => (&rest[..at], &rest[at..]),
            },

            None => (rest, ""),
        };

        out.push_str(&convert_all(prose));
        out.push_str(verbatim);

        rest = &rest[prose.len() + verbatim.len()..];
    }

    out
}

/// Convert every delimited equation in one run of prose.
fn convert_all(prose: &str) -> String {
    let mut out = String::with_capacity(prose.len());
    let mut rest = prose;

    while let Some((at, open, close, notation)) = next_equation(rest) {
        out.push_str(&rest[..at]);

        let body = &rest[at + open.len()..];
        let Some(end) = body.find(close) else {
            // An opening delimiter with no closing one is not an equation.
            out.push_str(&rest[at..at + open.len()]);
            rest = &rest[at + open.len()..];
            continue;
        };

        let source = unescape(&body[..end]);
        let converted = convert(&source, notation, MathDisplay::Inline);

        match converted {
            Some(mathml) => out.push_str(&mathml),

            // Left exactly as it arrived, so nothing is lost to a failure.
            None => out.push_str(&rest[at..at + open.len() + end + close.len()]),
        }

        rest = &body[end + close.len()..];
    }

    out.push_str(rest);
    out
}

/// The next delimited equation in `prose`, whichever notation comes first.
fn next_equation(prose: &str) -> Option<(usize, &'static str, &'static str, Notation)> {
    let latex = prose
        .find("\\(")
        .map(|at| (at, "\\(", "\\)", Notation::Latex));
    let ascii = prose
        .find("\\$")
        .map(|at| (at, "\\$", "\\$", Notation::AsciiMath));

    match (latex, ascii) {
        (Some(l), Some(a)) if a.0 < l.0 => Some(a),
        (Some(l), _) => Some(l),
        (None, found) => found,
    }
}

/// Turn escaped page text back into the source the author wrote.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// LaTeX, which arrives with its own `<math>` wrapper.
fn latex(source: &str, display: MathDisplay) -> Option<String> {
    // The converter carries a macro table, so it is built once and shared.
    static CONVERTER: std::sync::OnceLock<Option<LatexToMathML>> = std::sync::OnceLock::new();

    let converter = CONVERTER
        .get_or_init(|| LatexToMathML::new(MathCoreConfig::default()).ok())
        .as_ref()?;

    converter
        .convert_with_local_state(source, display)
        .ok()
        .map(|rendered| rendered.mathml)
}

/// `AsciiMath`, which yields the contents and needs wrapping.
///
/// The parser answers with an expression tree for anything at all, so an empty
/// rendering is the only sign that it made nothing of the source.
fn ascii(source: &str, display: MathDisplay) -> Option<String> {
    let rendered = asciimath_rs::parse(source).to_mathml();

    if rendered.trim().is_empty() {
        return None;
    }

    let attribute = match display {
        MathDisplay::Block => " display=\"block\"",
        MathDisplay::Inline => "",
    };

    Some(format!("<math{attribute}>{rendered}</math>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_latex() {
        let out = mathml(r"C = \alpha + \beta Y^{\gamma}", Notation::Latex).expect("converts");

        assert!(out.starts_with("<math"), "{out}");
        assert!(out.contains("<mi>α</mi>"), "{out}");
    }

    #[test]
    fn converts_the_shapes_worth_having() {
        for source in [
            r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
            r"\int_0^x e^{-t^2}\,dt",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\lim_{x \to \infty} \frac{1}{x} = 0",
        ] {
            assert!(
                mathml(source, Notation::Latex).is_some(),
                "should convert: {source}"
            );
        }
    }

    #[test]
    fn converts_asciimath() {
        let out = mathml("a^2 + b^2 = c^2", Notation::AsciiMath).expect("converts");

        assert!(out.starts_with("<math"), "{out}");
        assert!(out.contains("<msup>"), "{out}");
    }

    #[test]
    fn declines_what_it_cannot_read() {
        assert!(mathml(r"\frac{", Notation::Latex).is_none());
        assert!(mathml("   ", Notation::AsciiMath).is_none());
        assert!(mathml("", Notation::Latex).is_none());
    }
}
