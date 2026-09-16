//! Equations, converted from the notation they were written in to Typst's own.
//!
//! Typst has a mathematics mode, but its syntax is neither LaTeX's nor
//! `AsciiMath`'s — `\sum_{i=1}^{n}` is `sum_(i=1)^n` there — so an equation
//! cannot be handed over as it stands.
//!
//! [`mitex`](https://crates.io/crates/mitex) translates LaTeX to Typst in Rust,
//! which is the whole of what is needed: no package to fetch, no WebAssembly to
//! run, nothing but a function call. What it produces calls a handful of
//! handlers that its own Typst package would supply through a scope, and
//! [`PRELUDE`] defines them instead.
//!
//! `AsciiMath` is not converted. It looks close enough to Typst to tempt one
//! into passing it through, and it is not: `int_0^1` is an integral in
//! `AsciiMath` and a name Typst already has for the whole-number type. An
//! equation shown as the notation it was written in is honest; one quietly
//! typeset wrong is not.

/// The handlers `mitex`'s output calls for, defined once at the top of a
/// document.
pub(crate) const PRELUDE: &str = include_str!("math.typ");

/// Translate one LaTeX equation to Typst mathematics.
///
/// `None` when `mitex` cannot read it — an unknown command, or something that
/// is not LaTeX at all — which leaves the caller to show the source.
pub(crate) fn typst(source: &str) -> Option<String> {
    let source = source.trim();

    if source.is_empty() {
        return None;
    }

    let converted = mitex::convert_math(source, None).ok()?;

    (!converted.trim().is_empty()).then(|| retarget(&converted))
}

/// Symbols Typst has renamed since `mitex`'s tables were written.
///
/// These are paths rather than plain names, so a `#let` cannot stand in for
/// them: `plus.circle.big` has to become `plus.o.big`, modifier and all. What
/// `mitex` calls something else again — a name this does not know — falls back
/// to being shown as its source, which is what [`super::pdf`] catches.
const RENAMED: &[(&str, &str)] = &[
    ("plus.circle", "plus.o"),
    ("minus.circle", "minus.o"),
    ("times.circle", "times.o"),
    ("dot.circle", "dot.o"),
    ("ast.circle", "ast.o"),
    ("dash.circle", "dash.o"),
    ("angle.l", "chevron.l"),
    ("angle.r", "chevron.r"),
];

// Not renamed but retired: this Typst has no reduced Planck constant, and `h`
// for `ℏ` would be a different constant rather than a different name, so
// `\hbar` is left to fall back to its source.

/// Put the renamed symbols under the names this Typst knows them by.
fn retarget(converted: &str) -> String {
    let mut out = converted.to_string();

    for (old, new) in RENAMED {
        out = replace_symbol(&out, old, new);
    }

    out
}

/// Replace one symbol path, and only where it is the whole of one.
///
/// A modifier may follow — `plus.circle.big` is still the symbol being renamed
/// — but a longer name that merely begins the same way is not.
fn replace_symbol(text: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find(old) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + old.len()..].chars().next();

        let starts = !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
        let ends = !after.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');

        out.push_str(&rest[..at]);
        out.push_str(if starts && ends { new } else { old });

        rest = &rest[at + old.len()..];
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_what_typst_writes_differently() {
        let out = typst(r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}").expect("converts");

        assert!(out.contains("sum _(i = 1 )^(n )"), "{out}");
        assert!(out.contains("frac("), "{out}");
    }

    #[test]
    fn calls_a_handler_the_prelude_defines() {
        let out = typst(r"\sqrt{4}").expect("converts");

        assert!(out.contains("mitexsqrt"), "{out}");
        assert!(PRELUDE.contains("#let mitexsqrt"), "the prelude defines it");
    }

    #[test]
    fn puts_a_renamed_symbol_under_its_current_name() {
        let out = typst(r"a \oplus b \quad \bigotimes \quad \langle x \rangle").expect("converts");

        assert!(out.contains("plus.o"), "{out}");
        assert!(
            out.contains("times.o.big"),
            "the modifier comes along: {out}"
        );
        assert!(
            out.contains("chevron.l") && out.contains("chevron.r"),
            "{out}"
        );
        assert!(!out.contains("circle"), "{out}");
    }

    #[test]
    fn leaves_a_name_that_merely_begins_the_same_way() {
        assert_eq!(
            replace_symbol("angle.l", "angle.l", "chevron.l"),
            "chevron.l"
        );
        assert_eq!(
            replace_symbol("angle.left", "angle.l", "chevron.l"),
            "angle.left"
        );
        assert_eq!(
            replace_symbol("triangle.l", "angle.l", "chevron.l"),
            "triangle.l"
        );
        assert_eq!(
            replace_symbol("angle.l.big", "angle.l", "chevron.l"),
            "chevron.l.big"
        );
    }

    #[test]
    fn declines_what_it_cannot_read() {
        assert!(typst(r"\thiscommanddoesnotexist{x}").is_none());
        assert!(typst("   ").is_none());
    }
}
