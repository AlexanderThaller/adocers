//! The page shown for a directory: its documents, and what is open on each.
//!
//! Unlike `adocers serve`'s listing this one leaves out everything that is
//! not a document or a directory. A reviewer is looking for something to read,
//! and on a phone every row they do not want is a row in the way of the one
//! they do.

use std::{
    fmt::Write as _,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use adocers::{
    inputs::is_asciidoc,
    render::{
        Page,
        escape_attr,
        escape_text,
    },
};
use anyhow::{
    Context,
    Result,
};

use crate::server::encode_segment;

/// One row of the listing.
#[derive(Debug)]
struct Entry {
    /// The name as it appears on disk.
    name: String,

    /// Where it is.
    path: PathBuf,

    /// Whether it is a directory, which sorts it first.
    is_directory: bool,
}

/// Build the listing for `directory`, served at `url_path`.
///
/// `open` counts the open comments on a document.
pub(crate) fn page(
    directory: &Path,
    url_path: &str,
    stylesheet: &str,
    open: impl Fn(&Path) -> usize,
) -> Result<String> {
    let entries = read(directory).with_context(|| format!("listing `{}`", directory.display()))?;

    let mut body = String::new();

    let _ = writeln!(
        body,
        "<header id=\"header\">\n<h1>{}</h1>\n</header>",
        escape_text(url_path)
    );

    body.push_str("<main id=\"content\">\n<ul class=\"review-listing\">\n");

    if url_path != "/" {
        body.push_str("<li><a href=\"../\"><span class=\"review-name\">..</span></a></li>\n");
    }

    for entry in &entries {
        let suffix = if entry.is_directory { "/" } else { "" };

        let count = if entry.is_directory {
            0
        } else {
            open(&entry.path)
        };

        let badge = match count {
            0 => String::new(),
            1 => " <span class=\"review-count\">1 open</span>".to_string(),
            count => format!(" <span class=\"review-count\">{count} open</span>"),
        };

        let _ = writeln!(
            body,
            "<li><a href=\"{}{suffix}\"><span \
             class=\"review-name\">{}{suffix}</span>{badge}</a></li>",
            escape_attr(&encode_segment(&entry.name)),
            escape_text(&entry.name),
        );
    }

    body.push_str("</ul>\n");

    if entries.is_empty() {
        body.push_str("<div class=\"paragraph\">\n<p>There are no documents here.</p>\n</div>\n");
    }

    body.push_str("</main>\n");

    Ok(adocers::render::page(
        &Page {
            lang: "en",
            title: url_path,
            description: None,
            body_classes: "article",
            stylesheet: Some(stylesheet),
            body_suffix: "",
        },
        &body,
    ))
}

/// The directories and documents in `directory`, directories first and each
/// group by name.
///
/// Hidden names are left out: the server refuses them anyway.
fn read(directory: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();

        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();

        // `is_dir` follows a symlink, and a broken one is neither.
        let is_directory = path.is_dir();

        let is_document = path.is_file() && is_asciidoc(&path);

        if !is_directory && !is_document {
            continue;
        }

        entries.push(Entry {
            name,
            path,
            is_directory,
        });
    }

    entries.sort_by(|a, b| {
        b.is_directory
            .cmp(&a.is_directory)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_documents_and_directories_only() {
        let root =
            std::env::temp_dir().join(format!("adocers-review-listing-{}", std::process::id()));

        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("guide")).unwrap();
        fs::create_dir_all(root.join(".adocers-review")).unwrap();
        fs::write(root.join("b.adoc"), "").unwrap();
        fs::write(root.join("A.adoc"), "").unwrap();
        fs::write(root.join("picture.png"), "").unwrap();

        let names: Vec<_> = read(&root)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();

        assert_eq!(names, ["guide", "A.adoc", "b.adoc"]);

        let html = page(&root, "/", "", |path| {
            usize::from(path.ends_with("b.adoc")) * 2
        })
        .unwrap();

        assert!(html.contains("2 open"));
    }
}
