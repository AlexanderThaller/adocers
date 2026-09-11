# adocers

Render AsciiDoc documents to HTML — or to PDF — from the command line.

Parsing is done by [`asciidoc-parser`](https://github.com/asciidoc-rs/asciidoc-parser).
Diagnostics are drawn against the source with
[`ariadne`](https://codeberg.org/zesterer/ariadne). The command line is
[`clap`](https://github.com/clap-rs/clap), `serve` is built on
[`axum`](https://github.com/tokio-rs/axum), and PDFs are typeset by
[Typst](https://typst.app).

## Usage

```
adocers [OPTIONS] <FILE>...   # same as `adocers render`
adocers render [OPTIONS] <FILE>...
adocers serve [OPTIONS] [DIR]
```

## Showcase

`resources/showcase.adoc` is a single page through every feature, written the way
Asciidoctor's Writer's Guide is written: each feature explained, the AsciiDoc
that produces it shown, and the result directly underneath. Rendering it is the
quickest way to see whether a change broke anything.

```
adocers resources/showcase.adoc     # writes resources/showcase.html
adocers serve resources             # or read it in a browser
```

## render

Each input is rendered to a sibling `.html` file:

```
adocers doc.adoc            # writes doc.html
adocers -o build/ *.adoc    # writes build/<name>.html for each input
adocers -o - doc.adoc       # writes to standard output
adocers -o doc.pdf doc.adoc # writes a PDF instead
```

### Options

| Option | Effect |
| --- | --- |
| `-o, --output <PATH>` | A file, a directory, or `-` for standard output. Several inputs require a directory. |
| `--fragment` | Emit only the body content — no page, no `#header`/`#footer`, no `#content` wrapper — for embedding in another page. |
| `--css <FILE>` | Embed this stylesheet instead of the built-in one. |
| `--no-css` | Emit the page unstyled. |
| `-a, --attribute <NAME[=VALUE]>` | Set a document attribute. `NAME`, `NAME=VALUE`, `NAME!` and `!NAME` all work, and the document cannot override them. Repeatable. |
| `--no-icons` | Mark admonitions with their label instead of an icon. |
| `--no-highlight` | Leave source blocks unhighlighted. |
| `--no-mermaid` | Show mermaid diagrams as the listing blocks they were written as. |
| `--no-math` | Show equations as the notation they were written in. |
| `--safe-mode <MODE>` | `unsafe` (default), `safe`, `server` or `secure`. Anything above `unsafe` confines `include::` to the document's own directory. |
| `-w, --watch` | Re-render on change; see below. |
| `--deny-warnings` | Exit non-zero if any warning was reported. |
| `-v, --verbose` | Also show low-severity (debug) diagnostics. |
| `-q, --quiet` | Report nothing. |
| `--color <WHEN>` | `auto` (default), `always` or `never`. |

The last six are shared with `serve`.

## serve

```
adocers serve ./docs
adocers serve ./docs --bind 3000
```

Serves a directory over HTTP, rendering each document when it is requested —
nothing is built ahead of time and nothing is left behind. Every page carries a
small script that reloads it when anything under the directory changes, so
editing a file and glancing at the browser is the whole loop.

| Option | Effect |
| --- | --- |
| `-b, --bind <ADDR>` | `HOST:PORT`, or a bare port number for `127.0.0.1`. Default `127.0.0.1:8080`. |
| `--index-file <NAME>` | Document rendered in place of a directory, tried in order. Repeatable, and *replaces* the defaults (`INDEX.adoc`, then `README.adoc`) rather than adding to them. |
| `--no-index-file` | Never stand a document in for a directory; go straight to the listing. |
| `--no-listing` | Do not offer a browsable listing. A directory with no index document is then a 404. |
| `--no-reload` | Do not reload pages when their sources change, and stop watching the directory. |

A request for a directory is answered with the first `--index-file` that exists
in it, and otherwise with a listing of its contents — every entry linked, and
every ancestor linked in the heading. Names beginning with a dot are left out of
the listing; they are still served if asked for by name.

`.adoc`, `.asciidoc`, `.ad` and `.asc` are rendered. Everything else — images,
stylesheets, fonts, PDFs — is served as it is, with a content type guessed from
its extension. `.txt` is served as text rather than rendered, even though the
`include::` directive treats it as AsciiDoc.

A cross reference between documents names the page its target *becomes*, so
`xref:guide.adoc[]` links to `guide.html` — which is right for a rendered
directory and names nothing here, since nothing is built ahead of time. A
request for a `.html` file that does not exist is therefore answered with the
document it would have been made from, so those links work while you are
reading. A `.html` file that does exist is served as itself.

### Reading the source

Adding `raw` to the query serves a document's source as plain text instead of
rendering it:

```
http://localhost:8080/guide.adoc        # the page
http://localhost:8080/guide.adoc?raw    # the AsciiDoc behind it
http://localhost:8080/?raw              # the source of the directory's index document
```

A bare `?raw` and an explicit `?raw=1` or `?raw=true` mean the same thing;
`?raw=0` and `?raw=false` mean the page. The flag reaches whichever document
answers for a directory, so a page and its source are always one query
parameter apart. Files that are not documents are served the same either way,
and a directory listing has no source to show, so it ignores the flag.

A raw response is the file on disk, not the preprocessed document: `include::`
directives appear as written rather than expanded. It carries no reload script,
being plain text.

### Reading it as a PDF

`?format=pdf` typesets the document instead of rendering it as a page, so the
printed form can be checked without writing a file:

```
http://localhost:8080/guide.adoc              # the page
http://localhost:8080/guide.adoc?format=pdf   # the same document, typeset
http://localhost:8080/?format=pdf             # the directory's index document
```

The PDF is built for the request and sent as `application/pdf`, named after the
document so that saving it from the browser's viewer does not suggest
`guide.adoc`. `?format=html` is the default and is accepted so a link can be
built by appending it. A `format` this server cannot produce is a 400 rather
than a page, since answering with the page would quietly hand back the wrong
thing. `?raw` answers first when both are given: it is about the file rather
than the rendering.

Like a raw response, a PDF carries no reload script — a PDF has nowhere to put
one — so it is re-fetched rather than reloaded. A build without the `pdf`
feature answers 501 and says so.

Paths are confined to the served directory: a request cannot climb out with
`..`, and a symlink pointing outside is not followed out. Diagnostics for each
rendered document go to the terminal, exactly as they do for `render`, so a
malformed document is visible without leaving the browser.

### Live reload

Each page holds the value of a counter that goes up whenever the directory
changes. Not every change counts: a file being *read* is not a change to it, and
neither is anything under `target/`, `node_modules/` or a dotted directory such
as `.git/`. Serving a project root rather than a documentation directory is easy
to do — `adocers serve` with no argument does it — and a build or a commit would
otherwise reload the reader's browser every few seconds.

The page asks the server for the current value; the server does not answer
until the two differ, or twenty seconds pass and the request is worth
renewing. So a change reaches the browser as soon as the file system reports it,
with no polling in between, and a page that loses its connection backs off and
recovers on its own.

### Compression

Responses are compressed when the client asks for it, brotli or gzip. That
matters more than it usually would: the vendored drawing module is 5.6 MB of
JavaScript and comes down to about 1.5 MB, and a rendered page of any size is
mostly repetitive markup. A reverse proxy in front of this would normally do
the same thing, and doing it in both places is harmless — whichever sees an
`Accept-Encoding` first handles it.

Measured against the showcase, which carries both the drawing module and the
typesetter, a mobile Lighthouse run went from 7,665 KiB to 2,146 KiB
transferred and from a performance score of 69 to 75. What compression cannot
help is the time the browser spends *running* that JavaScript, which is most of
what is left.

### Health checks

`GET /healthz` answers `200 ok` while the server is fit to take traffic, and
`503` when it is not, which makes it something a load balancer or an orchestrator
can be pointed at. `HEAD` works too, and the answer is never cached.

The one thing it checks is that the served directory is still there and still a
directory. That is the failure a doc server can be in without noticing — an
unmounted volume, or a deployment that moved the tree out from under it — after
which every request answers 404 while the process itself looks perfectly well.
Nothing is parsed or rendered, so the check is cheap enough to run every second.

`/healthz` is reserved: a directory of that name in the served tree is not
reachable. Everything else the server answers for itself lives under
`/__adocers/`, but a health check is aimed at something that neither knows nor
cares what is being served, and will only have been configured with the usual
name.

### Building without it

`serve` is behind the default-on `serve` feature. `cargo build
--no-default-features` leaves out the subcommand and the `axum`, `tokio` and
`tokio-util` dependencies entirely; the rest of the tool is unaffected.

## Diagnostics

Parsing AsciiDoc never fails — every UTF-8 string is a valid document — so
everything the parser has to say is a warning with a source span:

```
[UnterminatedDelimitedBlock] Warning: closing marker for delimited block not found
    ╭─[ doc.adoc:14:1 ]
    │
 14 │ ----
    │ ──┬─
    │   ╰─── closing marker for delimited block not found
────╯
```

Warnings never stop a render; the HTML is written regardless. Use
`--deny-warnings` to fail the run instead.

Spans are shown against the *preprocessed* source — the document after
`include::` expansion — which is what the parser measured, so the underline
always lands on the right bytes. When a warning came from an included file, a
note names the file and line it really came from.

## Watch mode

`--watch` renders every input once, then re-renders a document whenever it, or
any file it included, changes. The dependency set is recorded during each
render, so an `include::` added to a document is watched from the next pass
onward, even if it lives in a directory nothing was watching before.

Directories are watched rather than individual files, which survives the
rename-into-place that most editors use to save. Events are debounced by 200 ms
so a single save produces a single render. A failed render is reported and the
session continues.

## HTML output

The back end follows Asciidoctor's HTML5 converter — the same wrapper `div`s and
class names — so stylesheets written for Asciidoctor apply unchanged. A
standalone page carries a small built-in stylesheet that honours the reader's
light/dark preference.

`asciidoc-parser` renders inline content only; block and document assembly is
this tool's own back end (`src/render/`). `render` and `serve` go through the
same pipeline, so a document looks the same either way.

### Syntax highlighting

A source block is highlighted with [tree-sitter](https://tree-sitter.github.io)
grammars compiled into the binary, so the colours are markup in the page rather
than the work of a script in the reader's browser. A grammar knows what it is
reading, so `fn` is a keyword where Rust means one and ordinary text inside a
string or a comment.

`rust`, `python`, `javascript`, `typescript`, `tsx`, `go`, `c`, `bash`, `java`,
`json`, `toml`, `yaml`, `html` and `css` have grammars, under the names people
actually write — `rs`, `py`, `js`, `ts`, `sh`, `console`, `yml` and the rest.
A language with no grammar is left plain.

Callouts survive highlighting. A `<1>` marker is taken out before the source
reaches the highlighter and put back as the marker the parser would have
rendered, with the parser left as the authority on which `<1>` is a callout and
which is a comparison against a generic — where the two disagree the block is
left to the parser rather than guessed at.

Every block keeps its `language-…` class either way, so a page can still be
highlighted in the browser instead. `--no-highlight` leaves the code plain, and
`--no-default-features` builds without the `highlight` feature, leaving the
grammars out of the binary entirely.

### The document header

The title, author and revision are shown as labelled lines:

```
Author: Alexander Thaller <claude@thallerware.de>
Version: 1.0
Date: 2026-09-10
```

Both ways of writing a revision work — the `v1.0, 2026-09-10` line and the
`:revnumber:`/`:revdate:` attributes — since the line sets those attributes
anyway. The version and the date take a line each, so a document carrying only
one of them says only that one.

A header also carries attributes, and they come in two kinds: facts about the
document, and instructions to the renderer. Only the first kind is shown.
That means anything in Antora's `page-` namespace, with the prefix dropped, and
the names documents conventionally use for their own metadata:

| Written | Shown as |
| --- | --- |
| `:status: implementing` | Status: implementing |
| `:page-tags: design, flux, ci` | Tags: design flux ci |
| `:page-last-reviewed: 2026-01-01` | Last reviewed: 2026-01-01 |
| `:keywords: alpha, beta` | Keywords: alpha beta |
| `:sectnums:`, `:icons: font`, `:toc:` | *nothing — these are settings* |

The named set is `status`, `keywords`, `category`, `edition`, `organization`
and `copyright`. It is an allowlist, so an attribute nobody thought about is
left out rather than shown by accident; prefix one with `page-` to have it
shown. A list of tags or keywords is a set of separate things written with
commas, so each is shown as its own mark.

### Admonition icons

`NOTE`, `TIP`, `IMPORTANT`, `CAUTION` and `WARNING` are marked with an icon, in
the layout Asciidoctor uses: a wide centred icon column, a rule between it and
the text, and no box around the whole thing. The colours are Asciidoctor's own,
lightened under a dark scheme, where its choices were made against a white page.

The icons are inline SVG rather than the Font Awesome glyphs Asciidoctor names
with `:icons: font`. A page this tool produces has no way to pull in that
webfont, so the icon column came out blank; an SVG costs about a hundred bytes,
needs no network, and takes its colour from `currentColor`, so one rule per
admonition themes it in both schemes. Each icon carries its label as an
`aria-label`, so nothing is lost to a screen reader by drawing the mark instead
of writing the word.

`--no-icons` goes back to the uppercase text labels, which is what Asciidoctor
emits when `:icons:` is not set.

### Callout marks

A `<1>` at the end of a line in a listing, and the item in the list beneath the
block that explains it, are both drawn as a number in a circle, so an item and
the line it annotates carry the same mark.

The markup is Asciidoctor's own for `:icons: font` — an empty element carrying
the number as data, followed by the `(1)` it replaces — but the mark is drawn by
the stylesheet rather than by Font Awesome: a circle, and the number from the
element's own data. A page with no stylesheet at all still reads, because the
`(1)` is still there behind the mark.

`--no-icons` is one switch for both kinds of mark: an admonition goes back to
its label, and a callout to the `(1)` the parser rendered.

### Mermaid diagrams

A block written as `[mermaid]` or `[source,mermaid]` is drawn as a diagram.
Both spellings are recognized, because a document that has to render here *and*
on GitHub usually picks between them with an attribute.

The drawing is done here, while the page is rendered, by
[`merman`](https://crates.io/crates/merman) — a mermaid parser and layout
engine in Rust. A page therefore carries its diagrams as `<svg>` and fetches
nothing to show them.

That is worth more than it sounds. The obvious alternative is to send the
reader mermaid itself and let the browser draw, which is what this tool used to
do: 5.6 MB of JavaScript, on every page with a diagram, that has to arrive and
run before anything appears. Measured on the showcase, which has five diagrams,
replacing that with drawing here took the page from 2,146 KiB transferred to
671 KiB, total blocking time from 1,470 ms to 300 ms, the speed index from
3.6 s to 1.4 s, and a mobile Lighthouse score from 48 to 79.

Diagrams take the reader's colour scheme. Mermaid's own palette is for a light
page, so a second stylesheet is put on each diagram that gives the page's
colours to the *chrome* — boxes, lines, labels, backgrounds. The *data* colours
are left alone: the slices of a pie and the branches of a git graph are telling
the reader something, and are not the page's to recolour.

Those colours are the page's own custom properties rather than copies of them,
so a diagram follows a change of scheme while the reader is looking at it, with
nothing running to redraw it. Drawing in the browser needed a script listening
for that and redrawing the diagram each time, because mermaid baked the theme
into the SVG it produced.

`--no-mermaid` renders a diagram as the listing block it was written as, which
is also what happens to a diagram `merman` cannot read — the source of a
diagram being more use than a gap. `merman` is at `0.8.0-alpha.6` and its
layout is close to mermaid's rather than identical: labels wrap at slightly
different widths, because the two measure text differently.

`mermaid` is a default-on feature. Without it nothing is drawn and every
diagram renders as its source.

A diagram in a PDF is drawn the same way, with two differences: it keeps
mermaid's own palette, since a printed page has no colour scheme to follow, and
`merman` is asked for SVG text labels rather than the `<foreignObject>` HTML
ones it normally produces, which need a browser to lay out.

### Mathematics

A `[stem]`, `[latexmath]` or `[asciimath]` block is an equation, converted here
into MathML — which every current browser draws itself, using fonts it already
has. A page carries its equations and loads nothing to show them.

That replaces MathJax, which was 2.2 MB of JavaScript this tool used to vendor
and hand to the reader. An equation in MathML is a few hundred bytes.

Two notations, because AsciiDoc has two. `[latexmath]`, and `[stem]` under
`:stem: latexmath`, goes through
[`math-core`](https://crates.io/crates/math-core), which is thorough — sums,
integrals, matrices and limits all come out right. A plain `:stem:` means
AsciiMath, which goes through
[`asciimath-rs`](https://crates.io/crates/asciimath-rs) and is rougher:
`sqrt(4)` keeps the parentheses a reader would expect it to drop. Prefer LaTeX
if you write much of it.

`--no-math` leaves an equation as the notation it was written in, which is also
what happens to one neither converter can read. `math` is a default-on feature;
without it nothing is converted, in a PDF either — see [Equations in a
PDF](#equations-in-a-pdf) for what happens to one on the way there.

### Accessibility

The page is laid out in landmarks — `<header>`, `<main>` and `<footer>`, each
keeping the id Asciidoctor gives its `<div>`, so a stylesheet written for
Asciidoctor still finds them. Footnotes sit inside the main content rather than
in a fourth region belonging to nothing.

A region that scrolls sideways rather than running off a narrow page — a
listing, a diagram, an equation — carries `tabindex="0"`, because a scrolling
region with nothing focusable in it is one a keyboard cannot reach into. One tab
stop is the price of the rest of the line being readable.

Colours meet WCAG AA against both schemes, links are underlined rather than
distinguished by colour alone, and every admonition icon carries its label as an
accessible name. `axe-core` reports no violations on the showcase across
`wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa` and its best-practice rules.

### Security

The HTML renderer is not an HTML sanitizer, and AsciiDoc deliberately lets a
document emit raw HTML through passthroughs and attribute substitution. If you
render untrusted AsciiDoc and serve the result to other people, run it through a
sanitizer such as [`ammonia`](https://crates.io/crates/ammonia) first. Safe mode
does not address this — it governs how far a document may reach outside itself,
not what it may put on the page.

## PDF output

`-o <name>.pdf` typesets the document instead of rendering it as a page. There
is no `--pdf` flag, because the name already says it and two ways of saying one
thing is one way too many.

```
adocers -o guide.pdf guide.adoc
```

The typesetter is [Typst](https://typst.app), which is written in Rust and
compiles in beside everything else, so a PDF needs no LaTeX installation, no
headless browser and nothing fetched. The fonts are the ones Typst embeds, so
the same document gives the same PDF whatever is installed on the machine. The
showcase — 16 pages, five diagrams, a figure and an outline — takes about
290 ms, against 70 ms for the same document as a page.

This is a second back end rather than a setting on the first (`src/render/typst/`).
It walks the same block tree and covers the shape of an ordinary document:
headings, paragraphs, lists, tables with spans and footers, listings, quotes and
verses, admonitions with their icons, images, page breaks and cross references.
A `:toc:` becomes a real outline, with page numbers and links to the sections it
lists; `:sectnums:` numbers the headings from the same place the page's numbers
come from; a listing is highlighted by the syntaxes Typst carries — the same
code, read by a different highlighter than the page's tree-sitter one; and a
`footnote:[]` becomes a real footnote, at the foot of the page its reference
landed on. Callout marks are the same circled numbers the page draws, as
characters rather than drawings, so they sit in the line of code where the
marker was.

What the header says about the document is written twice: as the labelled lines
under the title, the same ones the page carries, and as the PDF's own
properties — title, author, description, keywords and date — which is what a
viewer's document-properties panel and a search index read.
Diagrams are drawn into it as vector graphics, so they are as sharp printed as
they are on screen; `merman` is asked for SVG text labels rather than the
`<foreignObject>` HTML ones mermaid normally uses, which a browser lays out and
a PDF has no way to.

### Equations in a PDF

Typst has a mathematics mode, but its syntax is neither LaTeX's nor AsciiMath's
— `\sum_{i=1}^{n}` is `sum_(i=1)^n` there — so an equation cannot be handed over
as it stands. `[latexmath]`, and `[stem]` under `:stem: latexmath`, goes through
[`mitex`](https://crates.io/crates/mitex), which translates LaTeX to Typst in
Rust: no package to fetch, no WebAssembly to run. Sums, integrals, roots,
matrices, `cases`, `aligned`, arrays, accents, named operators and `\text` all
come out typeset, inline as well as displayed.

What `mitex` produces calls a few dozen handlers that its own Typst package
supplies through a scope; `src/render/typst/math.typ` defines them, transcribed
from that package. Typst has also renamed a good deal of its mathematics since
`mitex`'s tables were written, and the names it moved are put back. Measured
against every command `mitex` knows — 936 of them — 854 typeset and 82 do not.

The 82 are not an error. A document whose equations will not lay out is
rendered a second time with all of them shown as their source, and says so on
standard error: a page of equations written in LaTeX is a far better answer than
no page at all.

**AsciiMath is not converted**, and is shown as its source. It looks close
enough to Typst to tempt one into passing it through, and it is not: `int_0^1`
is an integral in AsciiMath and the name Typst already uses for the whole-number
type. An equation shown as what the author wrote is honest; one quietly typeset
wrong is not. Prefer LaTeX if you want your maths set.

### What a PDF leaves behind

- **A table cell written as AsciiDoc comes out empty.** A cell holding whole
  blocks is more structure than a page of this kind wants.
- **Passthroughs are dropped.** They are HTML, which a PDF has no use for.
- **A cross reference to anything but a section becomes plain text.** Typst
  refuses to lay out a document that links to a label it cannot find, so a
  reference that would dangle keeps its words and loses its link.

`pdf` is a default-on feature. `cargo build --no-default-features` leaves out
the PDF back end along with the `typst` crates, and asking such a build
for a `.pdf` says so rather than writing something wrong.

## Benchmarks and profiling

`benches/render.rs` times the pipeline in the pieces it is actually made of, so
a change can be judged against the number it was meant to move rather than
against a total that is mostly something else.

```
cargo bench                        # everything
cargo bench -- parse               # one group
cargo bench -- --save-baseline before
cargo bench -- --baseline before   # after a change
```

| Group | What it measures |
| --- | --- |
| `parse` | `asciidoc-parser` alone. The floor under every render, and not this crate's code — worth knowing so a slow document can be blamed correctly. |
| `render` | The back end with highlighting off: block tree in, markup out. The number to watch when changing `src/render/`. |
| `render-highlighted` | The same documents with highlighting on, against warm grammars. The difference from `render` is what tree-sitter costs per byte. |
| `cold-start` | A fresh process per iteration, which is the only way to see what a one-shot render pays. `one-block` against `one-block-no-highlight` is what compiling a grammar costs; `showcase-once` against `showcase-twice` differs by one whole document, so the gap is what a document costs and the rest is setup. |
| `pipeline` | Parse and render together, with the stylesheet, as the command line does it. |

Each group runs over the showcase, the writer's guide if the submodule is
checked out, and three synthetic documents — prose, tables and nested lists —
that isolate the shapes which recurse.

`pipeline` calls `job::render_file`, the function the command line calls,
rather than assembling the steps by hand. That matters: the parser does its
inline substitution lazily and remembers the result, so a benchmark that parses
once and renders in a loop is timing a warm document the command line never
sees.

The same caution applies to reading `parse` and `render` at all. Both are warm
numbers — useful for judging a change to the code they cover, and not what a
single `adocers` run experiences. `cold-start` is where that lives.

### Profiling

Benchmarks say *what* got slower. For *where*, sample the binary:

```
# A flamegraph of one render
cargo flamegraph --bin adocers -- render -q -o - resources/showcase.adoc

# Or sample it in a browser-based profiler
samply record ./target/release/adocers render -q -o - resources/showcase.adoc

# Compare two builds end to end, process startup included
hyperfine './target/release/adocers render -q -o - resources/showcase.adoc' \
          './old/adocers render -q -o - resources/showcase.adoc'
```

A release build carries no debug symbols, which makes a flamegraph unreadable.
Build with them kept:

```
CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release
```
