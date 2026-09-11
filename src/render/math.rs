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
pub enum Notation {
    /// What a plain `:stem:` means, and what `[asciimath]` says outright.
    AsciiMath,

    /// What `[latexmath]` says, and what `:stem: latexmath` makes the default.
    Latex,
}

/// Convert one equation to `MathML`, or `None` if it cannot be read.
///
/// The result is a complete `<math>` element, laid out as a displayed equation
/// rather than one sitting in a line of text.
pub fn mathml(source: &str, notation: Notation) -> Option<String> {
    let source = source.trim();

    if source.is_empty() {
        return None;
    }

    match notation {
        Notation::Latex => latex(source),
        Notation::AsciiMath => ascii(source),
    }
}

/// LaTeX, which arrives with its own `<math>` wrapper.
fn latex(source: &str) -> Option<String> {
    // The converter carries a macro table, so it is built once and shared.
    static CONVERTER: std::sync::OnceLock<Option<LatexToMathML>> = std::sync::OnceLock::new();

    let converter = CONVERTER
        .get_or_init(|| LatexToMathML::new(MathCoreConfig::default()).ok())
        .as_ref()?;

    converter
        .convert_with_local_state(source, MathDisplay::Block)
        .ok()
        .map(|rendered| rendered.mathml)
}

/// `AsciiMath`, which yields the contents and needs wrapping.
///
/// The parser answers with an expression tree for anything at all, so an empty
/// rendering is the only sign that it made nothing of the source.
fn ascii(source: &str) -> Option<String> {
    let rendered = asciimath_rs::parse(source).to_mathml();

    if rendered.trim().is_empty() {
        return None;
    }

    Some(format!("<math display=\"block\">{rendered}</math>"))
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
