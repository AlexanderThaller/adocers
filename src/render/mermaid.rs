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

#[cfg(feature = "pdf")]
use merman::svg::SvgPipeline;
use merman::{
    OperationControl,
    RenderOutput,
    RenderRequest,
    Renderer,
    SvgRequest,
};

/// The page's own colours, applied to the parts of a diagram that are chrome
/// rather than data.
///
/// Each entry is a selector list and the declarations to put on it. They are
/// kept apart because only the selectors may be scoped to the diagram, and a
/// scope has to go in front of a whole selector rather than in front of every
/// class in it: `text.actor` scoped by hand becomes `text#id .actor`, which
/// matches nothing, and `opacity:0.9` is not a selector at all.
///
/// The rules are emitted after `merman`'s own stylesheet and scoped to the same
/// id, so these win on order without needing `!important` or a specificity
/// fight.
///
/// Substituting colour literals instead does not work, and it is worth saying
/// why: mermaid paints a flowchart node and the first slice of a pie chart the
/// same `#ECECFF`. One is chrome and the other is data, and by the time it is a
/// colour there is nothing left to tell them apart. Only the class says which
/// is which — so a pie keeps its palette and a git graph keeps its branch
/// colours, while the boxes, lines and labels around them follow the reader.
const THEME: &[(&str, &str)] = &[
    // What anything the rest of this table does not name falls back to.
    // Mermaid's own default is `#333`, which is all but invisible on a dark
    // page. An element carrying its own `fill` — a pie slice, a git branch —
    // keeps it, because a presentation attribute outranks an inherited value.
    ("&", "fill:var(--fg);"),
    (
        ".flowchartTitleText,.gitTitleText,.statediagramTitleText,.classTitleText",
        "fill:var(--fg);",
    ),
    // Flowcharts.
    (
        ".node rect,.node circle,.node ellipse,.node polygon,.node path",
        "fill:var(--code-bg);stroke:var(--accent);",
    ),
    (".node .katex path", "fill:var(--fg);stroke:var(--fg);"),
    // A git graph writes each branch's name on a chip painted in that branch's
    // colour, and picks the name's colour to suit the chip — data, both of
    // them, so the label rules step around that one group and leave mermaid's
    // cascade to decide it.
    (
        ".nodeLabel,.nodeLabel p,.label:not([class*=\"branch-label\"]),         \
         .label:not([class*=\"branch-label\"]) text,.label span,.labelText",
        "fill:var(--fg);color:var(--fg);",
    ),
    (
        ".edgePaths .path,.edgePath .path,.flowchart-link,.transition,.relation",
        "stroke:var(--muted);",
    ),
    (
        ".edgeLabel,.edgeLabel p,.labelBkg",
        "background-color:var(--bg);color:var(--fg);",
    ),
    (".edgeLabel rect,.edgeLabel .label rect", "fill:var(--bg);"),
    (
        ".label div .edgeLabel,.edgeLabel .label text",
        "fill:var(--fg);color:var(--fg);",
    ),
    (
        ".cluster rect,.statediagram-cluster rect,.statediagram-cluster.statediagram-cluster \
         .inner",
        "fill:var(--sidebar-bg);stroke:var(--rule);",
    ),
    (
        ".cluster text,.cluster span,.cluster-label text,.cluster-label span",
        "fill:var(--fg);color:var(--fg);",
    ),
    // Arrowheads. Mermaid draws them inside `<marker>` elements named after the
    // diagram rather than given a class, so they are reached through the
    // element instead: every marker in a diagram is an arrow of some kind, and
    // all of them are chrome. The `[id]` is not there to narrow anything — a
    // marker without one would be unusable — but to match the specificity of
    // the `[id$="-arrowhead"]` rules it is overriding, which order alone would
    // not be enough to beat.
    (
        ".arrowheadPath,.marker,marker[id] path,marker[id] circle",
        "fill:var(--muted);stroke:var(--muted);",
    ),
    // A class diagram's relation ends are the one place mermaid reaches for
    // `!important`, so outranking them means matching it. Which of them are
    // filled and which are hollow is left alone: that is the difference
    // between composition and aggregation, and it is being read, not decorated.
    (
        ".composition,.dependency",
        "fill:var(--muted)!important;stroke:var(--muted)!important;",
    ),
    (".extension,.aggregation", "stroke:var(--muted)!important;"),
    (
        ".lollipop",
        "fill:var(--code-bg)!important;stroke:var(--accent)!important;",
    ),
    // The number sits on a marker's disc, so it takes the page's ground.
    (".sequenceNumber", "fill:var(--bg);stroke:none;"),
    // Sequence diagrams. The boxes and their labels both carry `.actor`, so the
    // element has to say which of the two is being coloured.
    (
        "rect.actor,.actor-man circle,.actor-man line,.labelBox",
        "fill:var(--code-bg);stroke:var(--accent);",
    ),
    ("text.actor,text.actor>tspan", "fill:var(--fg);stroke:none;"),
    (".actor-line", "stroke:var(--rule);"),
    (
        ".messageText,.loopText,.loopText>tspan,.noteText,.noteText>tspan,.sectionTitle,.\
         sectionTitle>tspan",
        "fill:var(--fg);stroke:none;",
    ),
    (".messageLine0,.messageLine1", "stroke:var(--muted);"),
    (
        ".note,g rect.rect",
        "fill:var(--sidebar-bg);stroke:var(--rule);",
    ),
    (".loopLine", "stroke:var(--rule);"),
    (
        ".activation0,.activation1,.activation2",
        "fill:var(--sidebar-bg);stroke:var(--rule);",
    ),
    // State and class diagrams.
    (
        "g.stateGroup rect,g.classGroup rect,.stateLabel .box,.classLabel .box,.stateGroup \
         .composit,.stateGroup .alt-composit,.statediagram-state rect.divider,.end-state-inner",
        "fill:var(--code-bg);stroke:var(--accent);",
    ),
    ("g.stateGroup text", "fill:var(--fg);stroke:none;"),
    ("g.stateGroup line", "stroke:var(--muted);"),
    // Solid on purpose: these are the filled discs a state machine starts and
    // forks at, not boxes with something written in them.
    (
        ".node circle.state-start,.node .fork-join",
        "fill:var(--fg);stroke:var(--fg);",
    ),
    (
        ".state-note,.statediagram-note rect",
        "fill:var(--sidebar-bg);stroke:var(--rule);",
    ),
    (
        ".state-note text,.statediagram-note text,.statediagram-note .nodeLabel,.noteLabel \
         .nodeLabel,.noteLabel .edgeLabel",
        "fill:var(--fg);color:var(--fg);",
    ),
    // Pie charts and git graphs: the labels that sit on the page follow it, and
    // the ones that sit on the data follow the data. A slice's percentage is
    // written across the wedge, in mermaid's palette rather than the reader's,
    // so it stays where mermaid put it.
    (".pieTitleText,.legend text", "fill:var(--fg);"),
    // A slice is data; the hairline separating one from the next is not, and
    // mermaid draws it in black, which on a dark page reads as a gap.
    (".pieOuterCircle", "stroke:var(--rule);"),
    (".pieCircle", "stroke:var(--bg);"),
    (".branch", "stroke:var(--muted);"),
    (".tag-hole", "fill:var(--bg);"),
    // A commit's hash sits on the page, on a card of mermaid's own.
    (".commit-label", "fill:var(--fg);"),
    (
        ".commit-label-bkg,.tag-label-bkg",
        "fill:var(--sidebar-bg);opacity:0.9;",
    ),
];

/// [`THEME`] as a stylesheet, with every selector scoped to the diagram `id`.
///
/// `&` stands for the diagram element itself, as it does in a nested
/// stylesheet; anything else is taken to be a descendant of it.
fn theme(id: &str) -> String {
    let mut css = String::new();

    for (selectors, declarations) in THEME {
        for (nth, selector) in selectors.split(',').enumerate() {
            if nth > 0 {
                css.push(',');
            }

            css.push('#');
            css.push_str(id);

            match selector.trim() {
                "&" => {}

                selector => {
                    css.push(' ');
                    css.push_str(selector);
                }
            }
        }

        css.push('{');
        css.push_str(declarations);
        css.push('}');
    }

    css
}

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
    let theme = format!("<style>{}</style>", theme(id));

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
    fn colours_a_sequence_diagram_s_labels_and_arrows() {
        let svg = svg(
            "sequenceDiagram\n  A->>B: hello\n  Note over A,B: a note",
            "d1",
        )
        .expect("renders");

        // The actor labels and the actor boxes both carry `.actor`, and a
        // scope written in front of every class turns `text.actor` into
        // `text#d1 .actor`, which matches nothing — leaving the labels black on
        // whatever the boxes were filled with.
        assert!(
            svg.contains("#d1 text.actor"),
            "an actor's label should be scoped by element, not by class"
        );
        assert!(
            !svg.contains("text#d1"),
            "the scope goes before the selector"
        );

        // Arrowheads live in markers named after the diagram rather than given
        // a class, so a class-only theme leaves them at mermaid's near-black.
        assert!(
            svg.contains("#d1 marker[id] path"),
            "arrowheads should follow the page"
        );
    }

    #[test]
    fn leaves_the_declarations_alone() {
        let svg = svg("gitGraph\n  commit", "d1").expect("renders");

        assert!(
            svg.contains("opacity:0.9"),
            "a decimal point is not a class"
        );
    }

    #[test]
    fn declines_what_it_cannot_read() {
        assert!(svg("this is not a diagram at all", "d1").is_none());
    }
}
