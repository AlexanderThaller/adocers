# Changelog

Notable changes to `adocers` and the three crates it is built from. The
versions are kept in step: all four are released together from one workspace.

## 0.2.0

A page can now be read as well as rendered: a listing offers to copy itself, a
docked outline says where the reader is in the document, and a document written
in a language other than English is typeset as one.

### A page that helps the reader

- **A copy button on every code block.** Selecting a listing by hand dragged
  the callout marks along with it; what lands on the clipboard now is what the
  reader would have selected, minus those marks — they are the page's
  annotations, not the reader's code. The button is added by a small script
  rather than rendered into the markup, so a listing's markup stays
  Asciidoctor's byte for byte and a `--fragment` never carries a control its
  host did not ask for. The script goes in only when the body has a verbatim
  block in it. `--no-copy` leaves it out.
- **A mark on the outline entry for the section being read.** Beside a long
  document that is the difference between a list of links and a sense of place.
  The entry marked is the one for the last heading scrolled past, which is the
  only choice that moves in one direction as the page does. `--no-reading-mark`
  leaves it out; with `--no-copy`, that is a page carrying no script at all.
- **The outline is a column of entries rather than a boxed list**, on the page's
  own ground, with a gutter held open down the side of each entry that only the
  section being read draws in — so the mark has somewhere to go and nothing
  moves when it arrives.
- **An admonition is a box with a coloured edge** rather than an icon column
  with a rule beside it. The edge carries the colour, so the block says what
  kind of aside it is from across the page — without the icon having to be
  large enough to read as one, and without a reader who does not separate those
  five hues losing the boundary of the aside itself.

### Documents not written in English

The PDF back end never set Typst's `text(lang:)`, so Typst assumed English
whatever `:lang:` said. The labels were already translated; the prose around
them was still hyphenated by English patterns and quoted with English glyphs.
AsciiDoc writes one BCP 47 tag where Typst takes the language and the region
apart, so the tag is split — first subtag the language, a later two-letter one
the region — and a tag Typst would not recognise is left out rather than passed
on.

`resources/showcase_de.adoc` is the showcase's question asked the other way
round: not what `adocers` can render, but what moves when the document is not
in English. It demonstrates the quotation marks Typst chooses and the patterns
it hyphenates by, and names the two labels `adocers` does not translate.

### The stylesheet, in the three parts a host needs

`adocers-html` gains `stylesheet_variables()` and `document_stylesheet()`
alongside the existing `default_stylesheet()`, which is unchanged and still
composes all three.

A host rendering a `--fragment` into a page of its own cannot use the whole
stylesheet — the rules for `#header`, `#content` and the outline describe a page
it is not making — but it must not write its own instead. A mermaid diagram this
crate draws carries theme overrides written against the custom properties the
stylesheet declares, looked up from inside the `<svg>`. A host that declared its
own names for those colours leaves every override resolving to nothing and
`fill` falling back to its initial value: a diagram of solid black boxes, with
nothing in the page to say why. So the properties are handed out separately, to
declare at the top level where a diagram can reach them, and the document's own
rules separately, to nest under wherever the document goes. Every selector in
the latter is a class this crate emits or an element inside a document.

### Packaging

`resources/` and `benches/` leave the published package: the corpus is what the
benchmarks and the doctest suite read, not the crate. Both suites already skip
what they cannot find. **7.1 MB to 129 KB.**

Sibling crates are taken with `default-features = false` from the workspace
entry rather than from each member, which newer cargo refuses to inherit. The
resolved feature set is unchanged.

Minimum supported Rust version is still **1.96**.

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
