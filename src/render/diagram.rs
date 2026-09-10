//! Mermaid diagrams.
//!
//! A diagram is delivered as its source in a `<pre class="mermaid">`, drawn in
//! the browser by the mermaid module. Nothing is rendered at build time: doing
//! that would mean shelling out to a headless browser the way
//! `asciidoctor-diagram` does, and it would put a build dependency in the way
//! of what is otherwise a self-contained binary. It also degrades honestly — a
//! reader with no scripts sees the source of the diagram rather than a gap.
//!
//! The module itself is vendored and compiled in, so a rendered page reaches no
//! further than the machine that rendered it. The server hands it out from its
//! own reserved path, a file render writes it beside the page, and output with
//! nowhere to put a file beside it carries the module in the page.
//! `--mermaid-url` overrides all three.

use crate::render::html::escape_attr;

/// The mermaid module, vendored so that a rendered page needs no network.
///
/// This is the UMD build rather than the ESM one: the ESM entry point is a
/// loader that pulls in several dozen chunk files at run time, which is only
/// vendorable as a whole directory, while the UMD build is one self-contained
/// file that defines `window.mermaid`.
pub const BUNDLE: &str = include_str!("../../vendor/mermaid/mermaid.min.js");

/// Directory a file render puts its assets in, relative to the page.
pub const ASSET_DIR: &str = "adocers-assets";

/// File name the bundle is written and served under.
///
/// The version is part of the name on purpose. The bundle cannot change under a
/// given name, so it is served with a long lifetime, and a browser holding the
/// old copy would never look again if an upgrade reused the name.
/// `vendor/mermaid/README.md` records where it came from and how to replace it.
pub const BUNDLE_FILE: &str = "mermaid-12.0.0.min.js";

/// Where a page written to a file looks for the bundle beside it.
pub const ASSET_HREF: &str = "adocers-assets/mermaid-12.0.0.min.js";

/// Where a page gets the drawing module from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Source {
    /// Loaded from a URL. The server points this at its own reserved path and
    /// a file render at the copy written beside the page; `--mermaid-url`
    /// points it wherever the author likes.
    Url(String),

    /// Carried in the page itself, for output with nowhere to put a file beside
    /// it — a document written to standard output, most of all.
    Inline,
}

/// The script that draws every diagram on the page, preceded by the module it
/// needs.
///
/// The drawing pass keeps each diagram's source so it can run again when the
/// reader's colour scheme changes: mermaid bakes the theme into the SVG it
/// produces, so a diagram drawn once would stay light on a page that has since
/// gone dark.
pub fn script(source: &Source) -> String {
    let module = match source {
        Source::Url(url) => format!("<script src=\"{}\"></script>", escape_attr(url)),
        Source::Inline => format!("<script>{}</script>", escape_closing_tag(BUNDLE)),
    };

    format!(
        r#"{module}
<script>
(function () {{
  var mermaid = window.mermaid;

  if (!mermaid) {{
    // The module did not load. Each diagram stays as its own source, which is
    // more use to a reader than an empty space would be.
    return;
  }}

  var diagrams = Array.prototype.slice.call(document.querySelectorAll("pre.mermaid"));
  diagrams.forEach(function (diagram) {{
    diagram.dataset.source = diagram.textContent;
  }});

  var dark = window.matchMedia("(prefers-color-scheme: dark)");

  function draw() {{
    mermaid.initialize({{ startOnLoad: false, theme: dark.matches ? "dark" : "default" }});

    diagrams.forEach(function (diagram) {{
      diagram.removeAttribute("data-processed");
      diagram.textContent = diagram.dataset.source;
    }});

    return mermaid.run({{ nodes: diagrams }}).then(function () {{
      // Mermaid sizes a diagram to fill its container, which scales a wide one
      // down until its labels are unreadable — a 3000px flowchart in a 760px
      // column ends up a quarter size. Give each diagram back the size its own
      // viewBox asks for and let the container scroll instead.
      diagrams.forEach(function (diagram) {{
        var svg = diagram.querySelector("svg");
        var box = svg && svg.viewBox.baseVal;

        if (box && box.width) {{
          svg.setAttribute("width", box.width);
          svg.setAttribute("height", box.height);
          svg.style.maxWidth = "none";
        }}
      }});
    }});
  }}

  draw();
  dark.addEventListener("change", draw);
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
fn escape_closing_tag(script: &str) -> String {
    script.replace("</script", "<\\/script")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_href_names_the_file_that_is_written() {
        assert_eq!(ASSET_HREF, format!("{ASSET_DIR}/{BUNDLE_FILE}"));
    }

    #[test]
    fn the_asset_name_carries_a_version() {
        // A name reused across an upgrade would leave every browser that
        // cached the old bundle holding it forever.
        assert!(BUNDLE_FILE.contains(char::is_numeric));
    }

    #[test]
    fn the_vendored_bundle_defines_the_global_the_script_uses() {
        assert!(BUNDLE.contains("mermaid"));
        assert!(BUNDLE.len() > 1_000_000, "that is not the whole bundle");
    }

    #[test]
    fn inlined_script_cannot_close_its_own_element() {
        let escaped = escape_closing_tag(r#"var s = "</script><script>alert(1)</script>";"#);

        assert!(!escaped.contains("</script"));
        assert!(escaped.contains(r"<\/script"));
    }

    #[test]
    fn a_url_is_escaped_into_the_src_attribute() {
        let script = script(&Source::Url("a\"><script>alert(1)</script>".to_string()));

        assert!(script.contains("src=\"a&quot;&gt;&lt;script&gt;"));
    }
}
