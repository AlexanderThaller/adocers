# adocers-html

Render an [AsciiDoc](https://asciidoc.org) document to HTML.

[`asciidoc-parser`](https://crates.io/crates/asciidoc-parser) renders *inline*
content — bold, links, cross references, passthroughs — but deliberately stops
there: turning the block tree into a page is the back end's job, and this crate
is that back end. The markup follows Asciidoctor's HTML5 converter — the same
wrapper `div`s and the same class names — so a stylesheet written for
Asciidoctor applies unchanged.

```rust
let mut parser = asciidoc_parser::Parser::default();
let document = parser.parse("= Title\n\nSome prose.\n");

let page = adocers_html::render(&document, &adocers_html::Options {
    stylesheet: Some(adocers_html::default_stylesheet()),
    ..adocers_html::Options::default()
}).html;
```

Set `fragment` to emit the body alone, for pasting into a page that supplies
its own furniture.

## Beyond Asciidoctor

- Source blocks are highlighted here, while the page is built, rather than by
  something the reader has to download and run.
- Mermaid blocks are drawn to inline SVG, which saves the reader a 5.6 MB
  drawing module.
- Equations become `MathML` for the browser to draw.
- Admonitions are marked with an inline-SVG icon rather than a webfont.

Each is a cargo feature — `highlight`, `mermaid`, `math` — and all three are on
by default. Turning one off degrades to the plain markup rather than to nothing:
a mermaid block renders as the listing it was written as.

## Fidelity

`tests/doctest.rs` in the [workspace](https://github.com/AlexanderThaller/adocers)
renders Asciidoctor's own test corpus through this crate and compares the result
against Asciidoctor's output. What does not match yet is listed there by name
rather than guessed at.

## License

MIT OR Apache-2.0, at your option.
