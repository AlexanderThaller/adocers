# adocers-typst

Typeset an [AsciiDoc](https://asciidoc.org) document as a PDF, with
[Typst](https://typst.app).

Typst is written in Rust, so it compiles in beside everything else: a PDF needs
no LaTeX installation, no headless browser and nothing fetched at run time. The
fonts are the ones Typst embeds, so the result does not depend on what the
machine happens to have.

```rust
let mut parser = asciidoc_parser::Parser::default();
let document = parser.parse("= Title\n\nSome prose.\n");

let pdf = adocers_typst::pdf(&document, std::path::Path::new("."), &adocers_typst::Options {
    icons: true,
    math: true,
    ..adocers_typst::Options::default()
})?;

std::fs::write("out.pdf", pdf.bytes)?;
```

The base directory is where the images a document names are looked for, since
it names them relative to where it was read from.

`pdf` returns a `Pdf` rather than the bytes alone because a library has no
console to complain to. When an equation will not typeset it shows every
equation as its source instead — a page of equations as their source being a
better answer than no page at all — and says so in `fallback`, for a caller
that has somewhere to put a diagnostic.

## What it covers

The shape of an ordinary document: headings, paragraphs, lists, tables with
spans and footers, listings, quotes and verses, admonitions with their icons,
images, page breaks and cross references. A `:toc:` becomes a real outline with
page numbers and links; `:sectnums:` numbers headings from the same place a
page's numbers come from. LaTeX equations are translated by
[`mitex`](https://crates.io/crates/mitex), and `AsciiMath` under the `math`
feature.

A block it has no rendering for becomes its own text rather than a gap.

This is a second back end rather than a setting on
[`adocers-html`](https://crates.io/crates/adocers-html); neither depends on the
other, so a tool that only wants PDFs does not compile an HTML converter to get
one.

## License

MIT OR Apache-2.0, at your option.
