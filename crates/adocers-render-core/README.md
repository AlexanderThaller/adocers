# adocers-render-core

What the [AsciiDoc](https://asciidoc.org) back ends behind
[`adocers`](https://crates.io/crates/adocers) have in common.

[`adocers-html`](https://crates.io/crates/adocers-html) renders a document as a
page and [`adocers-typst`](https://crates.io/crates/adocers-typst) typesets it
as a PDF. Neither depends on the other, and this crate is what they agree on:

- `numbering` — which sections are numbered and what each one shows, including
  the special sections that stand outside the sequence and the appendices the
  parser letters.
- `metadata` — which header attributes are facts about the document rather than
  instructions to the renderer, and how each reads.
- `icons` — the inline SVG an admonition is marked with.
- `mermaid` — a diagram drawn to SVG by
  [`merman`](https://crates.io/crates/merman), for a page or for print.
- `escape` — escaping text and attribute values, which both need because an
  icon is markup wherever it ends up.

Everything here is about the *document* rather than about either output format,
which is why both back ends can read it and why a third one could. A page and a
PDF of the same document cannot disagree about any of it.

You probably want one of the back ends rather than this crate directly.

## License

MIT OR Apache-2.0, at your option.
