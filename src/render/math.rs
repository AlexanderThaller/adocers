//! Mathematics, typeset in the browser by `MathJax`.
//!
//! A stem block reaches the page as the notation the author wrote, wrapped in
//! the delimiters Asciidoctor uses — `\$…\$` for `AsciiMath`, `\[…\]` for LaTeX
//! — and `MathJax` turns it into an equation. Nothing is typeset at build time,
//! for the same reason no diagram is drawn at build time: it would mean a
//! headless browser in the way of an otherwise self-contained binary. It also
//! degrades honestly, since a reader with no scripts sees the notation rather
//! than a gap.
//!
//! The module is vendored and compiled in, so a rendered page reaches no
//! further than the machine that rendered it. `--mathjax-url` overrides that,
//! and the `math` feature leaves it out of the binary altogether.
//!
//! Each block is converted by hand rather than by letting `MathJax` scan the
//! page for delimiters. `MathJax`'s scanner reads the backslash in `\$` as an
//! escape and leaves a stray dollar sign behind; this back end already knows
//! which elements are equations and which notation each is in, so it says so
//! outright.

use crate::render::html::escape_attr;

/// The `MathJax` module, vendored so that a rendered page needs no network.
///
/// This is the SVG output build. The CHTML builds fetch web fonts at run time,
/// which is the one thing vendoring is meant to prevent; the SVG build draws
/// its own glyphs and needs nothing but the script.
#[cfg(feature = "math")]
pub const BUNDLE: &str = include_str!("../../vendor/mathjax/tex-mml-svg.js");

/// The `AsciiMath` input processor, which the main bundle does not include.
///
/// `MathJax` 3 ships no combined build with `AsciiMath` in it, and `AsciiMath`
/// is what a plain `:stem:` means, so it travels as a second file. `MathJax`'s
/// loader looks for it relative to the main bundle, which is why the two keep
/// their directory layout wherever they are written or served.
#[cfg(feature = "math")]
pub const ASCIIMATH: &str = include_str!("../../vendor/mathjax/input/asciimath.js");

/// Directory a file render puts its assets in, relative to the page.
pub const ASSET_DIR: &str = "adocers-assets";

/// File name the bundle is written and served under.
///
/// The version is part of the name on purpose: the bundle cannot change under
/// a given name, so it is served with a long lifetime, and a browser holding
/// the old copy would never look again if an upgrade reused the name.
#[cfg(feature = "math")]
pub const BUNDLE_FILE: &str = "mathjax-3.2.2-tex-mml-svg.js";

/// Path the `AsciiMath` processor is written and served under, relative to the
/// bundle. `MathJax`'s loader builds this path itself; it cannot be renamed.
#[cfg(feature = "math")]
pub const ASCIIMATH_FILE: &str = "input/asciimath.js";

/// Where a page written to a file looks for the bundle beside it.
pub const ASSET_HREF: &str = "adocers-assets/mathjax-3.2.2-tex-mml-svg.js";

/// Where a page gets the typesetting module from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Loaded from a URL. The server points this at its own reserved path and
    /// a file render at the copy written beside the page; `--mathjax-url`
    /// points it wherever the author likes.
    Url(String),

    /// Carried in the page itself, for output with nowhere to put a file
    /// beside it — a document written to standard output, most of all.
    ///
    /// `AsciiMath` is not typeset in this form. Its processor is a second file
    /// that `MathJax` insists on fetching, and an inlined page has no URL to
    /// fetch it from, so a page built this way waits forever if it asks. LaTeX
    /// is in the main bundle and works.
    Inline,
}

/// The script that typesets every equation on the page, preceded by the module
/// it needs.
pub fn script(source: &Source) -> String {
    // The AsciiMath processor is only asked for when there is somewhere to
    // fetch it from. Asking with no URL leaves MathJax waiting on a load that
    // never finishes, which would cost the page its LaTeX as well.
    let (module, load) = match source {
        Source::Url(url) => (
            format!("<script src=\"{}\"></script>", escape_attr(url)),
            "\"input/asciimath\"",
        ),

        // Without the `math` feature there is no vendored module to inline,
        // so the page carries the notation and nothing else.
        #[cfg(feature = "math")]
        Source::Inline => (
            format!("<script>{}</script>", escape_closing_tag(BUNDLE)),
            "",
        ),

        #[cfg(not(feature = "math"))]
        Source::Inline => return String::new(),
    };

    format!(
        r#"<script>
window.MathJax = {{
  loader: {{ load: [{load}] }},
  svg: {{ fontCache: "local" }},
  startup: {{ typeset: false }}
}};
</script>
{module}
<script>
(function () {{
  var MathJax = window.MathJax;

  if (!MathJax || !MathJax.startup) {{
    // The module did not load. Each equation stays as the notation it was
    // written in, which is more use to a reader than an empty space.
    return;
  }}

  MathJax.startup.promise.then(function () {{
    // Typesetting by hand skips the pass that installs MathJax's own styles,
    // and one of the things they hide is the MathML it puts beside every
    // equation for a screen reader. Without them each equation is shown twice.
    if (MathJax.svgStylesheet) {{
      document.head.appendChild(MathJax.svgStylesheet());
    }}

    var blocks = Array.prototype.slice.call(
      document.querySelectorAll(".stemblock > .content")
    );

    return Promise.all(blocks.map(function (block) {{
      var source = block.textContent.trim();

      // The delimiters say which notation this is. They are the author's, put
      // there by the renderer, so an equation that carries neither is left
      // exactly as it was written.
      var latex = source.slice(0, 2) === "\\[" && source.slice(-2) === "\\]";
      var ascii = source.slice(0, 2) === "\\$" && source.slice(-2) === "\\$";

      if (!latex && !ascii) {{
        return null;
      }}

      if (ascii && !MathJax.asciimath2svgPromise) {{
        return null;
      }}

      var body = source.slice(2, -2);
      var typeset = latex
        ? MathJax.tex2svgPromise(body, {{ display: true }})
        : MathJax.asciimath2svgPromise(body, {{ display: true }});

      return typeset.then(function (rendered) {{
        block.textContent = "";
        block.appendChild(rendered);
      }});
    }}));
  }});
}})();
</script>"#
    )
}

/// Neutralize any `</script` inside JavaScript that is about to be inlined.
///
/// An HTML parser ends a `<script>` element at the first `</script`, wherever
/// it falls — inside a string literal or a regular expression included.
/// `<\/script` is the same text to a JavaScript parser, which reads `\/` as `/`
/// in both places, and is no longer a closing tag to an HTML one.
#[cfg(feature = "math")]
fn escape_closing_tag(script: &str) -> String {
    script.replace("</script", "<\\/script")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_for_asciimath_only_when_it_can_be_fetched() {
        let url = script(&Source::Url("/m.js".to_string()));
        assert!(url.contains("\"input/asciimath\""), "{url}");

        assert!(
            !script(&Source::Inline).contains("input/asciimath"),
            "an inlined page has nowhere to fetch it from"
        );
    }

    #[cfg(feature = "math")]
    #[test]
    fn closes_no_script_element_early() {
        assert!(!script(&Source::Inline).contains("</script>MathJax"));
        assert!(escape_closing_tag("a</script>b").contains("<\\/script"));
    }
}
