//! The stylesheet embedded in a standalone page.
//!
//! It targets Asciidoctor's class names, so a document rendered here looks
//! right without pulling in Asciidoctor's own (much larger) stylesheet, and a
//! reader who prefers a dark theme gets one.

/// The built-in stylesheet.
pub const DEFAULT: &str = r#"
:root {
  color-scheme: light dark;
  --bg: #fdfdfd;
  --fg: #1c1e21;
  --muted: #656d77;
  --rule: #dcdfe4;
  --accent: #1565a8;
  --code-bg: #f5f6f8;
  --sidebar-bg: #f2f4f7;
  /* Asciidoctor's own admonition colours. */
  --admon-note: #19407c;
  --admon-tip: #b58900;
  --admon-important: #bf6900;
  --admon-caution: #bf3400;
  --admon-warning: #bf0000;
  /* Syntax highlighting. */
  --hl-comment: #656d77;
  --hl-keyword: #cf222e;
  --hl-string: #0a3069;
  --hl-number: #0550ae;
  --hl-function: #6639ba;
  --hl-type: #953800;
  --hl-constant: #0550ae;
  --hl-property: #0550ae;
  --hl-tag: #116329;
  --hl-punctuation: #57606a;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #16181c;
    --fg: #dfe3e8;
    --muted: #9aa3ad;
    --rule: #2e3339;
    --accent: #6cb2f0;
    --code-bg: #1e2126;
    --sidebar-bg: #1e2126;
    /* Lightened, because Asciidoctor's are chosen against a white page. */
    --admon-note: #6ea8e8;
    --admon-tip: #e3b341;
    --admon-important: #e08c3e;
    --admon-caution: #f0764a;
    --admon-warning: #f0716f;
    --hl-comment: #8b949e;
    --hl-keyword: #ff7b72;
    --hl-string: #a5d6ff;
    --hl-number: #79c0ff;
    --hl-function: #d2a8ff;
    --hl-type: #ffa657;
    --hl-constant: #79c0ff;
    --hl-property: #79c0ff;
    --hl-tag: #7ee787;
    --hl-punctuation: #8b949e;
  }
}

* { box-sizing: border-box; }

body {
  margin: 0;
  background: var(--bg);
  color: var(--fg);
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  font-size: 16px;
  line-height: 1.6;
}

/* The three landmarks the page is made of. `#footnotes` is not among them: it
   sits inside `#content` so that it belongs to a landmark, and constraining it
   again there would indent it and narrow it a second time. */
#header, #content, #footer { max-width: 50rem; margin: 0 auto; padding: 0 1.25rem; }
#header { padding-top: 2.5rem; }
#footer { padding-bottom: 3rem; color: var(--muted); font-size: 0.85rem; }

/* Footnote definitions, gathered under a rule at the foot of the page. */
#footnotes { margin-top: 2.5rem; font-size: 0.9rem; color: var(--muted); }
#footnotes hr { margin: 0 0 1rem; }
#footnotes .footnote { margin-bottom: 0.5rem; padding-left: 1.5rem; text-indent: -1.5rem; }
#footnotes .footnote > a { font-weight: 600; }

h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 2rem 0 0.75rem; font-weight: 600; }
h1 { font-size: 2rem; margin-top: 0; }
h2 { font-size: 1.6rem; padding-bottom: 0.3rem; border-bottom: 1px solid var(--rule); }
h3 { font-size: 1.3rem; }
h4 { font-size: 1.1rem; }
h5, h6 { font-size: 1rem; }
h1 .subtitle { color: var(--muted); font-weight: 400; }

/* `:sectanchors:` puts an empty link before a heading, which needs a mark of
   its own to be worth anything. It sits in the margin so the heading does not
   move when it appears, and appears when the heading is under the pointer or
   the link itself has the keyboard. */
h1 .anchor, h2 .anchor, h3 .anchor, h4 .anchor, h5 .anchor, h6 .anchor {
  position: absolute; margin-left: -1em; width: 1em;
  opacity: 0; text-align: center; font-weight: 400; color: var(--muted);
}
h1 .anchor::before, h2 .anchor::before, h3 .anchor::before,
h4 .anchor::before, h5 .anchor::before, h6 .anchor::before { content: "\00A7"; }
h1:hover .anchor, h2:hover .anchor, h3:hover .anchor,
h4:hover .anchor, h5:hover .anchor, h6:hover .anchor,
.anchor:focus { opacity: 1; }

/* `:sectlinks:` makes the heading itself a link, which should still read as a
   heading rather than as something to click. */
h1 > .link, h2 > .link, h3 > .link, h4 > .link, h5 > .link, h6 > .link {
  color: inherit;
}

/* A link in a run of text is underlined rather than only coloured. The accent
   is 2.75:1 against the body colour in the light scheme and 1.76:1 in the dark
   one, and a reader who does not separate those two colours would have nothing
   else to go on. Where a whole block is links — the outline, a heading, an
   image — the underline is noise and comes off again. */
a { color: var(--accent); text-decoration: underline; text-underline-offset: 0.15em; }
a:hover { text-decoration-thickness: 2px; }
#toc a, a.image, a.anchor { text-decoration: none; }
h1 > .link, h2 > .link, h3 > .link, h4 > .link, h5 > .link, h6 > .link { text-decoration: none; }
h1 > .link:hover, h2 > .link:hover, h3 > .link:hover, h4 > .link:hover,
h5 > .link:hover, h6 > .link:hover { text-decoration: underline; }

p { margin: 0 0 1rem; }
hr { border: 0; border-top: 1px solid var(--rule); margin: 2rem 0; }

.details { color: var(--muted); font-size: 0.9rem; margin-bottom: 1.5rem; }
.details .detail { line-height: 1.5; }
.details .label { font-weight: 600; }
.details .email a { color: inherit; }
.details .tag { display: inline-block; padding: 0.02em 0.5em; border: 1px solid var(--rule); border-radius: 999px; font-size: 0.9em; }
/* The remark is a sentence about the revision, not another labelled value. */
.details .remark { margin-top: 0.75rem; font-style: italic; }
.title { font-style: italic; color: var(--muted); margin-bottom: 0.4rem; font-size: 0.95rem; }

/* Table of contents */
#toc { border: 1px solid var(--rule); border-radius: 6px; padding: 0.75rem 1.25rem; margin: 1.5rem 0 2rem; }
#toctitle { font-weight: 600; margin-bottom: 0.25rem; }
#toc ul { list-style: none; padding-left: 1rem; margin: 0.25rem 0; }
#toc > ul { padding-left: 0; }

/* Code */
code, pre { font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; font-size: 0.9em; }
code { background: var(--code-bg); padding: 0.1em 0.35em; border-radius: 3px; }
pre { background: var(--code-bg); padding: 0.9rem 1rem; border-radius: 6px; overflow-x: auto; line-height: 1.45; }
pre code { background: none; padding: 0; }
.listingblock, .literalblock { margin-bottom: 1.25rem; }

/* Syntax highlighting. Several tree-sitter captures share a colour on purpose:
   a builtin function is still a function to a reader, and a listing that picks
   out everything picks out nothing. */
.hl-comment { color: var(--hl-comment); font-style: italic; }
.hl-keyword { color: var(--hl-keyword); }
.hl-string { color: var(--hl-string); }
.hl-number { color: var(--hl-number); }
.hl-constant { color: var(--hl-constant); }
.hl-function { color: var(--hl-function); }
.hl-type { color: var(--hl-type); }
.hl-property { color: var(--hl-property); }
.hl-attr { color: var(--hl-property); }
.hl-tag { color: var(--hl-tag); }
.hl-escape { color: var(--hl-number); }
.hl-label { color: var(--hl-type); }
.hl-punctuation, .hl-operator { color: var(--hl-punctuation); }
/* Variables and parameters are left in the body colour: they are most of a
   listing, and colouring them leaves nothing for the eye to catch on. */
.hl-variable, .hl-parameter, .hl-unknown { color: inherit; }
/* A callout keeps its own weight against whatever the highlighter did. */
.listingblock .conum { color: var(--accent); font-weight: 600; font-style: normal; }

/* A callout mark, drawn rather than written: Asciidoctor's markup for it names
   a Font Awesome glyph, and a page this tool produced has no way to pull in
   that webfont. The number is in the markup as data, so a circle and a counter
   are the whole of it, and the `(1)` beside it — which is what a page with no
   stylesheet shows — is hidden once the mark is drawn. */
.conum[data-value] {
  display: inline-block; width: 1.3em; height: 1.3em; line-height: 1.3em;
  border-radius: 50%; background: var(--accent); color: var(--bg);
  text-align: center; font-family: inherit; font-size: 0.8em; font-style: normal;
  font-weight: 600; vertical-align: 0.05em;
}
.conum[data-value]::after { content: attr(data-value); }
.conum[data-value] + b { display: none; }
/* The list beneath the block marks its items the same way, so an item and the
   line it annotates carry the same mark. */
.colist.conums > ol { list-style: none; padding-left: 0; counter-reset: conum; }
.colist.conums > ol > li { counter-increment: conum; position: relative; padding-left: 2em; }
.colist.conums > ol > li::before {
  content: counter(conum); position: absolute; left: 0; top: 0.1em;
  display: inline-block; width: 1.3em; height: 1.3em; line-height: 1.3em;
  border-radius: 50%; background: var(--accent); color: var(--bg);
  text-align: center; font-size: 0.8em; font-weight: 600;
}

/* Quotes */
.quoteblock { margin: 1.25rem 0; }
.quoteblock blockquote { margin: 0; padding: 0.25rem 0 0.25rem 1.25rem; border-left: 4px solid var(--rule); color: var(--fg); }
.verseblock pre.content { background: none; padding: 0 0 0 1.25rem; border-left: 4px solid var(--rule); white-space: pre-wrap; }
.attribution { color: var(--muted); font-size: 0.9rem; margin-top: 0.5rem; }
.attribution cite { font-style: italic; }

/* A stem block is a displayed equation: it stands apart from the text the way a
   figure does, and a long one scrolls rather than pushing the column wide. Until
   a maths renderer is loaded into the page it shows its own delimiters, which is
   the honest thing for it to show. */
.stemblock { margin: 1.5rem 0; text-align: center; }
.stemblock > .content { overflow-x: auto; }

/* Admonitions, laid out as Asciidoctor lays them out: a wide centred icon
   column, a rule between it and the text, and no box around the whole thing. */
.admonitionblock { margin: 1.4rem 0; }
.admonitionblock > table { width: 100%; border-collapse: separate; border: 0; background: none; }
/* The icon is centred against the whole admonition rather than sitting at its
   top: the mark stands for the block, and a tall one left it stranded. */
.admonitionblock td.icon { width: 80px; text-align: center; vertical-align: middle; padding: 0 0.75rem 0 0; color: var(--admon); }
.admonitionblock td.icon .icon { width: 2.25rem; height: 2.25rem; }
.admonitionblock td.icon .title { font-style: normal; font-weight: 700; text-transform: uppercase; font-size: 0.8rem; letter-spacing: 0.03em; margin: 0; color: var(--admon); }
.admonitionblock td.content { padding: 0 0 0 1.125rem; border-left: 1px solid var(--rule); vertical-align: top; }
.admonitionblock td.content > :last-child { margin-bottom: 0; }
.admonitionblock td.content > .title { text-transform: uppercase; font-style: normal; font-weight: 600; }
.admonitionblock.note { --admon: var(--admon-note); }
.admonitionblock.tip { --admon: var(--admon-tip); }
.admonitionblock.important { --admon: var(--admon-important); }
.admonitionblock.caution { --admon: var(--admon-caution); }
.admonitionblock.warning { --admon: var(--admon-warning); }

/* Sidebars, examples, open blocks */
.sidebarblock { background: var(--sidebar-bg); border-radius: 6px; padding: 1rem 1.25rem; margin: 1.25rem 0; }
.exampleblock > .content { border: 1px solid var(--rule); border-radius: 6px; padding: 1rem 1.25rem; }
.exampleblock, .openblock { margin: 1.25rem 0; }
.exampleblock details > summary { cursor: pointer; }

/* Lists */
ul, ol { padding-left: 1.5rem; }
.ulist, .olist, .dlist, .colist, .qlist { margin-bottom: 1.25rem; }
.ulist li p, .olist li p, .dlist dd p { margin-bottom: 0.35rem; }
.checklist { list-style: none; padding-left: 0.5rem; }
/* The bullet styles an author can ask for by name. `unstyled` and `no-bullet`
   differ in whether the text keeps its indent; `inline` runs the items along
   one line, which suits a row of links under a heading. */
ul.square { list-style-type: square; }
ul.circle { list-style-type: circle; }
ul.disc { list-style-type: disc; }
ul.none, ul.no-bullet { list-style: none; }
ul.unstyled { list-style: none; padding-left: 0; }
ul.inline { list-style: none; padding-left: 0; display: flex; flex-wrap: wrap; gap: 0 1rem; }
ul.inline > li > p { margin: 0; }
/* A styled description list — `[glossary]`, say — carries no class on its
   terms, so the term is styled by where it sits rather than by what it is. */
.dlist dt { font-weight: 600; margin-top: 0.75rem; }
dd { margin-left: 1.5rem; }
.hdlist table { border-collapse: collapse; }
.hdlist td { padding: 0.25rem 1rem 0.25rem 0; vertical-align: top; }
.hdlist td.hdlist1 { font-weight: 600; }

/* Images and media */
.imageblock, .videoblock, .audioblock { margin: 1.5rem 0; }
.imageblock .content, .videoblock .content { text-align: center; }
img, video, audio { max-width: 100%; height: auto; }
.imageblock .title { text-align: center; margin-top: 0.5rem; }

/* Diagrams */
.diagram > .content { text-align: center; overflow-x: auto; }
pre.mermaid { background: none; padding: 0; margin: 0; text-align: center; line-height: normal; }
/* A diagram is held to the column, whatever width it was drawn at. The scroll
   on the box above is the fallback for one that cannot be made to fit. */
pre.mermaid svg { display: inline-block; vertical-align: top; max-width: 100%; height: auto; }
/* Until the drawing module runs, the source is what there is to show. */
pre.mermaid:not([data-processed]) { text-align: left; background: var(--code-bg); padding: 0.9rem 1rem; border-radius: 6px; }

/* Tables */
table.tableblock { border-collapse: collapse; margin: 1.5rem 0; font-size: 0.95rem; }
table.tableblock.stretch { width: 100%; }
table.tableblock.fit-content { width: auto; }
table.tableblock caption.title { text-align: left; margin-bottom: 0.5rem; }
table.tableblock th, table.tableblock td { padding: 0.5rem 0.75rem; vertical-align: top; }
table.tableblock th { font-weight: 600; text-align: left; }
table.tableblock p.tableblock { margin: 0; }
table.frame-all { border: 1px solid var(--rule); }
table.frame-ends { border-top: 1px solid var(--rule); border-bottom: 1px solid var(--rule); }
table.frame-sides { border-left: 1px solid var(--rule); border-right: 1px solid var(--rule); }
table.grid-all th, table.grid-all td { border: 1px solid var(--rule); }
table.grid-rows th, table.grid-rows td { border-top: 1px solid var(--rule); border-bottom: 1px solid var(--rule); }
table.grid-cols th, table.grid-cols td { border-left: 1px solid var(--rule); border-right: 1px solid var(--rule); }
table.stripes-even tbody tr:nth-child(even),
table.stripes-odd tbody tr:nth-child(odd),
table.stripes-all tbody tr { background: var(--code-bg); }
table.stripes-hover tbody tr:hover { background: var(--code-bg); }
/* A cell's own alignment has to out-specify the defaults above, or a header
   cell would silently ignore the `^` or `>` its author wrote. */
table.tableblock td.halign-center, table.tableblock th.halign-center { text-align: center; }
table.tableblock td.halign-right, table.tableblock th.halign-right { text-align: right; }
table.tableblock td.halign-left, table.tableblock th.halign-left { text-align: left; }
table.tableblock td.valign-middle, table.tableblock th.valign-middle { vertical-align: middle; }
table.tableblock td.valign-bottom, table.tableblock th.valign-bottom { vertical-align: bottom; }
/* A literal cell is still a table cell: the code-block chrome would make the
   row taller than its neighbours and push its text out of line with them. */
table.tableblock .literal pre { background: none; padding: 0; border-radius: 0; }

/* Side-docked table of contents on wide screens */
@media (min-width: 62rem) {
  body.toc2 #toc {
    position: fixed;
    top: 0;
    bottom: 0;
    width: 16rem;
    overflow-y: auto;
    border: 0;
    border-radius: 0;
    margin: 0;
    padding: 2rem 1.25rem;
    background: var(--sidebar-bg);
  }
  body.toc2.toc-left #toc { left: 0; border-right: 1px solid var(--rule); }
  body.toc2.toc-right #toc { right: 0; border-left: 1px solid var(--rule); }
  body.toc2.toc-left #header,
  body.toc2.toc-left #content,
  body.toc2.toc-left #footer { margin-left: calc(16rem + 2rem); }
  body.toc2.toc-right #header,
  body.toc2.toc-right #content,
  body.toc2.toc-right #footer { margin-right: calc(16rem + 2rem); }
}
"#;
