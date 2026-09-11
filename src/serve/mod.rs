//! `serve`: an HTTP view of a directory of AsciiDoc.
//!
//! Nothing is built ahead of time. A request names a path in the served
//! directory; if it is a document it is parsed and rendered right then, and if
//! it is a directory the server either renders the document that stands for it
//! or offers a listing to click through. Pages carry a small script that
//! reloads them when anything under the directory changes, so editing a file
//! and looking at the browser is the whole loop.

mod listing;
mod mime;
mod reload;
mod url;

use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use anyhow::{
    Context,
    Result,
    bail,
};
use axum::{
    Router,
    body::Body,
    extract::{
        Request,
        State,
    },
    http::{
        Method,
        StatusCode,
        header,
    },
    response::{
        IntoResponse as _,
        Response,
    },
};
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;

use tower_http::compression::CompressionLayer;

use crate::{
    cli::{
        CommonArgs,
        ServeArgs,
    },
    diagnostics::Reporter,
    job::{
        self,
        Format,
    },
    render::{
        Options,
        Page,
        escape_text,
    },
    serve::reload::Reload,
};

/// Path a load balancer's health check is expected at.
///
/// Unprefixed, unlike the server's other endpoints, because this one is aimed
/// at something that does not know or care what is being served and will only
/// be configured with the conventional name. It is therefore reserved: a
/// directory called `healthz` in the served tree is unreachable.
const HEALTH_PATH: &str = "/healthz";

/// Path prefix reserved for the server's own endpoints.
///
/// It is deliberately unlikely to collide with a real file, because anything
/// under it is answered by the server rather than read from the directory.
const INTERNAL_PREFIX: &str = "/__adocers/";

/// Content type of every page this server generates.
const HTML: &str = "text/html; charset=utf-8";

/// Content type of a typeset document.
const PDF: &str = "application/pdf";

/// Serve a directory until the process is interrupted.
pub fn run(args: &ServeArgs) -> Result<()> {
    let root = args
        .root
        .canonicalize()
        .with_context(|| format!("opening `{}`", args.root.display()))?;

    if !root.is_dir() {
        bail!("`{}` is not a directory", args.root.display());
    }

    // The watch guard is held here rather than in `Site`: it lives exactly as
    // long as the server, while `Site` is shared into every request.
    let (reload, watch) = if args.no_reload {
        (None, None)
    } else {
        let (reload, watch) = reload::start(&root)?;
        (Some(reload), Some(watch))
    };

    // A served page reaches the drawing module through this server rather than
    // through the network, so it is pointed at the endpoint below.
    let options = crate::options(&args.common, false)?;

    let site = Arc::new(Site {
        index_files: args.index_files(),
        listing: !args.no_listing,
        common: args.common.clone(),
        options,
        reporter: crate::reporter(&args.common),
        reload,
        root,
    });

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?;

    let result = runtime.block_on(listen(site, &bind_address(&args.bind)));

    drop(watch);
    result
}

/// Bind the listener and serve until interrupted.
async fn listen(site: Arc<Site>, address: &str) -> Result<()> {
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("listening on `{address}`"))?;

    let bound = listener
        .local_addr()
        .map_or_else(|_| address.to_string(), |address| address.to_string());

    eprintln!(
        "adocers: serving {} at http://{bound}/",
        site.root.display()
    );

    if site.reload.is_none() {
        eprintln!("adocers: live reload is off");
    }

    // Compression is not an optimization here so much as a correction: the
    // vendored drawing module is 5.6 MB of JavaScript and compresses to about
    // a fifth of that. A reverse proxy in front of this would usually do it,
    // but a tool that can be run on its own should not need one to be
    // reasonable about what it puts on the wire.
    let app = Router::new()
        .fallback(handle)
        .layer(CompressionLayer::new())
        .with_state(site);

    axum::serve(listener, app)
        .with_graceful_shutdown(interrupted())
        .await
        .context("serving")
}

/// Resolves once the process is asked to stop.
///
/// The first interrupt starts a graceful shutdown: the listener closes and
/// the requests in flight are allowed to finish. One of those may be a reload
/// request waiting for the directory to change, which holds on for up to
/// twenty seconds — long enough that a second press, from someone who wanted
/// the prompt back, should not be made to wait for it.
async fn interrupted() {
    // A failure to install the handler leaves the future pending, which simply
    // means the server runs until it is killed.
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }

    eprintln!("adocers: stopping; press Ctrl+C again to quit at once");

    // The handler stays installed, so waiting again waits for the next press.
    // Exiting from here rather than unwinding is the point: nothing that is
    // still running is worth waiting for.
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("adocers: quitting");
            std::process::exit(130);
        }
    });
}

/// Answer one request.
async fn handle(State(site): State<Arc<Site>>, request: Request) -> Response {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return site
            .status_page(
                StatusCode::METHOD_NOT_ALLOWED,
                "Method not allowed",
                "Only GET and HEAD are served.",
            )
            .send()
            .await;
    }

    let uri = request.uri();
    let encoded_path = uri.path().to_string();
    let query = uri.query().unwrap_or_default().to_string();

    let Some(path) = url::decode(&encoded_path) else {
        return site
            .status_page(
                StatusCode::BAD_REQUEST,
                "Bad request",
                "That request path is not valid.",
            )
            .send()
            .await;
    };

    if path == HEALTH_PATH {
        return site.health().await;
    }

    if let Some(endpoint) = path.strip_prefix(INTERNAL_PREFIX) {
        return site.internal(endpoint, &query).await;
    }

    // `?raw` asks for the document behind a page, and `?format=` for one of the
    // forms it can be rendered into.
    let Some(wanted) = wanted(&query) else {
        return site
            .status_page(
                StatusCode::BAD_REQUEST,
                "Bad request",
                "`format` can be `html` or `pdf`. Use `?raw` for the AsciiDoc itself.",
            )
            .send()
            .await;
    };

    // Resolving a path, reading a directory and parsing a document are all
    // blocking work, and doing them on a runtime thread would stall every other
    // request in flight.
    let answer = {
        let site = Arc::clone(&site);

        tokio::task::spawn_blocking(move || site.answer(&path, &encoded_path, &query, wanted)).await
    };

    match answer {
        Ok(answer) => answer.send().await,

        Err(error) => {
            site.status_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot render",
                &format!("The render task failed: {error}"),
            )
            .send()
            .await
        }
    }
}

/// Everything a request needs to be answered.
#[derive(Debug)]
struct Site {
    /// The served directory, canonicalized so that escapes can be detected.
    root: PathBuf,

    /// Documents tried for a directory, in order; empty when disabled.
    index_files: Vec<String>,

    /// Whether a directory without an index document may be browsed.
    listing: bool,

    /// Parser settings shared with the `render` command.
    common: CommonArgs,

    /// Base page options; the reload script is added per request.
    options: Options,

    /// Where parse diagnostics go.
    reporter: Reporter,

    /// The change counter, absent when live reload is switched off.
    reload: Option<Reload>,
}

/// Which of a document's forms a request asked for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Wanted {
    /// The rendered page, which is what a request without a query gets.
    Page,

    /// The AsciiDoc behind the page, asked for with `?raw`.
    Source,

    /// The document typeset as a PDF, asked for with `?format=pdf`.
    Pdf,
}

/// What to send back, before the body has been opened.
#[derive(Debug)]
enum Answer {
    /// Markup or text the server generated.
    Bytes {
        /// HTTP status code.
        status: StatusCode,

        /// Value of the `Content-Type` header.
        content_type: String,

        /// The body itself.
        body: Vec<u8>,
    },

    /// A permanent redirect, sent so relative links resolve correctly.
    Redirect {
        /// Where the client should go instead.
        location: String,
    },

    /// A typeset document, named so that saving it from the viewer suggests
    /// something better than the AsciiDoc file's name.
    Pdf {
        /// The PDF itself.
        body: Vec<u8>,

        /// What to call it when it is saved.
        name: String,
    },

    /// A file to stream straight from disk.
    File {
        /// The file to open.
        path: PathBuf,

        /// Value of the `Content-Type` header.
        content_type: &'static str,
    },
}

impl Site {
    /// Work out what a request should be answered with.
    ///
    /// `encoded_path` and `query` are the request as it arrived, used to build
    /// a redirect that is still valid after the round trip. `wanted` is which
    /// form of the document was asked for.
    fn answer(&self, path: &str, encoded_path: &str, query: &str, wanted: Wanted) -> Answer {
        let Some(target) = self.resolve(path).or_else(|| self.document_behind(path)) else {
            return self.not_found(path);
        };

        let Ok(metadata) = fs::metadata(&target) else {
            return self.not_found(path);
        };

        if metadata.is_dir() {
            // Without the trailing slash every relative link in the page would
            // resolve against the parent directory instead of this one. The
            // query has to survive the trip, or `?raw` and `?format` would be
            // lost exactly when they were asked for.
            if !path.ends_with('/') {
                let query = if query.is_empty() {
                    String::new()
                } else {
                    format!("?{query}")
                };

                return Answer::Redirect {
                    location: format!("{encoded_path}/{query}"),
                };
            }

            return self.directory(&target, path, wanted);
        }

        self.file(&target, wanted)
    }

    /// Answer a health check.
    ///
    /// A load balancer is asking one question — should traffic still come here
    /// — so the answer is the status code and the body is a courtesy to whoever
    /// opens it by hand.
    ///
    /// The one thing that can go wrong without the process noticing is the
    /// served directory going away: an unmounted volume, or a deployment that
    /// replaced the tree. After that every request would answer 404 while the
    /// process itself looked perfectly well, which is exactly the state a
    /// health check exists to catch. Nothing is parsed or rendered, so the
    /// check stays cheap enough to run every second.
    async fn health(&self) -> Response {
        let readable = tokio::fs::metadata(&self.root)
            .await
            .is_ok_and(|metadata| metadata.is_dir());

        let (status, body) = health_answer(readable);

        build(status, "text/plain; charset=utf-8", None, Body::from(body))
    }

    /// Answer one of the server's own endpoints.
    async fn internal(&self, endpoint: &str, query: &str) -> Response {
        if endpoint != "reload" {
            return self
                .status_page(StatusCode::NOT_FOUND, "Not found", "No such endpoint.")
                .send()
                .await;
        }

        let Some(reload) = &self.reload else {
            return self
                .status_page(
                    StatusCode::NOT_FOUND,
                    "Not found",
                    "Live reload is switched off.",
                )
                .send()
                .await;
        };

        // Without a generation to compare against, report the current one; that
        // is how a page that lost its place gets back in step.
        let generation = match query_value(query, "generation").and_then(|seen| seen.parse().ok()) {
            Some(seen) => reload.wait_for_change(seen).await,
            None => reload.generation(),
        };

        Answer::Bytes {
            status: StatusCode::OK,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: generation.to_string().into_bytes(),
        }
        .send()
        .await
    }

    /// Answer a request that named a directory.
    ///
    /// `wanted` reaches the index document, so a directory's page can be asked
    /// for in the same forms as any other page. A listing has no document
    /// behind it, so it is always a page.
    fn directory(&self, target: &Path, path: &str, wanted: Wanted) -> Answer {
        for name in &self.index_files {
            let candidate = target.join(name);

            if candidate.is_file() {
                return self.file(&candidate, wanted);
            }
        }

        if !self.listing {
            return self.status_page(
                StatusCode::NOT_FOUND,
                "Not found",
                &format!("{path} has no index document, and directory listing is off."),
            );
        }

        match listing::page(
            target,
            path,
            self.options.stylesheet.as_deref(),
            &self.body_suffix(),
        ) {
            Ok(html) => Answer::html(StatusCode::OK, html),

            Err(error) => self.status_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot list directory",
                &format!("{error:#}"),
            ),
        }
    }

    /// Answer a request that named a file.
    ///
    /// Only a document is rendered, so `wanted` matters only for one: every
    /// other file is served as it is whatever was asked for.
    fn file(&self, target: &Path, wanted: Wanted) -> Answer {
        if wanted == Wanted::Source || !mime::is_asciidoc(target) {
            return Answer::File {
                path: target.to_path_buf(),
                content_type: mime::of(target),
            };
        }

        if wanted == Wanted::Pdf && !cfg!(feature = "pdf") {
            return self.status_page(
                StatusCode::NOT_IMPLEMENTED,
                "No typesetter",
                "This build cannot write PDFs: it was built without the `pdf` feature.",
            );
        }

        let format = match wanted {
            Wanted::Pdf => Format::Pdf,
            _ => Format::Html,
        };

        let options = Options {
            body_suffix: self.body_suffix(),
            ..self.options.clone()
        };

        match job::render_as(target, format, &self.common, &options, self.reporter) {
            Ok(outcome) if format == Format::Pdf => Answer::Pdf {
                body: outcome.bytes,
                name: pdf_name(target),
            },

            Ok(outcome) => Answer::html(
                StatusCode::OK,
                String::from_utf8_lossy(&outcome.bytes).into_owned(),
            ),

            // The document exists but could not be read. Reporting it as a page
            // rather than a bare 500 means the reload script is on it, so
            // fixing the file replaces the message with the document.
            Err(error) => self.status_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot render",
                &format!("{error:#}"),
            ),
        }
    }

    /// The document a request for a page that does not exist is really asking
    /// for.
    ///
    /// A cross reference between documents names the page its target becomes:
    /// `xref:two.adoc[]` links to `two.html`, because that is where a build
    /// would have put it. Nothing is built here, so a `.html` request that
    /// names no file is answered with the document it would have been made
    /// from. A `.html` file that does exist is served as itself — this is only
    /// reached once the literal path has been tried.
    fn document_behind(&self, path: &str) -> Option<PathBuf> {
        document_candidates(path)
            .into_iter()
            .find_map(|candidate| self.resolve(&candidate))
            .filter(|target| target.is_file())
    }

    /// Turn a request path into a file inside the served directory.
    ///
    /// Returns `None` for anything that does not resolve to a path under the
    /// root, whether it climbed out with `..` or followed a symlink out.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let mut resolved = self.root.clone();

        for segment in path.split('/') {
            match segment {
                "" | "." => {}
                ".." => return None,
                segment if segment.contains('\\') || segment.contains('\0') => return None,
                segment => resolved.push(segment),
            }
        }

        let canonical = resolved.canonicalize().ok()?;

        canonical.starts_with(&self.root).then_some(canonical)
    }

    /// The reload script for a page rendered now, or nothing when live reload
    /// is switched off.
    fn body_suffix(&self) -> String {
        self.reload
            .as_ref()
            .map_or_else(String::new, |reload| reload::script(reload.generation()))
    }

    /// The answer for a path that names nothing.
    fn not_found(&self, path: &str) -> Answer {
        self.status_page(
            StatusCode::NOT_FOUND,
            "Not found",
            &format!("Nothing is served at {path}"),
        )
    }

    /// A short page explaining a status, styled like the documents around it.
    fn status_page(&self, status: StatusCode, heading: &str, detail: &str) -> Answer {
        let body = format!(
            "<div id=\"header\">\n<h1>{}</h1>\n</div>\n<main id=\"content\">\n<div \
             class=\"paragraph\">\n<p>{}</p>\n</div>\n<div class=\"paragraph\">\n<p><a \
             href=\"/\">Back to the top</a></p>\n</div>\n</main>\n",
            escape_text(heading),
            escape_text(detail)
        );

        let html = crate::render::page(
            &Page {
                lang: "en",
                title: heading,
                description: None,
                body_classes: "article",
                stylesheet: self.options.stylesheet.as_deref(),
                body_suffix: &self.body_suffix(),
            },
            &body,
        );

        Answer::html(status, html)
    }
}

impl Answer {
    /// An HTML page.
    fn html(status: StatusCode, html: String) -> Self {
        Self::Bytes {
            status,
            content_type: HTML.to_string(),
            body: html.into_bytes(),
        }
    }

    /// Open the body, if there is one to open, and build the response.
    async fn send(self) -> Response {
        match self {
            Self::Bytes {
                status,
                content_type,
                body,
            } => build(status, &content_type, None, Body::from(body)),

            Self::Redirect { location } => build(
                StatusCode::MOVED_PERMANENTLY,
                "text/plain; charset=utf-8",
                Some(&location),
                Body::empty(),
            ),

            Self::Pdf { body, name } => {
                let mut response = build(StatusCode::OK, PDF, None, Body::from(body));

                // `inline` so the browser shows it rather than saving it, and a
                // name so that saving it anyway does not suggest `guide.adoc`
                // for a PDF.
                if let Ok(value) =
                    header::HeaderValue::from_str(&format!("inline; filename=\"{name}\""))
                {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_DISPOSITION, value);
                }

                response
            }

            Self::File { path, content_type } => match tokio::fs::File::open(&path).await {
                // Streaming means a large asset never has to be held in memory
                // in its entirety, however big the directory being served is.
                Ok(file) => build(
                    StatusCode::OK,
                    content_type,
                    None,
                    Body::from_stream(ReaderStream::new(file)),
                ),

                Err(error) => build(
                    StatusCode::NOT_FOUND,
                    "text/plain; charset=utf-8",
                    None,
                    Body::from(format!("Cannot open {}: {error}", path.display())),
                ),
            },
        }
    }
}

/// What a health check is told, given whether the served directory is still
/// there.
///
/// Split out from the check itself so the two answers can be tested without a
/// server to ask.
fn health_answer(readable: bool) -> (StatusCode, &'static str) {
    if readable {
        (StatusCode::OK, "ok\n")
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "the served directory is unreadable\n",
        )
    }
}

/// Assemble a response, falling back to a bare status if a header is rejected.
fn build(status: StatusCode, content_type: &str, location: Option<&str>, body: Body) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        // Nothing this server hands out is worth keeping: every file under the
        // served directory is one somebody is editing.
        .header(header::CACHE_CONTROL, "no-store");

    if let Some(location) = location {
        builder = builder.header(header::LOCATION, location);
    }

    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Which form of a document a query asked for, or `None` if it named one this
/// server does not have.
///
/// `?raw` came first and stays as it is, so `format` names only what is
/// rendered: the page, or the PDF. Answering an unknown value with the page
/// would quietly hand back the wrong thing to somebody who asked for
/// `?format=epub` and meant it.
fn wanted(query: &str) -> Option<Wanted> {
    if query_flag(query, "raw") {
        return Some(Wanted::Source);
    }

    // Lowercased for the same reason `-o GUIDE.PDF` writes a PDF: the case a
    // name happens to be typed in says nothing about what was meant.
    let format = query_value(query, "format").map(str::to_lowercase);

    match format.as_deref() {
        // `html` is accepted, and not only the default, so that a link can be
        // built by appending `format=html` without knowing that.
        None | Some("html" | "") => Some(Wanted::Page),

        Some("pdf") => Some(Wanted::Pdf),
        Some(_) => None,
    }
}

/// What a typeset document should be called when it is saved.
///
/// The AsciiDoc file's name with a `.pdf` on it, minus anything a browser would
/// have to read as part of the header rather than as the name.
fn pdf_name(target: &Path) -> String {
    let stem = target
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();

    let stem: String = stem
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '"' | '\\'))
        .collect();

    if stem.is_empty() {
        return "document.pdf".to_string();
    }

    format!("{stem}.pdf")
}

/// The value of one parameter in a query string.
fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}

/// Whether a query string sets a flag.
///
/// A bare `?raw` is the form a person types, and `?raw=1` or `?raw=true` the
/// form a script generates, so both mean the same thing. The negative values
/// are honoured too, because a page that builds links by appending `raw=0`
/// should not turn the flag on.
fn query_flag(query: &str, name: &str) -> bool {
    query.split('&').any(|pair| match pair.split_once('=') {
        None => pair == name,
        Some((key, value)) => key == name && !matches!(value, "0" | "false" | "no" | "off"),
    })
}

/// Interpret the `--bind` value, which may be a full address or just a port.
fn bind_address(bind: &str) -> String {
    if bind.parse::<u16>().is_ok() {
        return format!("127.0.0.1:{bind}");
    }

    bind.to_string()
}

/// The documents a request for a page could have been rendered from, in the
/// order they should be tried.
///
/// A request that does not name a page has none: only `.html` is a page a
/// cross reference would have pointed at.
fn document_candidates(path: &str) -> Vec<String> {
    let Some(stem) = path.strip_suffix(".html") else {
        return Vec::new();
    };

    mime::DOCUMENT_EXTENSIONS
        .iter()
        .map(|extension| format!("{stem}.{extension}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_for_the_document_behind_a_page() {
        assert_eq!(
            document_candidates("/guide.html"),
            ["/guide.adoc", "/guide.asciidoc", "/guide.ad", "/guide.asc"]
        );

        assert_eq!(
            document_candidates("/sub/guide.html").first().unwrap(),
            "/sub/guide.adoc"
        );
    }

    #[test]
    fn looks_for_nothing_behind_anything_but_a_page() {
        for path in ["/guide.adoc", "/logo.png", "/guide", "/", "/a.html.txt"] {
            assert!(
                document_candidates(path).is_empty(),
                "`{path}` should name no document"
            );
        }
    }

    #[test]
    fn reads_which_form_of_a_document_was_asked_for() {
        assert_eq!(wanted(""), Some(Wanted::Page));
        assert_eq!(wanted("format=html"), Some(Wanted::Page));
        assert_eq!(wanted("format="), Some(Wanted::Page));
        assert_eq!(wanted("format=pdf"), Some(Wanted::Pdf));
        assert_eq!(wanted("generation=3&format=pdf"), Some(Wanted::Pdf));
    }

    #[test]
    fn keeps_raw_ahead_of_the_format() {
        // `?raw` is about the file rather than the rendering, so it answers
        // whatever else was asked for.
        assert_eq!(wanted("raw"), Some(Wanted::Source));
        assert_eq!(wanted("format=pdf&raw"), Some(Wanted::Source));
    }

    #[test]
    fn refuses_a_format_it_cannot_produce() {
        // Answering with the page would hand back the wrong thing to somebody
        // who asked for something else and meant it.
        assert_eq!(wanted("format=epub"), None);

        // The case a name is typed in says nothing about what was meant.
        assert_eq!(wanted("format=PDF"), Some(Wanted::Pdf));
    }

    #[test]
    fn names_a_typeset_document_after_its_source() {
        assert_eq!(pdf_name(Path::new("/srv/guide.adoc")), "guide.pdf");
        assert_eq!(pdf_name(Path::new("guide.asciidoc")), "guide.pdf");

        // A quote would end the filename early and leave the rest of the name
        // to be read as header syntax.
        assert_eq!(pdf_name(Path::new(r#"od"d.adoc"#)), "odd.pdf");
    }

    #[test]
    fn reads_a_flag_in_every_form_it_is_written() {
        assert!(query_flag("raw", "raw"));
        assert!(query_flag("raw=", "raw"));
        assert!(query_flag("raw=1", "raw"));
        assert!(query_flag("raw=true", "raw"));
        assert!(query_flag("generation=3&raw", "raw"));
    }

    #[test]
    fn leaves_a_flag_off_when_it_was_not_set() {
        assert!(!query_flag("", "raw"));
        assert!(!query_flag("generation=3", "raw"));
        assert!(!query_flag("raw=0", "raw"));
        assert!(!query_flag("raw=false", "raw"));

        // A parameter that merely starts with the name is a different one.
        assert!(!query_flag("rawish", "raw"));
        assert!(!query_flag("rawish=1", "raw"));
    }

    #[test]
    fn reads_a_query_parameter_value() {
        assert_eq!(query_value("generation=7", "generation"), Some("7"));
        assert_eq!(query_value("raw&generation=7", "generation"), Some("7"));
        assert_eq!(query_value("generation=7", "raw"), None);
    }

    #[test]
    fn answers_a_health_check_from_the_served_directory() {
        let (status, body) = health_answer(true);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "ok\n");

        // A directory that has gone away takes the server out of rotation
        // rather than leaving it to answer 404 to everything.
        let (status, body) = health_answer(false);
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body.contains("unreadable"));
    }

    #[test]
    fn accepts_a_bare_port_as_a_bind_address() {
        assert_eq!(bind_address("8080"), "127.0.0.1:8080");
        assert_eq!(bind_address("0.0.0.0:9000"), "0.0.0.0:9000");
        assert_eq!(bind_address("[::1]:9000"), "[::1]:9000");
    }
}
