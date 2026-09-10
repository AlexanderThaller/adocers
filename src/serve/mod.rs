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

use crate::{
    cli::{
        CommonArgs,
        ServeArgs,
    },
    diagnostics::Reporter,
    job,
    render::{
        Options,
        Page,
        diagram,
        escape_text,
    },
    serve::reload::Reload,
};

/// Path prefix reserved for the server's own endpoints.
///
/// It is deliberately unlikely to collide with a real file, because anything
/// under it is answered by the server rather than read from the directory.
const INTERNAL_PREFIX: &str = "/__adocers/";

/// Content type of every page this server generates.
const HTML: &str = "text/html; charset=utf-8";

/// Endpoint the vendored drawing module is served from.
///
/// The name carries mermaid's version, so the response can be cached for as
/// long as the browser likes: this URL will never hold anything else.
const MERMAID_ENDPOINT: &str = "mermaid";

/// How long the vendored module may be cached for.
///
/// A year, which is the conventional way of saying "forever" — and `immutable`
/// so a reload does not even ask again.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

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
    let mut options = crate::options(&args.common, false)?;

    if crate::uses_vendored_mermaid(&args.common) {
        options.mermaid = Some(diagram::Source::Url(format!(
            "{INTERNAL_PREFIX}{MERMAID_ENDPOINT}/{}",
            diagram::BUNDLE_FILE
        )));
    }

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

    let app = Router::new().fallback(handle).with_state(site);

    axum::serve(listener, app)
        .with_graceful_shutdown(interrupted())
        .await
        .context("serving")
}

/// Resolves once the process is asked to stop.
async fn interrupted() {
    // A failure to install the handler leaves the future pending, which simply
    // means the server runs until it is killed.
    if tokio::signal::ctrl_c().await.is_ok() {
        eprintln!("adocers: stopping");
    } else {
        std::future::pending::<()>().await;
    }
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

    if let Some(endpoint) = path.strip_prefix(INTERNAL_PREFIX) {
        return site.internal(endpoint, &query).await;
    }

    // `?raw` asks for the document behind a page rather than the page.
    let raw = query_flag(&query, "raw");

    // Resolving a path, reading a directory and parsing a document are all
    // blocking work, and doing them on a runtime thread would stall every other
    // request in flight.
    let answer = {
        let site = Arc::clone(&site);

        tokio::task::spawn_blocking(move || site.answer(&path, &encoded_path, &query, raw)).await
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
    /// a redirect that is still valid after the round trip. `raw` asks for
    /// a document's source instead of its rendering.
    fn answer(&self, path: &str, encoded_path: &str, query: &str, raw: bool) -> Answer {
        let Some(target) = self.resolve(path).or_else(|| self.document_behind(path)) else {
            return self.not_found(path);
        };

        let Ok(metadata) = fs::metadata(&target) else {
            return self.not_found(path);
        };

        if metadata.is_dir() {
            // Without the trailing slash every relative link in the page would
            // resolve against the parent directory instead of this one. The
            // query has to survive the trip, or `?raw` would be lost exactly
            // when it was asked for.
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

            return self.directory(&target, path, raw);
        }

        self.file(&target, raw)
    }

    /// Answer one of the server's own endpoints.
    async fn internal(&self, endpoint: &str, query: &str) -> Response {
        if let Some(file) = endpoint
            .strip_prefix(MERMAID_ENDPOINT)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            return Self::vendored(file);
        }

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

    /// Serve a module vendored into this binary.
    ///
    /// The file name carries the version it holds, so a request for any other
    /// name is a stale link rather than something to guess at.
    fn vendored(file: &str) -> Response {
        if file != diagram::BUNDLE_FILE {
            return build(
                StatusCode::NOT_FOUND,
                "text/plain; charset=utf-8",
                None,
                Caching::Never,
                Body::from("No such asset."),
            );
        }

        build(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            None,
            Caching::Forever,
            Body::from(diagram::BUNDLE),
        )
    }

    /// Answer a request that named a directory.
    ///
    /// `raw` reaches the index document, so the source behind a directory's
    /// page can be read the same way as the source behind any other page. A
    /// listing has no document behind it, so it ignores the flag.
    fn directory(&self, target: &Path, path: &str, raw: bool) -> Answer {
        for name in &self.index_files {
            let candidate = target.join(name);

            if candidate.is_file() {
                return self.file(&candidate, raw);
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
    /// Only a document is rendered, so `raw` matters only for one: every other
    /// file is served as it is either way.
    fn file(&self, target: &Path, raw: bool) -> Answer {
        if raw || !mime::is_asciidoc(target) {
            return Answer::File {
                path: target.to_path_buf(),
                content_type: mime::of(target),
            };
        }

        let options = Options {
            body_suffix: self.body_suffix(),
            ..self.options.clone()
        };

        match job::render_file(target, &self.common, &options, self.reporter) {
            Ok(outcome) => Answer::html(StatusCode::OK, outcome.html),

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
            "<div id=\"header\">\n<h1>{}</h1>\n</div>\n<div id=\"content\">\n<div \
             class=\"paragraph\">\n<p>{}</p>\n</div>\n<div class=\"paragraph\">\n<p><a \
             href=\"/\">Back to the top</a></p>\n</div>\n</div>\n",
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
            } => build(
                status,
                &content_type,
                None,
                Caching::Never,
                Body::from(body),
            ),

            Self::Redirect { location } => build(
                StatusCode::MOVED_PERMANENTLY,
                "text/plain; charset=utf-8",
                Some(&location),
                Caching::Never,
                Body::empty(),
            ),

            Self::File { path, content_type } => match tokio::fs::File::open(&path).await {
                // Streaming means a large asset never has to be held in memory
                // in its entirety, however big the directory being served is.
                Ok(file) => build(
                    StatusCode::OK,
                    content_type,
                    None,
                    Caching::Never,
                    Body::from_stream(ReaderStream::new(file)),
                ),

                Err(error) => build(
                    StatusCode::NOT_FOUND,
                    "text/plain; charset=utf-8",
                    None,
                    Caching::Never,
                    Body::from(format!("Cannot open {}: {error}", path.display())),
                ),
            },
        }
    }
}

/// How long a response may be reused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Caching {
    /// Never. Everything read out of the served directory is being edited, so
    /// a cached copy is always the wrong answer.
    Never,

    /// Indefinitely. Only the vendored module qualifies: its name carries the
    /// version it holds, so that URL can never mean anything else.
    Forever,
}

/// Assemble a response, falling back to a bare status if a header is rejected.
fn build(
    status: StatusCode,
    content_type: &str,
    location: Option<&str>,
    caching: Caching,
    body: Body,
) -> Response {
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            header::CACHE_CONTROL,
            match caching {
                Caching::Never => "no-store",
                Caching::Forever => IMMUTABLE,
            },
        );

    if let Some(location) = location {
        builder = builder.header(header::LOCATION, location);
    }

    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
    fn accepts_a_bare_port_as_a_bind_address() {
        assert_eq!(bind_address("8080"), "127.0.0.1:8080");
        assert_eq!(bind_address("0.0.0.0:9000"), "0.0.0.0:9000");
        assert_eq!(bind_address("[::1]:9000"), "[::1]:9000");
    }
}
