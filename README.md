# adocers

Render AsciiDoc documents to HTML from the command line.

Parsing is done by [`asciidoc-parser`](https://github.com/asciidoc-rs/asciidoc-parser).
Diagnostics are drawn against the source with
[`ariadne`](https://codeberg.org/zesterer/ariadne). The command line is
[`clap`](https://github.com/clap-rs/clap), and `serve` is built on
[`axum`](https://github.com/tokio-rs/axum).

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
| `--mermaid-url <URL>` | Load mermaid from this URL instead of the built-in copy. Must be a UMD build. |
| `--no-mermaid` | Show mermaid diagrams as the listing blocks they were written as. |
| `--mathjax-url <URL>` | Load MathJax from this URL instead of the built-in copy. Must be a MathJax 3 build with `input/asciimath.js` beside it. |
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

### Mermaid diagrams

A block written as `[mermaid]` or `[source,mermaid]` is rendered as a diagram:

```
[mermaid]
----
flowchart LR
    A --> B
----
```

Both spellings are recognized, because a document that has to render on GitHub
*and* through Antora usually picks between them with an attribute:

```
ifdef::env-github[]
:MERMAID: source, mermaid
endif::[]
ifndef::env-github[]
:MERMAID: mermaid
endif::[]

[{MERMAID}]
----
flowchart LR
    A --> B
----
```

The diagram is drawn in the browser, not at build time. Rendering it here would
mean shelling out to a headless browser the way `asciidoctor-diagram` does, and
that puts a build dependency in the way of what is otherwise a self-contained
binary. It also degrades honestly: a reader with no scripts sees the source of
the diagram rather than a gap.

Mermaid itself is vendored — `vendor/mermaid/`, compiled into the binary — so a
rendered page reaches no further than the machine that rendered it. It is
delivered three ways, depending on where the page is going:

| Output | Where the page gets mermaid |
| --- | --- |
| `serve` | The server's own `/__adocers/vendored/…`, cached indefinitely since the name carries the version. |
| A file | `adocers-assets/mermaid-<version>.min.js`, written beside the page. Documents sharing a directory share one copy. |
| Standard output | The page carries the module itself; there is nowhere to put a file beside it. |

Only a page that actually has a diagram gets any of this. `--mermaid-url` loads
from somewhere else instead — it must be a UMD build, one that defines
`window.mermaid` — and `--no-mermaid` leaves diagrams as the listing blocks they
were written as. A `--fragment` keeps the diagram markup but never the script:
the page it is embedded in owns what it loads.

Diagrams follow the reader's colour scheme, and are redrawn if it changes, since
mermaid bakes the theme into the SVG it produces. A diagram is drawn to the
width of the column and no wider, so it is there to be read rather than to be
scrolled at, and it narrows with the column on a small screen.

### Mathematics

A `[stem]`, `[latexmath]` or `[asciimath]` block is an equation, typeset in the
browser by MathJax on the same terms: vendored into the binary, delivered only
to a page that has one, and never fetched from the network.

MathJax is asked to convert each block by name rather than to scan the page for
delimiters. Its scanner reads the backslash in Asciidoctor's `\$…\$` as an
escape and leaves a stray dollar sign behind, and this renderer already knows
which elements are equations and which notation each is in.

| Output | Where the page gets MathJax |
| --- | --- |
| `serve` | The server's own `/__adocers/vendored/…`, as for mermaid. |
| A file | `adocers-assets/mathjax-<version>-tex-mml-svg.js`, with `input/asciimath.js` beside it. |
| Standard output | The page carries the module itself — and handles LaTeX only, since AsciiMath's processor is a second file MathJax insists on fetching and an inlined page has no URL to fetch it from. |

`--mathjax-url` loads from somewhere else instead, and `--no-math` leaves an
equation as the notation it was written in. The `math` feature is what compiles
the copy in; a build without it behaves as though `--no-math` were always set,
and is 2.2 MB of JavaScript lighter.

### Security

The HTML renderer is not an HTML sanitizer, and AsciiDoc deliberately lets a
document emit raw HTML through passthroughs and attribute substitution. If you
render untrusted AsciiDoc and serve the result to other people, run it through a
sanitizer such as [`ammonia`](https://crates.io/crates/ammonia) first. Safe mode
does not address this — it governs how far a document may reach outside itself,
not what it may put on the page.

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
