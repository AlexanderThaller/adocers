//! The HTTP side: pages to read, and an API for the comments on them.
//!
//! A request for a document renders it then and there, through the same
//! pipeline `adocers serve` uses, and hands it back with the review script
//! added. A directory is answered with a listing of the documents in it and
//! how many open comments each has. Anything else in the tree — the pictures a
//! document shows, mostly — is served as it is.
//!
//! Every name beginning with a dot is refused rather than served. This server
//! is meant to be reached from another device, so `.git` and a stray `.env`
//! should not be, and neither should the comment files themselves: they are
//! read through the API, which knows what they are.

use std::{
    net::{
        SocketAddr,
        UdpSocket,
    },
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
};

use adocers::{
    cli::CommonArgs,
    diagnostics::Reporter,
    inputs::{
        DOCUMENT_EXTENSIONS,
        is_asciidoc,
    },
    job,
    render::{
        Options,
        Page,
        escape_text,
    },
};
use anyhow::{
    Context,
    Result,
    bail,
};
use axum::{
    Json,
    Router,
    body::Body,
    extract::{
        Query,
        Request,
        State,
    },
    http::{
        Method,
        StatusCode,
        header,
    },
    response::{
        IntoResponse,
        Response,
    },
    routing::{
        get,
        patch,
    },
};
use percent_encoding::{
    AsciiSet,
    NON_ALPHANUMERIC,
    percent_decode_str,
    utf8_percent_encode,
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tower_http::compression::CompressionLayer;

use crate::{
    Args,
    listing,
    store::{
        Change,
        NewComment,
        Refusal,
        Store,
    },
};

/// Where the comments are read and written.
///
/// Not under a dotted name, since those are refused; a directory of the tree
/// called `__review` is out of reach instead.
const API: &str = "/__review/api/comments";

/// Content type of every page this server generates.
const HTML: &str = "text/html; charset=utf-8";

/// The script each document page carries.
const SCRIPT: &str = include_str!("../assets/review.js");

/// The rules for what the script adds to a page, and for the listing.
const STYLESHEET: &str = include_str!("../assets/review.css");

/// Characters a path segment may keep as they are in a link.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// What every request can reach.
#[derive(Debug)]
pub(crate) struct Site {
    /// The reviewed tree, canonical.
    root: PathBuf,

    /// The comments.
    store: Store,

    /// The command line's rendering options, for the parser.
    common: CommonArgs,

    /// How a document is rendered, before the review script is added.
    options: Options,

    /// How a document's diagnostics are reported.
    reporter: Reporter,

    /// The page's stylesheet with the review rules after it.
    stylesheet: String,
}

/// Serve the tree until the process is interrupted.
pub(crate) fn run(args: &Args) -> Result<()> {
    let root = args
        .root
        .canonicalize()
        .with_context(|| format!("opening `{}`", args.root.display()))?;

    if !root.is_dir() {
        bail!("`{}` is not a directory", args.root.display());
    }

    let options = adocers::options(&args.common, false)?;

    // The review rules read the page's colours, and fall back to their own
    // when there is no stylesheet to read them from.
    let stylesheet = format!(
        "{}\n{STYLESHEET}",
        options.stylesheet.as_deref().unwrap_or_default()
    );

    let site = Arc::new(Site {
        store: Store::new(&root),
        root,
        common: args.common.clone(),
        options,
        reporter: adocers::reporter(&args.common),
        stylesheet,
    });

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the async runtime")?
        .block_on(listen(site, &bind_address(&args.bind)))
}

/// Bind the listener and serve until interrupted.
async fn listen(site: Arc<Site>, address: &str) -> Result<()> {
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("listening on `{address}`"))?;

    let bound = listener.local_addr().context("reading the bound address")?;

    eprintln!(
        "adocers-review: reviewing {} at {}",
        site.root.display(),
        reachable(bound)
    );

    if bound.ip().is_loopback() {
        eprintln!(
            "adocers-review: only this machine can reach that; `--bind 0.0.0.0:{}` opens it to \
             your phone",
            bound.port()
        );
    }

    let app = Router::new()
        .route(API, get(list).post(create))
        .route(&format!("{API}/{{id}}"), patch(update).delete(remove))
        .fallback(page)
        .layer(CompressionLayer::new())
        .with_state(site);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            // A failure to install the handler leaves the server running until
            // it is killed, which is what it would do anyway.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
        .await
        .context("serving")
}

/// The address to open in a browser.
///
/// A server listening on every interface was bound to `0.0.0.0`, which no
/// phone can open; the address this machine would send from is the one worth
/// printing. Connecting a UDP socket sends nothing, it only asks the system
/// which interface it would use.
fn reachable(bound: SocketAddr) -> String {
    if bound.ip().is_unspecified()
        && let Ok(socket) = UdpSocket::bind("0.0.0.0:0")
        && socket.connect("192.0.2.1:9").is_ok()
        && let Ok(local) = socket.local_addr()
    {
        return format!("http://{}:{}/", local.ip(), bound.port());
    }

    format!("http://{bound}/")
}

/// Interpret the `--bind` value, which may be a full address or just a port.
fn bind_address(bind: &str) -> String {
    if bind.parse::<u16>().is_ok() {
        return format!("127.0.0.1:{bind}");
    }

    bind.to_string()
}

/// Which document an API request is about.
#[derive(Debug, Deserialize)]
struct DocumentQuery {
    /// Its path within the tree, as the page was told it.
    document: String,
}

/// An API answer that is not what was asked for.
#[derive(Debug)]
struct ApiError {
    /// The status to answer with.
    status: StatusCode,

    /// Why, in words a reader can be shown.
    message: String,
}

impl ApiError {
    /// A request that named no document under review.
    fn no_document() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: "There is no such document.".to_string(),
        }
    }
}

impl From<Refusal> for ApiError {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::Invalid(message) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: message.to_string(),
            },

            Refusal::Missing => Self {
                status: StatusCode::NOT_FOUND,
                message: "That comment is gone; it may have been deleted elsewhere.".to_string(),
            },

            Refusal::Failed(error) => error.into(),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        // The server's console is where the detail is useful; the page gets
        // enough to know it was not its own fault.
        eprintln!("adocers-review: {error:#}");

        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "The comments could not be saved or read.".to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}

/// Run file system work off the async threads, and answer with what it
/// produced as JSON.
async fn blocking<T, F>(work: F) -> Response
where
    T: serde::Serialize + Send + 'static,
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(error)) => error.into_response(),
        Err(error) => ApiError::from(anyhow::Error::new(error)).into_response(),
    }
}

/// Every comment on a document.
async fn list(State(site): State<Arc<Site>>, Query(query): Query<DocumentQuery>) -> Response {
    blocking(move || {
        let document = site
            .document(&query.document)
            .ok_or_else(ApiError::no_document)?;

        Ok(site.store.list(&document)?)
    })
    .await
}

/// Add a comment to a document.
async fn create(
    State(site): State<Arc<Site>>,
    Query(query): Query<DocumentQuery>,
    Json(new): Json<NewComment>,
) -> Response {
    blocking(move || {
        let document = site
            .document(&query.document)
            .ok_or_else(ApiError::no_document)?;

        Ok(site.store.add(&document, new)?)
    })
    .await
}

/// Resolve, reopen or reword a comment.
async fn update(
    State(site): State<Arc<Site>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(query): Query<DocumentQuery>,
    Json(change): Json<Change>,
) -> Response {
    blocking(move || {
        let document = site
            .document(&query.document)
            .ok_or_else(ApiError::no_document)?;

        Ok(site.store.update(&document, &id, &change)?)
    })
    .await
}

/// Delete a comment.
async fn remove(
    State(site): State<Arc<Site>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Query(query): Query<DocumentQuery>,
) -> Response {
    blocking(move || {
        let document = site
            .document(&query.document)
            .ok_or_else(ApiError::no_document)?;
        site.store.remove(&document, &id)?;

        Ok(serde_json::json!({}))
    })
    .await
}

/// Answer a request for a page or a file.
async fn page(State(site): State<Arc<Site>>, request: Request) -> Response {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return site
            .status_page(
                StatusCode::METHOD_NOT_ALLOWED,
                "Method not allowed",
                "Only GET and HEAD are served here.",
            )
            .send()
            .await;
    }

    let encoded = request.uri().path().to_string();

    let Some(path) = decode(&encoded) else {
        return site
            .status_page(
                StatusCode::BAD_REQUEST,
                "Bad request",
                "That request path is not valid.",
            )
            .send()
            .await;
    };

    // Resolving a path, reading a directory and rendering a document are all
    // blocking work, and doing them here would stall every other request.
    let answer = {
        let site = Arc::clone(&site);
        tokio::task::spawn_blocking(move || site.answer(&path, &encoded)).await
    };

    match answer {
        Ok(answer) => answer.send().await,

        Err(error) => {
            site.status_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                &error.to_string(),
            )
            .send()
            .await
        }
    }
}

/// What a request for a page is answered with.
#[derive(Debug)]
enum Answer {
    /// A page this server made.
    Html(StatusCode, String),

    /// The same path with a trailing slash, for a directory asked for without.
    Redirect(String),

    /// A file from the tree, as it is.
    File(PathBuf, &'static str),
}

impl Site {
    /// Work out what a request for `path` should be answered with.
    ///
    /// `encoded` is the path as it arrived, for a redirect that is still valid
    /// after the round trip.
    fn answer(&self, path: &str, encoded: &str) -> Answer {
        let Some(target) = self.resolve(path).or_else(|| self.document_behind(path)) else {
            return self.not_found(path);
        };

        if target.is_dir() {
            // Without the trailing slash every relative link in the listing
            // would resolve against the parent directory.
            if !path.ends_with('/') {
                return Answer::Redirect(format!("{encoded}/"));
            }

            return match listing::page(&target, path, &self.stylesheet, |document| {
                self.relative(document)
                    .map_or(0, |document| self.store.open_count(&document))
            }) {
                Ok(html) => Answer::Html(StatusCode::OK, html),

                Err(error) => self.status_page(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Cannot list directory",
                    &format!("{error:#}"),
                ),
            };
        }

        if !is_asciidoc(&target) {
            return Answer::File(target.clone(), content_type(&target));
        }

        self.render(&target)
    }

    /// Render a document, with the review script added.
    fn render(&self, target: &Path) -> Answer {
        let Some(document) = self.relative(target) else {
            return self.not_found(&target.display().to_string());
        };

        let options = Options {
            stylesheet: Some(self.stylesheet.clone()),
            body_suffix: script(&document),
            ..self.options.clone()
        };

        match job::render_file(target, &self.common, &options, self.reporter) {
            Ok(outcome) => Answer::Html(
                StatusCode::OK,
                String::from_utf8_lossy(&outcome.bytes).into_owned(),
            ),

            Err(error) => self.status_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot render",
                &format!("{error:#}"),
            ),
        }
    }

    /// The document an API request names, as its path within the tree.
    ///
    /// The name is resolved like any request path and then turned back into
    /// one, so that two spellings of the same document share its comments and
    /// a name that reaches outside the tree reaches nothing.
    fn document(&self, name: &str) -> Option<String> {
        let target = self.resolve(name)?;

        (target.is_file() && is_asciidoc(&target))
            .then(|| self.relative(&target))
            .flatten()
    }

    /// The path of `target` within the tree, separated by `/`.
    fn relative(&self, target: &Path) -> Option<String> {
        let relative = target.strip_prefix(&self.root).ok()?;

        let segments: Vec<_> = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect();

        Some(segments.join("/"))
    }

    /// The document a request for a page that does not exist is really asking
    /// for.
    ///
    /// A cross reference between documents links to the page its target
    /// becomes — `xref:two.adoc[]` to `two.html` — so a `.html` that names no
    /// file is answered with the document it would have been made from.
    fn document_behind(&self, path: &str) -> Option<PathBuf> {
        let stem = path.strip_suffix(".html")?;

        DOCUMENT_EXTENSIONS
            .iter()
            .find_map(|extension| self.resolve(&format!("{stem}.{extension}")))
            .filter(|target| target.is_file())
    }

    /// Turn a request path into a path inside the tree.
    ///
    /// Returns `None` for anything that climbs out with `..`, follows a symlink
    /// out, or names anything hidden along the way.
    fn resolve(&self, path: &str) -> Option<PathBuf> {
        let mut resolved = self.root.clone();

        for segment in path.split('/') {
            match segment {
                "" | "." => {}
                ".." => return None,
                segment if segment.starts_with('.') => return None,
                segment if segment.contains('\\') || segment.contains('\0') => return None,
                segment => resolved.push(segment),
            }
        }

        let canonical = resolved.canonicalize().ok()?;

        canonical.starts_with(&self.root).then_some(canonical)
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
            "<header id=\"header\">\n<h1>{}</h1>\n</header>\n<main id=\"content\">\n<div \
             class=\"paragraph\">\n<p>{}</p>\n</div>\n<div class=\"paragraph\">\n<p><a \
             href=\"/\">Back to the top</a></p>\n</div>\n</main>\n",
            escape_text(heading),
            escape_text(detail)
        );

        let html = adocers::render::page(
            &Page {
                lang: "en",
                title: heading,
                description: None,
                body_classes: "article",
                stylesheet: Some(&self.stylesheet),
                body_suffix: "",
            },
            &body,
        );

        Answer::Html(status, html)
    }
}

impl Answer {
    /// Open the file, if there is one to open, and build the response.
    async fn send(self) -> Response {
        match self {
            Self::Html(status, html) => {
                (status, [(header::CONTENT_TYPE, HTML)], html).into_response()
            }

            Self::Redirect(location) => (
                StatusCode::MOVED_PERMANENTLY,
                [(header::LOCATION, location)],
            )
                .into_response(),

            Self::File(path, content_type) => match tokio::fs::File::open(&path).await {
                Ok(file) => (
                    [(header::CONTENT_TYPE, content_type)],
                    Body::from_stream(ReaderStream::new(file)),
                )
                    .into_response(),

                Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
            },
        }
    }
}

/// The script a document's page carries, told which document it is on.
///
/// The settings go in a JSON block of their own rather than into the script,
/// so the script stays a constant. `</` is escaped so that a document named
/// `</script>.adoc` cannot end the block early.
fn script(document: &str) -> String {
    let settings = serde_json::json!({
        "document": document,
        "api": API,
    })
    .to_string()
    .replace("</", "<\\/");

    format!(
        "<script id=\"review-settings\" \
         type=\"application/json\">{settings}</script>\n<script>\n{SCRIPT}</script>"
    )
}

/// Decode a request path; `None` if it is not valid UTF-8 once decoded.
fn decode(encoded: &str) -> Option<String> {
    percent_decode_str(encoded)
        .decode_utf8()
        .ok()
        .map(std::borrow::Cow::into_owned)
}

/// Encode one path segment for a link.
pub(crate) fn encode_segment(segment: &str) -> String {
    utf8_percent_encode(segment, SEGMENT).to_string()
}

/// The content type of a file served as it is.
///
/// Only what a document is likely to show or link to is named; anything else
/// is offered as bytes to download.
fn content_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();

    match extension.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "txt" | "md" | "csv" => "text/plain; charset=utf-8",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clap::Parser as _;

    use super::*;

    /// A site over a small tree of its own.
    fn site(name: &str) -> Site {
        let root =
            std::env::temp_dir().join(format!("adocers-review-site-{name}-{}", std::process::id()));

        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("guide")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("guide/install.adoc"), "= Install\n\nText.\n").unwrap();
        fs::write(root.join(".git/config"), "secret").unwrap();
        fs::write(root.join("picture.png"), "png").unwrap();

        let root = root.canonicalize().unwrap();
        let args = Args::parse_from(["adocers-review", root.to_str().unwrap()]);

        Site {
            store: Store::new(&root),
            root,
            options: adocers::options(&args.common, false).unwrap(),
            reporter: adocers::reporter(&args.common),
            common: args.common,
            stylesheet: String::new(),
        }
    }

    #[test]
    fn a_document_is_named_by_its_path_in_the_tree() {
        let site = site("names");

        assert_eq!(
            site.document("guide/install.adoc").as_deref(),
            Some("guide/install.adoc")
        );
        assert_eq!(
            site.document("/guide/./install.adoc").as_deref(),
            Some("guide/install.adoc")
        );
        assert_eq!(site.document("picture.png"), None);
        assert_eq!(site.document("guide"), None);
    }

    #[test]
    fn nothing_outside_the_tree_or_hidden_is_reached() {
        let site = site("escape");

        assert_eq!(site.resolve("../etc/passwd"), None);
        assert_eq!(site.resolve("guide/../../etc"), None);
        assert_eq!(site.resolve(".git/config"), None);
        assert_eq!(
            site.resolve(".adocers-review/guide/install.adoc.json"),
            None
        );
        assert!(site.resolve("picture.png").is_some());
    }

    #[test]
    fn a_page_link_finds_the_document_it_was_made_from() {
        let site = site("behind");

        assert_eq!(
            site.document_behind("/guide/install.html"),
            Some(site.root.join("guide/install.adoc"))
        );
        assert_eq!(site.document_behind("/guide/missing.html"), None);
    }

    #[test]
    fn a_document_page_carries_the_review_script() {
        let site = site("render");

        let Answer::Html(StatusCode::OK, html) = site.answer("/guide/install.adoc", "") else {
            panic!("the document did not render");
        };

        assert!(html.contains(r#""document":"guide/install.adoc""#));
        assert!(html.contains("review-settings"));
    }

    #[test]
    fn the_settings_cannot_end_their_own_block() {
        assert!(!script("</script>.adoc").contains("</script>.adoc"));
    }
}
