# Changelog

Notable changes to `adocers` and the three crates it is built from. The
versions are kept in step: all four are released together from one workspace.

## 0.1.0

First release.

`adocers` renders [AsciiDoc](https://asciidoc.org) to HTML or PDF from the
command line, checks documents without rendering them, and serves a directory
over HTTP while you write. Parsing is
[`asciidoc-parser`](https://crates.io/crates/asciidoc-parser)'s; everything
from the block tree onward is here.

Nothing reaches the network, at build time or in the reader's browser. Diagrams
are drawn, equations converted and code highlighted while the page is being
built, so a rendered page is one file that needs nothing else — and a PDF needs
no LaTeX installation, no headless browser and no fonts to install.

### Rendering to HTML

The markup follows Asciidoctor's HTML5 converter — the same wrapper `div`s and
the same class names — so a stylesheet written for Asciidoctor applies
unchanged. How closely is measured rather than asserted: Asciidoctor's own test
corpus is rendered through this back end and compared against Asciidoctor's
output, and what does not match yet is listed by name.

Beyond that converter:

- **Syntax highlighting** while the page is built, by tree-sitter, for Bash, C,
  CSS, Go, HTML, Java, JavaScript, JSON, Python, Rust, TOML, TypeScript, TSX and
  YAML. A block keeps its `language-…` class either way, so a page can still be
  highlighted in the browser instead.
- **Mermaid diagrams** drawn to inline SVG by
  [`merman`](https://crates.io/crates/merman). What this saves is not bytes so
  much as the 5.6 MB drawing module that would otherwise have to reach the
  reader and run before they see anything. The diagram's chrome follows the
  page's colours and the reader's light or dark scheme; its data colours are
  left alone, because the slices of a pie are telling the reader something.
- **Equations** converted to `MathML` for the browser to draw — LaTeX through
  [`math-core`](https://crates.io/crates/math-core), `AsciiMath` through
  [`asciimath-rs`](https://crates.io/crates/asciimath-rs) — inline as well as
  displayed.
- **Admonition icons** as inline SVG rather than a webfont: about a hundred
  bytes each, no network, and they take their colour from `currentColor`.
- **A labelled document header** — `Author:`, `Version:` — rather than a bare
  stack of values that leaves the reader to work out what a lone date means.
- **Landmarks** (`<main>`, `<header>`, `<footer>`) so an assistive reader can
  move between the parts of a page instead of only through them, keeping
  Asciidoctor's ids so its stylesheets still find them.
- Special sections — preface, glossary, bibliography, index, appendices — are
  rendered as such and numbered around rather than through.

`--fragment` emits the body alone, for pasting into a page that supplies its
own furniture. `--css` replaces the embedded stylesheet; `--no-css` omits it.

### Rendering to PDF

Naming an output `.pdf` typesets the document with [Typst](https://typst.app),
which is Rust and compiles in beside everything else.

It covers the shape of an ordinary document: headings, paragraphs, lists,
tables with spans and footers, listings, quotes and verses, admonitions with
their icons, images, page breaks and cross references. A `:toc:` becomes a real
outline with page numbers and links; `:sectnums:` numbers headings from the same
place the page's numbers come from, so a page and a PDF of one document cannot
disagree. Equations are typeset rather than shown — LaTeX via
[`mitex`](https://crates.io/crates/mitex), `AsciiMath` rewritten into Typst's
own notation. Measured against every command `mitex` knows, 854 of 936 typeset.

Three things stop short of the page and say so in the documentation: a table
cell written as AsciiDoc comes out empty, a passthrough is dropped, and a cross
reference to anything but a section keeps its words and loses its link. A block
with no rendering becomes its own text rather than a gap.

### Diagnostics

`adocers check` reports what is wrong with a document without rendering it, and
`adocers check .` walks a directory, which makes it a whole CI step. Warnings
are drawn against the source with [`ariadne`](https://codeberg.org/zesterer/ariadne):

```
[ImageNotFound] Warning: image `gone.png` was not found (looked at `docs/gone.png`)
   ╭─[ docs/page.adoc:9:8 ]
   │
 9 │ image::gone.png[This one does not]
   │        ────┬───
   │            ╰───── image `gone.png` was not found
   │
   │ Help: the target is relative to the document's directory; check the name,
   │       or set `:imagesdir:` if the pictures live somewhere else
───╯
```

The checks are `ImageNotFound`, `SkippingReferenceToMissingAttribute` and
`AttributeSetAfterHeader`. `--deny-warnings` turns any of them into a non-zero
exit; `--message-format json` emits one object per diagnostic for a machine to
read.

### Watching and serving

`--watch` re-renders a document whenever it or one of its includes changes,
debounced so that an editor writing a file in several steps — a temporary file,
a rename, a permissions change — does not cause a render for each.

`adocers serve` serves a directory — or a single document — over HTTP,
rendering each document as it is requested, with live reload, directory
listings, gzip and brotli, and a health check at `/healthz`. `?raw` gives a
document's source and `?format=pdf` typesets it on the spot. Names beginning
with a dot are left out of a listing by default, since a documentation tree
usually sits next to `.git` and friends: `--hidden` lists them, and
`--deny-hidden` answers any of them with a 404, so a stray `.env` stays in the
tree without being readable over HTTP. `--safe-mode` decides how far a document
may reach outside itself.

### The crates

The command line is a thin shell. Everything that turns a parsed document into
output is a library, published separately:

| Crate | |
| --- | --- |
| [`adocers-html`](https://crates.io/crates/adocers-html) | The HTML5 back end. |
| [`adocers-typst`](https://crates.io/crates/adocers-typst) | The PDF back end. |
| [`adocers-render-core`](https://crates.io/crates/adocers-render-core) | What the two agree on: section numbering, header metadata, admonition icons, mermaid diagrams. |

Neither back end depends on the other, so a tool that only wants PDFs does not
compile an HTML converter to get one. Both take a `Document` from
`asciidoc-parser`.

### Building it

Every extra is a cargo feature and all are on by default: `highlight`, `math`,
`mermaid`, `pdf`, `serve`. Turning one off degrades to the plain markup rather
than to nothing — a mermaid block renders as the listing it was written as —
except `pdf`, where a build asked for a PDF says plainly that it cannot write
one rather than writing something wrong.

`flake.nix` builds the same binary with no toolchain to install, and carries an
overlay for NixOS.

Minimum supported Rust version: **1.96**.
