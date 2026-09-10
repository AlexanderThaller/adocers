//! Guessing a content type from a file name.
//!
//! A served directory holds more than documents — the images, stylesheets and
//! fonts they reference have to arrive with a type the browser will act on.
//! This is a small fixed table rather than a dependency: a documentation tree
//! draws from a short list of file types, and anything unrecognized is still
//! served, just as an opaque download.

use std::path::Path;

/// The content type to serve `path` with.
#[must_use]
pub fn of(path: &Path) -> &'static str {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return "application/octet-stream";
    };

    // Extensions are compared in lower case, but only for the ASCII letters
    // that file extensions are actually made of.
    let extension = extension.to_lowercase();

    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "txt" | "adoc" | "asciidoc" | "ad" | "asc" | "md" | "log" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",

        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",

        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",

        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",

        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",

        _ => "application/octet-stream",
    }
}

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
            matches!(
                extension.to_lowercase().as_str(),
                "adoc" | "asciidoc" | "ad" | "asc"
            )
        })
}
