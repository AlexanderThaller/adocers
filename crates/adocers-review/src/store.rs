//! Where comments are kept: one JSON file per document, beside the tree.
//!
//! The files sit under [`DIRECTORY`] at the root of the reviewed tree,
//! mirroring the documents' own paths — `guide/install.adoc` keeps its
//! comments in `.adocers-review/guide/install.adoc.json`. That keeps the
//! document directories clean, gives one place to commit or to ignore, and
//! leaves each file small and readable on its own: a writer working through a
//! review can open it next to the document and see every comment with the text
//! it was left on.
//!
//! Nothing here knows what a block is. An [`Anchor`] is whatever the page said
//! it was, stored so that the page can find the block again; the server's only
//! business with it is to keep it within bounds.

use std::{
    fs,
    io::ErrorKind,
    path::{
        Path,
        PathBuf,
    },
    sync::{
        Mutex,
        PoisonError,
        atomic::{
            AtomicU32,
            Ordering,
        },
    },
    time::{
        SystemTime,
        UNIX_EPOCH,
    },
};

use anyhow::{
    Context,
    Result,
};
use serde::{
    Deserialize,
    Serialize,
};

/// The directory, at the root of the reviewed tree, that holds the comments.
///
/// Its name begins with a dot, so it is neither listed nor served.
pub(crate) const DIRECTORY: &str = ".adocers-review";

/// The longest comment accepted, in characters.
const MAX_BODY: usize = 10_000;

/// The longest name accepted for whoever wrote a comment, in characters.
const MAX_AUTHOR: usize = 80;

/// The longest quote kept of the block a comment is on, in characters.
const MAX_QUOTE: usize = 300;

/// The longest fingerprint or block kind accepted, in characters.
const MAX_TOKEN: usize = 64;

/// The name a comment is shown under when its writer gave none.
const ANONYMOUS: &str = "Anonymous";

/// Which block a comment was left on, as the page described it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Anchor {
    /// A hash of the block's text, which is what finds it again after a
    /// render: the same words make the same fingerprint.
    pub(crate) fingerprint: String,

    /// Which of the blocks with this fingerprint it was, counting from zero,
    /// for a document that says the same thing twice.
    #[serde(default)]
    pub(crate) occurrence: u32,

    /// The start of the block's text, for a reader of this file — and for the
    /// page to show when the block has since changed and cannot be found.
    #[serde(default)]
    pub(crate) quote: String,

    /// What sort of block it was, such as `paragraph` or `li`.
    #[serde(default)]
    pub(crate) kind: String,
}

/// One comment.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Comment {
    /// Unique within the document.
    pub(crate) id: String,

    /// The block it is on.
    pub(crate) anchor: Anchor,

    /// Who wrote it.
    pub(crate) author: String,

    /// What they wrote, as plain text.
    pub(crate) body: String,

    /// When it was written, in seconds since the Unix epoch.
    pub(crate) created: u64,

    /// Whether it has been dealt with.
    #[serde(default)]
    pub(crate) resolved: bool,
}

/// A comment as the page sends it, before it has an identity.
#[derive(Debug, Deserialize)]
pub(crate) struct NewComment {
    /// The block it is on.
    pub(crate) anchor: Anchor,

    /// Who wrote it; empty for nobody in particular.
    #[serde(default)]
    pub(crate) author: String,

    /// What they wrote.
    pub(crate) body: String,
}

/// A change to a comment that already exists. Absent fields are left alone.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Change {
    /// Mark it dealt with, or open it again.
    pub(crate) resolved: Option<bool>,

    /// Replace what it says.
    pub(crate) body: Option<String>,
}

/// Why a request to change the comments was refused.
#[derive(Debug)]
pub(crate) enum Refusal {
    /// What was sent does not make a comment; the text says why.
    Invalid(&'static str),

    /// No comment has that identity.
    Missing,

    /// The comments could not be read or written.
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for Refusal {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}

/// Everything known about one document's comments, as it is written to disk.
#[derive(Debug, Default, Deserialize, Serialize)]
struct Sidecar {
    /// The document's path within the tree, so the file says what it is about
    /// on its own.
    document: String,

    /// Every comment, in the order they were written.
    comments: Vec<Comment>,
}

/// The comments of every document in one tree.
#[derive(Debug)]
pub(crate) struct Store {
    /// Where the files go: [`DIRECTORY`] under the root.
    directory: PathBuf,

    /// Held across each read-modify-write, so that two comments sent at once
    /// do not each write back a file without the other.
    lock: Mutex<()>,

    /// Distinguishes identities made within the same millisecond.
    counter: AtomicU32,
}

impl Store {
    /// The store for the tree at `root`.
    pub(crate) fn new(root: &Path) -> Self {
        Self {
            directory: root.join(DIRECTORY),
            lock: Mutex::new(()),
            counter: AtomicU32::new(0),
        }
    }

    /// Every comment on `document`, oldest first.
    ///
    /// `document` is the document's path within the tree, separated by `/`.
    pub(crate) fn list(&self, document: &str) -> Result<Vec<Comment>> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);

        Ok(self.read(document)?.comments)
    }

    /// How many comments on `document` are still open.
    ///
    /// A file that cannot be read counts as none: this is for a listing, which
    /// should not fail because one document's comments are damaged.
    pub(crate) fn open_count(&self, document: &str) -> usize {
        self.list(document).map_or(0, |comments| {
            comments.iter().filter(|comment| !comment.resolved).count()
        })
    }

    /// Add a comment to `document`, and hand it back as it was stored.
    pub(crate) fn add(&self, document: &str, new: NewComment) -> Result<Comment, Refusal> {
        let body = new.body.trim();

        if body.is_empty() {
            return Err(Refusal::Invalid("A comment needs some text."));
        }

        if body.chars().count() > MAX_BODY {
            return Err(Refusal::Invalid("That comment is too long."));
        }

        let anchor = new.anchor;

        if anchor.fingerprint.is_empty()
            || anchor.fingerprint.chars().count() > MAX_TOKEN
            || anchor.kind.chars().count() > MAX_TOKEN
        {
            return Err(Refusal::Invalid("That is not a block a comment can go on."));
        }

        let author = truncate(new.author.trim(), MAX_AUTHOR);

        let comment = Comment {
            id: self.identity(),
            anchor: Anchor {
                quote: truncate(anchor.quote.trim(), MAX_QUOTE),
                ..anchor
            },
            author: if author.is_empty() {
                ANONYMOUS.to_string()
            } else {
                author
            },
            body: body.to_string(),
            created: now(),
            resolved: false,
        };

        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut sidecar = self.read(document)?;

        sidecar.comments.push(comment.clone());
        self.write(document, &sidecar)?;

        Ok(comment)
    }

    /// Change the comment `id` on `document`, and hand it back as it now is.
    pub(crate) fn update(
        &self,
        document: &str,
        id: &str,
        change: &Change,
    ) -> Result<Comment, Refusal> {
        let body = match change.body.as_deref().map(str::trim) {
            Some("") => return Err(Refusal::Invalid("A comment needs some text.")),
            Some(body) if body.chars().count() > MAX_BODY => {
                return Err(Refusal::Invalid("That comment is too long."));
            }
            body => body.map(ToString::to_string),
        };

        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut sidecar = self.read(document)?;

        let Some(comment) = sidecar.comments.iter_mut().find(|comment| comment.id == id) else {
            return Err(Refusal::Missing);
        };

        if let Some(resolved) = change.resolved {
            comment.resolved = resolved;
        }

        if let Some(body) = body {
            comment.body = body;
        }

        let comment = comment.clone();
        self.write(document, &sidecar)?;

        Ok(comment)
    }

    /// Delete the comment `id` from `document`.
    pub(crate) fn remove(&self, document: &str, id: &str) -> Result<(), Refusal> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut sidecar = self.read(document)?;
        let before = sidecar.comments.len();

        sidecar.comments.retain(|comment| comment.id != id);

        if sidecar.comments.len() == before {
            return Err(Refusal::Missing);
        }

        self.write(document, &sidecar)?;

        Ok(())
    }

    /// The file that holds `document`'s comments.
    fn sidecar(&self, document: &str) -> PathBuf {
        self.directory.join(format!("{document}.json"))
    }

    /// Read `document`'s comments; a document nobody has commented on has an
    /// empty file's worth.
    ///
    /// Only call this with the lock held.
    fn read(&self, document: &str) -> Result<Sidecar> {
        let path = self.sidecar(document);

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,

            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(Sidecar {
                    document: document.to_string(),
                    comments: Vec::new(),
                });
            }

            Err(error) => {
                return Err(error).with_context(|| format!("reading `{}`", path.display()));
            }
        };

        serde_json::from_str(&text).with_context(|| format!("reading `{}`", path.display()))
    }

    /// Write `document`'s comments back, or remove the file once there are
    /// none, so that a finished review leaves nothing behind.
    ///
    /// The file is written beside itself and then renamed over the old one, so
    /// that a reader never sees half of it and a full disk never costs the
    /// comments that were already there. Only call this with the lock held.
    fn write(&self, document: &str, sidecar: &Sidecar) -> Result<()> {
        let path = self.sidecar(document);

        if sidecar.comments.is_empty() {
            return match fs::remove_file(&path) {
                Err(error) if error.kind() != ErrorKind::NotFound => {
                    Err(error).with_context(|| format!("removing `{}`", path.display()))
                }

                _ => Ok(()),
            };
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating `{}`", parent.display()))?;
        }

        let mut text = serde_json::to_string_pretty(sidecar).context("encoding the comments")?;
        text.push('\n');

        let temporary = path.with_extension("json.tmp");

        fs::write(&temporary, text)
            .with_context(|| format!("writing `{}`", temporary.display()))?;
        fs::rename(&temporary, &path).with_context(|| format!("writing `{}`", path.display()))
    }

    /// A new comment identity: the time, and a counter for comments made
    /// within the same millisecond.
    fn identity(&self) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());

        let count = self.counter.fetch_add(1, Ordering::Relaxed);

        format!("{millis:x}-{count:x}")
    }
}

/// The time now, in seconds since the Unix epoch.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The first `limit` characters of `text`, with an ellipsis when that is not
/// all of it.
fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store over an empty directory of its own.
    fn store(name: &str) -> (Store, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("adocers-review-{name}-{}", std::process::id()));

        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        (Store::new(&root), root)
    }

    fn comment(body: &str) -> NewComment {
        NewComment {
            anchor: Anchor {
                fingerprint: "abc123".to_string(),
                occurrence: 0,
                quote: "The first words of the paragraph".to_string(),
                kind: "paragraph".to_string(),
            },
            author: String::new(),
            body: body.to_string(),
        }
    }

    #[test]
    fn a_comment_is_kept_beside_the_tree_and_read_back() {
        let (store, root) = store("round-trip");

        let added = store
            .add("guide/install.adoc", comment("  Say which version.  "))
            .unwrap();

        assert_eq!(added.body, "Say which version.");
        assert_eq!(added.author, ANONYMOUS);
        assert!(
            root.join(".adocers-review/guide/install.adoc.json")
                .is_file()
        );

        let listed = store.list("guide/install.adoc").unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, added.id);
        assert_eq!(store.open_count("guide/install.adoc"), 1);
    }

    #[test]
    fn resolving_closes_a_comment_without_losing_it() {
        let (store, _root) = store("resolve");
        let added = store.add("a.adoc", comment("Typo.")).unwrap();

        let changed = store
            .update(
                "a.adoc",
                &added.id,
                &Change {
                    resolved: Some(true),
                    body: None,
                },
            )
            .unwrap();

        assert!(changed.resolved);
        assert_eq!(changed.body, "Typo.");
        assert_eq!(store.open_count("a.adoc"), 0);
        assert_eq!(store.list("a.adoc").unwrap().len(), 1);
    }

    #[test]
    fn removing_the_last_comment_removes_the_file() {
        let (store, root) = store("remove");
        let added = store.add("a.adoc", comment("Typo.")).unwrap();

        store.remove("a.adoc", &added.id).unwrap();

        assert!(!root.join(".adocers-review/a.adoc.json").exists());
        assert!(matches!(
            store.remove("a.adoc", &added.id),
            Err(Refusal::Missing)
        ));
    }

    #[test]
    fn an_empty_comment_is_refused() {
        let (store, _root) = store("empty");

        assert!(matches!(
            store.add("a.adoc", comment("   ")),
            Err(Refusal::Invalid(_))
        ));
    }

    #[test]
    fn a_long_quote_is_cut_short() {
        assert_eq!(truncate("abcdef", 3), "abc…");
        assert_eq!(truncate("äbc", 3), "äbc");
    }
}
