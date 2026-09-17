//! Stand-in for [`super::highlight`] in a build without the `highlight`
//! feature.
//!
//! Highlighting is the one thing a source block does not need in order to be
//! correct, so a build without the grammars answers "no language is supported"
//! and every listing renders as plain escaped text.

/// Never highlights: no grammar is compiled in.
pub(crate) fn highlight(_language: &str, _source: &str) -> Option<String> {
    None
}
