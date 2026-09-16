//! What the AsciiDoc back ends have in common.
//!
//! [`adocers-html`](https://crates.io/crates/adocers-html) renders a document
//! as a page and [`adocers-typst`](https://crates.io/crates/adocers-typst)
//! typesets it as a PDF. Neither depends on the other, and this crate is what
//! they agree on: the section numbers a document shows, the attributes it puts
//! under its title, the icon an admonition is marked with, and the SVG a
//! mermaid block is drawn as.
//!
//! Everything here is about the *document*, not about either output format —
//! which is why both back ends can read it and why a third one could.

pub mod escape;
pub mod icons;
#[cfg(feature = "mermaid")]
pub mod mermaid;
pub mod metadata;
pub mod numbering;
