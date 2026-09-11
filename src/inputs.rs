//! Turning the paths on the command line into the documents to look at.
//!
//! A path that names a file is that file, whatever it is called: naming a
//! document explicitly is the author saying they mean it. A path that names a
//! directory is every AsciiDoc document beneath it, so that `adocers check .`
//! is a whole repository in one command.

use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Context,
    Result,
    bail,
};

/// The extensions that name an AsciiDoc document.
///
/// In `serve` this is also the order in which they are tried when looking for
/// the document behind a requested page.
pub const DOCUMENT_EXTENSIONS: &[&str] = &["adoc", "asciidoc", "ad", "asc"];

/// Whether a file is AsciiDoc, and so should be rendered rather than served.
///
/// `.txt` is AsciiDoc as far as the `include::` directive is concerned, but a
/// text file requested directly is far more likely to be meant as text, so it
/// is left alone.
#[must_use]
pub fn is_asciidoc(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            let extension = extension.to_lowercase();

            DOCUMENT_EXTENSIONS.contains(&extension.as_str())
        })
}

/// Expand the paths given on the command line into documents to read.
///
/// Files are taken as named. Each directory is walked to the bottom and
/// contributes every AsciiDoc document under it, in path order so that a run
/// reports the same thing twice. Nothing is skipped on the way down: a
/// document in a dot-directory is a document, and a CI step that silently
/// passed over `.github/` would be worse than useless.
///
/// A directory holding no documents is an error, because the only reason to
/// point this at a directory is to check what is in it, and a run that checked
/// nothing at all should say so rather than pass.
pub fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut documents = Vec::with_capacity(paths.len());

    for path in paths {
        if !path.is_dir() {
            documents.push(path.clone());
            continue;
        }

        let before = documents.len();
        collect(path, &mut documents)?;

        if documents.len() == before {
            bail!(
                "`{}` holds no AsciiDoc documents (looked for {})",
                path.display(),
                DOCUMENT_EXTENSIONS
                    .iter()
                    .map(|extension| format!("`.{extension}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    Ok(documents)
}

/// Append every AsciiDoc document under `directory`, deepest last within each
/// directory and sorted by name throughout.
fn collect(directory: &Path, documents: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(directory)
        .with_context(|| format!("reading `{}`", directory.display()))?
        .map(|entry| {
            entry
                .with_context(|| format!("reading `{}`", directory.display()))
                .map(|entry| entry.path())
        })
        .collect::<Result<_>>()?;

    // Sorted, so that the order documents are reported in is the order they
    // appear in, and not whatever order the file system happened to hand back.
    entries.sort();

    for entry in entries {
        // `symlink_metadata` rather than `metadata`: a symlink to a directory
        // is not descended into, since following one is how a walk ends up in
        // a loop, or in a tree the caller never asked about.
        let kind = fs::symlink_metadata(&entry)
            .with_context(|| format!("reading `{}`", entry.display()))?
            .file_type();

        if kind.is_dir() {
            collect(&entry, documents)?;
            continue;
        }

        // `is_file` follows a symlink and is false when there is nothing at
        // the end of it, so a symlink to a document is that document and a
        // broken one is quietly nothing.
        if is_asciidoc(&entry) && entry.is_file() {
            documents.push(entry);
        }
    }

    Ok(())
}
