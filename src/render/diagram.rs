//! Mermaid diagrams.
//!
//! A diagram is delivered as its source in a `<pre class="mermaid">`, drawn in
//! the browser by the mermaid module. Nothing is rendered at build time: doing
//! that would mean shelling out to a headless browser the way
//! `asciidoctor-diagram` does, and it would put a build dependency in the way
//! of what is otherwise a self-contained binary. It also degrades honestly — a
//! reader with no scripts sees the source of the diagram rather than a gap.

use std::fmt::Write as _;

/// Where the mermaid module is fetched from when the caller names no other.
///
/// The major version is pinned but the minor is not, so a page picks up fixes
/// without being able to change mermaid's diagram syntax underneath a document.
pub const DEFAULT_URL: &str = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs";

/// The script that draws every diagram on the page.
///
/// It keeps each diagram's source so it can draw them again when the reader's
/// colour scheme changes: mermaid bakes the theme into the SVG it produces, so
/// a diagram drawn once would stay light on a page that has gone dark.
pub fn script(url: &str) -> String {
    format!(
        r#"<script type="module">
import mermaid from "{url}";

const diagrams = Array.from(document.querySelectorAll("pre.mermaid"));
for (const diagram of diagrams) {{
  diagram.dataset.source = diagram.textContent;
}}

const dark = window.matchMedia("(prefers-color-scheme: dark)");

async function draw() {{
  mermaid.initialize({{ startOnLoad: false, theme: dark.matches ? "dark" : "default" }});

  for (const diagram of diagrams) {{
    diagram.removeAttribute("data-processed");
    diagram.textContent = diagram.dataset.source;
  }}

  await mermaid.run({{ nodes: diagrams }});

  // Mermaid sizes a diagram to fill its container, which scales a wide one
  // down until its labels are unreadable — a 3000px flowchart in a 760px
  // column ends up a quarter size. Give each diagram back the size its own
  // viewBox asks for and let the container scroll instead.
  for (const diagram of diagrams) {{
    const svg = diagram.querySelector("svg");
    const box = svg && svg.viewBox.baseVal;

    if (box && box.width) {{
      svg.setAttribute("width", box.width);
      svg.setAttribute("height", box.height);
      svg.style.maxWidth = "none";
    }}
  }}
}}

draw();
dark.addEventListener("change", draw);
</script>"#,
        url = escape_js(url)
    )
}

/// Escape a string for use inside a double-quoted JavaScript literal.
///
/// `<` is escaped along with the obvious characters, because a `</script>` in
/// the value would otherwise end the element early no matter how well the
/// quoting held up.
fn escape_js(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\x3C"),
            '\n' | '\r' => {}
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_cannot_break_out_of_the_script() {
        let escaped = escape_js("https://example.com/a\"</script><script>alert(1)</script>");

        assert!(
            !escaped.contains('<'),
            "a `<` would end the script element whatever the quoting did"
        );

        // Removing the escaped quotes must leave none behind, or one of them
        // was free to close the literal.
        assert!(!escaped.replace("\\\"", "").contains('"'));
    }

    #[test]
    fn an_ordinary_url_survives_unchanged() {
        assert_eq!(escape_js(DEFAULT_URL), DEFAULT_URL);
    }
}
