//! Drawing a mermaid diagram here rather than in the reader's browser.
//!
//! [`merman`](https://crates.io/crates/merman) parses and lays out mermaid in
//! Rust, so a diagram can become an `<svg>` while the page is being built. What
//! that saves is not really bytes — a diagram is a few kilobytes either way —
//! but the 5.6 MB drawing module that would otherwise have to reach the reader
//! and run before they see anything.
//!
//! Two things have to be corrected on the way out, both of them consequences of
//! `merman` reproducing mermaid's own markup faithfully:
//!
//! - Every diagram it draws carries `id="merman"`, and its stylesheet is scoped
//!   to that id. Two diagrams on a page would be two elements with the same id,
//!   which is invalid, so each is renamed on the way out.
//!
//! - The colours are mermaid's light palette. A page that follows the reader's
//!   colour scheme cannot use them as they are, so a second stylesheet is
//!   appended that puts the page's own colours on the *chrome* — boxes, lines,
//!   labels, backgrounds. The *data* colours are left alone, because the slices
//!   of a pie and the branches of a git graph are telling the reader something
//!   and are not the page's to recolour.
//!
//! A diagram bound for a PDF takes neither correction and one of its own: see
//! [`printable`].

use merman::{
    OperationControl,
    RenderOutput,
    RenderRequest,
    Renderer,
    SvgRequest,
    svg::SvgPipeline,
};

/// The page's own colours, applied to the parts of a diagram that are chrome
/// rather than data.
///
/// Appended after `merman`'s stylesheet and scoped to the same id, so these win
/// on order without needing `!important` or a specificity fight.
///
/// Substituting colour literals instead does not work, and it is worth saying
/// why: mermaid paints a flowchart node and the first slice of a pie chart the
/// same `#ECECFF`. One is chrome and the other is data, and by the time it is a
/// colour there is nothing left to tell them apart. Only the class says which
/// is which — so a pie keeps its palette and a git graph keeps its branch
/// colours, while the boxes, lines and labels around them follow the reader.
const THEME: &str =
    "\
.node rect,.node circle,.node ellipse,.node polygon,.node \
     path{fill:var(--code-bg);stroke:var(--accent);}.nodeLabel,.nodeLabel p,.label,.label \
     text,.labelText{fill:var(--fg);color:var(--fg);}.edgePath \
     .path,.flowchart-link{stroke:var(--muted);}.arrowheadPath,.marker{fill:var(--muted);stroke:\
     var(--muted);}.edgeLabel,.edgeLabel p,.edgeLabel \
     rect{fill:var(--bg);color:var(--fg);background-color:var(--bg);}.cluster \
     rect{fill:var(--sidebar-bg);stroke:var(--rule);}.cluster text,.cluster \
     span{fill:var(--fg);color:var(--fg);}.actor{fill:var(--code-bg);stroke:var(--accent);}.\
     actor-line{stroke:var(--rule);}text.actor,text.actor>tspan{fill:var(--fg);stroke:none;}.\
     messageText,.labelText,.loopText,.loopText>tspan,.noteText,.noteText>tspan{fill:var(--fg);\
     stroke:none;}.messageLine0,.messageLine1{stroke:var(--muted);}.note{fill:var(--sidebar-bg);\
     stroke:var(--rule);}.labelBox{fill:var(--code-bg);stroke:var(--accent);}.loopLine{stroke:\
     var(--rule);}.activation0,.activation1,.activation2{fill:var(--sidebar-bg);stroke:\
     var(--rule);}.pieTitleText,.slice,.legend \
     text{fill:var(--fg);}.pieOuterCircle{stroke:var(--rule);}.commit-label,.branch-label{fill:\
     var(--fg);}.commit-label-bkg,.branch-label-bkg{fill:var(--sidebar-bg);opacity:0.9;}";

/// Draw `source` as an SVG, or `None` if `merman` cannot read it.
///
/// A diagram it declines is not an error: the caller falls back to handing the
/// source to the browser, which is what every diagram used to do.
///
/// `id` has to be unique within the page.
pub fn svg(source: &str, id: &str) -> Option<String> {
    Some(adapt(&draw(source, SvgRequest::default())?, id))
}

/// Draw `source` for something that is not a browser.
///
/// Mermaid sets its labels in `<foreignObject>`, which is HTML inside the SVG
/// and needs a browser to lay out. `merman` can put SVG text there instead, at
/// the cost of exact parity, which is the difference between a diagram of
/// labelled boxes and a diagram of empty ones once it reaches a PDF.
///
/// The colours are left as mermaid drew them. The page's palette follows the
/// reader's colour scheme and a printed page has no reader to follow.
#[cfg(feature = "pdf")]
pub fn printable(source: &str) -> Option<String> {
    draw(
        source,
        SvgRequest {
            pipeline: Some(SvgPipeline::resvg_safe()),
            ..SvgRequest::default()
        },
    )
}

/// One render, however it was asked for.
fn draw(source: &str, request: SvgRequest) -> Option<String> {
    let output = Renderer::new()
        .render(RenderRequest::svg(source, OperationControl::new(), request))
        .ok()?;

    let RenderOutput::Svg(Some(rendered)) = output else {
        return None;
    };

    Some(rendered.svg().to_string())
}

/// Rename the diagram and remap its chrome onto the page's colours.
///
/// The name is not only on the element. It prefixes the stylesheet's selectors,
/// the arrow markers, the drop shadow filter, and every `url(#…)` that points
/// at one of them, so it is replaced wherever it appears — which keeps all of
/// those references pointing at the right diagram once there are several on a
/// page.
fn adapt(svg: &str, id: &str) -> String {
    let out = svg.replace("merman", id);

    // The white ground is an inline `style` on the element itself, which a
    // stylesheet cannot outrank without `!important`, so it is corrected here.
    let out = out.replace("background-color: white", "background-color: transparent");

    // Appended inside the element, after the stylesheet it is overriding.
    let theme = format!("<style>{}</style>", THEME.replace('.', &format!("#{id} .")));

    match out.find("</style>") {
        Some(at) => {
            let at = at + "</style>".len();
            format!("{}{theme}{}", &out[..at], &out[at..])
        }

        None => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_flowchart() {
        let svg = svg("flowchart LR\n  A[one] --> B[two]", "d1").expect("renders");

        assert!(svg.starts_with("<svg"), "{}", &svg[..60.min(svg.len())]);
        assert!(svg.contains("one") && svg.contains("two"));
    }

    #[test]
    fn gives_each_diagram_its_own_name() {
        let svg = svg("flowchart LR\n  A --> B", "diagram-7").expect("renders");

        assert!(svg.contains("id=\"diagram-7\""));

        // Not just the element: the markers and the filter carry the name too,
        // and a `url(#…)` pointing at the wrong diagram draws nothing.
        assert!(
            !svg.contains("merman"),
            "every use of the generated name should be replaced"
        );
        assert!(svg.contains("diagram-7-drop-shadow") || svg.contains("diagram-7_flowchart"));
    }

    #[test]
    fn follows_the_page_rather_than_mermaid_s_palette() {
        let svg = svg("flowchart LR\n  A[one] --> B[two]", "d1").expect("renders");

        assert!(
            svg.contains("var(--fg)"),
            "text should take the page's colour"
        );
        assert!(
            !svg.contains("background-color: white"),
            "a light background would be wrong on a dark page"
        );
    }

    #[cfg(feature = "pdf")]
    #[test]
    fn sets_a_printed_diagram_s_labels_in_svg() {
        let svg = printable("flowchart LR\n  A[one] --> B[two]").expect("renders");

        // HTML labels need a browser to lay out, and a PDF has none, so a
        // diagram carrying them would print as empty boxes.
        assert!(!svg.contains("foreignObject"), "labels should be SVG text");
        assert!(svg.contains("one") && svg.contains("two"));
    }

    #[test]
    fn declines_what_it_cannot_read() {
        assert!(svg("this is not a diagram at all", "d1").is_none());
    }
}
