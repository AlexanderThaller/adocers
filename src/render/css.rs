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
  --muted: #6a737d;
  --rule: #dcdfe4;
  --accent: #1565a8;
  --code-bg: #f5f6f8;
  --sidebar-bg: #f2f4f7;
  --admon-bg: #f5f6f8;
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
    --admon-bg: #1e2126;
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

#header, #content, #footer { max-width: 50rem; margin: 0 auto; padding: 0 1.25rem; }
#header { padding-top: 2.5rem; }
#footer { padding-bottom: 3rem; color: var(--muted); font-size: 0.85rem; }

h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 2rem 0 0.75rem; font-weight: 600; }
h1 { font-size: 2rem; margin-top: 0; }
h2 { font-size: 1.6rem; padding-bottom: 0.3rem; border-bottom: 1px solid var(--rule); }
h3 { font-size: 1.3rem; }
h4 { font-size: 1.1rem; }
h5, h6 { font-size: 1rem; }
h1 .subtitle { color: var(--muted); font-weight: 400; }

a { color: var(--accent); text-decoration: none; }
a:hover { text-decoration: underline; }

p { margin: 0 0 1rem; }
hr { border: 0; border-top: 1px solid var(--rule); margin: 2rem 0; }

.details { color: var(--muted); font-size: 0.9rem; margin-bottom: 1.5rem; }
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

/* Quotes */
.quoteblock { margin: 1.25rem 0; }
.quoteblock blockquote { margin: 0; padding: 0.25rem 0 0.25rem 1.25rem; border-left: 4px solid var(--rule); color: var(--fg); }
.verseblock pre.content { background: none; padding: 0 0 0 1.25rem; border-left: 4px solid var(--rule); white-space: pre-wrap; }
.attribution { color: var(--muted); font-size: 0.9rem; margin-top: 0.5rem; }
.attribution cite { font-style: italic; }

/* Admonitions */
.admonitionblock { margin: 1.25rem 0; }
.admonitionblock > table { width: 100%; border-collapse: collapse; background: var(--admon-bg); border-radius: 6px; }
.admonitionblock td.icon { width: 6.5rem; padding: 0.9rem 0 0.9rem 1rem; vertical-align: top; }
.admonitionblock td.icon .title { font-style: normal; font-weight: 600; text-transform: uppercase; font-size: 0.75rem; letter-spacing: 0.04em; margin: 0; }
.admonitionblock td.content { padding: 0.9rem 1rem; vertical-align: top; }
.admonitionblock.note td.icon .title { color: #1565a8; }
.admonitionblock.tip td.icon .title { color: #2f855a; }
.admonitionblock.important td.icon .title { color: #b7791f; }
.admonitionblock.caution td.icon .title { color: #c05621; }
.admonitionblock.warning td.icon .title { color: #c53030; }

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
dt.hdlist1 { font-weight: 600; margin-top: 0.75rem; }
dd { margin-left: 1.5rem; }
.hdlist table { border-collapse: collapse; }
.hdlist td { padding: 0.25rem 1rem 0.25rem 0; vertical-align: top; }
.hdlist td.hdlist1 { font-weight: 600; }

/* Images and media */
.imageblock, .videoblock, .audioblock { margin: 1.5rem 0; }
.imageblock .content, .videoblock .content { text-align: center; }
img, video, audio { max-width: 100%; height: auto; }
.imageblock .title { text-align: center; margin-top: 0.5rem; }

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
td.halign-center, th.halign-center { text-align: center; }
td.halign-right, th.halign-right { text-align: right; }
td.valign-middle, th.valign-middle { vertical-align: middle; }
td.valign-bottom, th.valign-bottom { vertical-align: bottom; }

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
