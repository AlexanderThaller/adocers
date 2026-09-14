//! The browsable listing shown for a directory.

use std::{
    fmt::Write as _,
    fs,
    path::Path,
};

use anyhow::{
    Context,
    Result,
};

use crate::{
    render::{
        Page,
        escape_attr,
        escape_text,
    },
    serve::url,
};

/// One entry in a directory listing.
#[derive(Debug)]
struct Entry {
    /// The file name as it appears on disk.
    name: String,

    /// Whether the entry is a directory, which sorts it first and gives its
    /// link a trailing slash.
    is_directory: bool,
}

/// Build the listing page for `directory`, served at `url_path`.
///
/// `url_path` is the request path with its trailing slash, so that links are
/// relative to it and the breadcrumb can be built from its segments. `hidden`
/// says whether entries whose name begins with a dot are listed.
pub fn page(
    directory: &Path,
    url_path: &str,
    hidden: bool,
    stylesheet: Option<&str>,
    body_suffix: &str,
) -> Result<String> {
    let entries =
        read(directory, hidden).with_context(|| format!("listing `{}`", directory.display()))?;

    let title = format!("Index of {url_path}");
    let mut body = String::new();

    body.push_str("<div id=\"header\">\n");
    let _ = writeln!(body, "<h1>{}</h1>", breadcrumb(url_path));
    body.push_str("</div>\n");
    body.push_str("<main id=\"content\">\n");
    body.push_str("<div class=\"ulist\">\n<ul>\n");

    // A link back out, except at the top of the served tree where there is
    // nothing above to go to.
    if url_path != "/" {
        body.push_str("<li>\n<p><a href=\"../\">../</a></p>\n</li>\n");
    }

    for entry in &entries {
        let suffix = if entry.is_directory { "/" } else { "" };

        let _ = writeln!(
            body,
            "<li>\n<p><a href=\"{}{suffix}\">{}{suffix}</a></p>\n</li>",
            escape_attr(&url::encode_segment(&entry.name)),
            escape_text(&entry.name)
        );
    }

    body.push_str("</ul>\n</div>\n");

    if entries.is_empty() {
        body.push_str("<div class=\"paragraph\">\n<p>This directory is empty.</p>\n</div>\n");
    }

    body.push_str("</main>\n");

    Ok(crate::render::page(
        &Page {
            lang: "en",
            title: &title,
            description: None,
            body_classes: "article",
            stylesheet,
            body_suffix,
        },
        &body,
    ))
}

/// Read a directory into a sorted list of entries.
///
/// Unless `hidden` is set, entries whose name begins with a dot are left out: a
/// documentation tree usually sits next to `.git` and friends, and a listing
/// full of them is harder to navigate. Such a file is still served if it is
/// asked for by name, whichever way this goes.
fn read(directory: &Path, hidden: bool) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();

        if !hidden && name.starts_with('.') {
            continue;
        }

        // A broken symlink has no file type to report; skip it rather than
        // offering a link that cannot resolve.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let is_directory = if file_type.is_symlink() {
            entry.path().is_dir()
        } else {
            file_type.is_dir()
        };

        entries.push(Entry { name, is_directory });
    }

    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

/// The heading for a listing, with every ancestor directory linked.
fn breadcrumb(url_path: &str) -> String {
    let mut out = String::from("Index of <a href=\"/\">/</a>");
    let mut href = String::from("/");

    for segment in url_path.split('/').filter(|segment| !segment.is_empty()) {
        href.push_str(&url::encode_segment(segment));
        href.push('/');

        let _ = write!(
            out,
            "<a href=\"{}\">{}</a>/",
            escape_attr(&href),
            escape_text(segment)
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a scratch tree of its own for the calling test, holding a hidden
    /// file, a hidden directory, and a plain one of each.
    fn tree(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir()
            .join(format!("adocers-listing-{}", std::process::id()))
            .join(name);

        // A test that ran before could have left something behind that this one
        // would then list.
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join(".github")).expect("scratch directory is writable");
        fs::create_dir_all(root.join("guide")).expect("scratch directory is writable");
        fs::write(root.join(".hidden.adoc"), "").expect("scratch file is writable");
        fs::write(root.join("README.adoc"), "").expect("scratch file is writable");

        root
    }

    fn names(directory: &Path, hidden: bool) -> Vec<String> {
        let mut names: Vec<String> = read(directory, hidden)
            .expect("the scratch directory is readable")
            .into_iter()
            .map(|entry| entry.name)
            .collect();

        names.sort();
        names
    }

    #[test]
    fn leaves_hidden_entries_out_by_default() {
        let root = tree("default");

        assert_eq!(names(&root, false), ["README.adoc", "guide"]);
    }

    #[test]
    fn lists_hidden_entries_when_asked_to() {
        let root = tree("hidden");

        assert_eq!(
            names(&root, true),
            [".github", ".hidden.adoc", "README.adoc", "guide"]
        );
    }
}
