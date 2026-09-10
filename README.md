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
changes. The page asks the server for the current value; the server does not
answer until the two differ, or twenty seconds pass and the request is worth
renewing. So a change reaches the browser as soon as the file system reports it,
with no polling in between, and a page that loses its connection backs off and
recovers on its own.

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

### Security

The HTML renderer is not an HTML sanitizer, and AsciiDoc deliberately lets a
document emit raw HTML through passthroughs and attribute substitution. If you
render untrusted AsciiDoc and serve the result to other people, run it through a
sanitizer such as [`ammonia`](https://crates.io/crates/ammonia) first. Safe mode
does not address this — it governs how far a document may reach outside itself,
not what it may put on the page.
