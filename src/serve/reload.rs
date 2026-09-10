//! Telling an open page that its sources changed.
//!
//! The server keeps a counter that goes up whenever anything under the served
//! directory changes. Every page it renders carries a small script holding the
//! counter's value at render time; that script asks the server for the current
//! value, and the server does not answer until the two differ (or the request
//! has waited long enough to be worth renewing). So a change reaches the
//! browser as soon as the file system reports it, with no polling in between.

use std::{
    path::{
        Component,
        Path,
    },
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use notify_debouncer_full::{
    DebounceEventResult,
    DebouncedEvent,
    Debouncer,
    RecommendedCache,
    new_debouncer,
    notify::{
        RecommendedWatcher,
        RecursiveMode,
        event::EventKind,
    },
};
use tokio::sync::watch;

/// How long to let file-system events settle before counting them as a change.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Directories whose contents cannot be part of a served page, and which change
/// often enough that watching them makes live reload useless.
///
/// Serving a project root rather than a documentation directory is an easy
/// thing to do — `adocers serve` with no argument does it — and a build writing
/// into `target/` would otherwise reload the reader's browser every few
/// seconds. Anything beginning with a dot is skipped for the same reason,
/// `.git` most of all; the directory listing already leaves those out too.
const IGNORED: &[&str] = &["target", "node_modules"];

/// How long a waiting request is held before it is answered unchanged.
///
/// Something has to bound the wait: proxies and browsers drop a connection that
/// produces nothing for long enough, and an answer the client recognizes as "no
/// change" costs one round trip and renews the wait cleanly.
const MAX_WAIT: Duration = Duration::from_secs(20);

/// The path the reload script asks about changes on.
pub const ENDPOINT: &str = "/__adocers/reload";

/// Reads the change counter and waits for it to move.
///
/// Cheap to clone, and shared by every request handler.
#[derive(Clone, Debug)]
pub struct Reload {
    /// Owns the counter; receivers are taken from it as they are needed.
    sender: watch::Sender<u64>,
}

/// Keeps the file-system watch alive.
///
/// The watcher is deliberately not part of [`Reload`]: it is held for the life
/// of the server and dropping it stops the watch, whereas [`Reload`] is copied
/// into every request that needs it.
#[derive(Debug)]
pub struct Watch {
    /// Stops watching when dropped.
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

/// Start watching `root`, returning the shared handle and the watch's guard.
pub fn start(root: &Path) -> Result<(Reload, Watch)> {
    let watched = root.to_path_buf();
    let (sender, _) = watch::channel(0_u64);
    let notified = sender.clone();

    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        let Ok(events) = result else {
            return;
        };

        // Nothing a reader could see has changed if every event was a read, or
        // was about a file no page can depend on.
        if events.iter().all(|event| is_uninteresting(event, &watched)) {
            return;
        }

        // `send_modify` reaches every waiting page and, unlike `send`, does not
        // mind that there may be none waiting right now.
        notified.send_modify(|generation| *generation = generation.wrapping_add(1));
    })
    .context("starting the file watcher")?;

    debouncer
        .watch(root, RecursiveMode::Recursive)
        .with_context(|| format!("watching `{}`", root.display()))?;

    Ok((
        Reload { sender },
        Watch {
            _debouncer: debouncer,
        },
    ))
}

/// Whether an event says nothing about what a reader would see.
fn is_uninteresting(event: &DebouncedEvent, root: &Path) -> bool {
    // Reading a file is not a change to it — and the server reads every file it
    // serves, so taking these for changes would reload the page that caused
    // them, over and over.
    if matches!(event.kind, EventKind::Access(_)) {
        return true;
    }

    // An event with no path says nothing about where it came from, so it is
    // taken at face value rather than assumed to be noise.
    !event.paths.is_empty() && event.paths.iter().all(|path| is_ignored(path, root))
}

/// Whether a path lies somewhere a served page can never depend on.
fn is_ignored(path: &Path, root: &Path) -> bool {
    // Only the part below the root is judged. The root itself may perfectly
    // well sit in a dotted directory, and holding that against every event
    // would stop the whole tree from ever reloading.
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };

    relative.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };

        let name = name.to_string_lossy();

        name.starts_with('.') || IGNORED.contains(&name.as_ref())
    })
}

impl Reload {
    /// The current value of the change counter.
    pub fn generation(&self) -> u64 {
        *self.sender.borrow()
    }

    /// Wait until the counter differs from `seen`, then return its new value.
    ///
    /// Returns the unchanged value if nothing happened within [`MAX_WAIT`].
    pub async fn wait_for_change(&self, seen: u64) -> u64 {
        let mut receiver = self.sender.subscribe();

        // The change may already have happened between the page rendering and
        // its script asking, in which case there is nothing to wait for.
        let current = *receiver.borrow_and_update();
        if current != seen {
            return current;
        }

        // A lapsed timeout is the normal, uneventful case, and a closed channel
        // means the server is going away; both just report what is known.
        let _ = tokio::time::timeout(MAX_WAIT, receiver.changed()).await;

        *receiver.borrow()
    }
}

/// The script a rendered page carries so it can reload itself.
///
/// `generation` is the counter's value at the moment the page was rendered, so
/// a change that lands between rendering and the script's first request is
/// still noticed.
pub fn script(generation: u64) -> String {
    format!(
        r#"<script>
(function () {{
  var seen = {generation};
  var delay = 1000;

  function poll() {{
    fetch("{ENDPOINT}?generation=" + seen, {{ cache: "no-store" }})
      .then(function (response) {{
        if (!response.ok) {{ throw new Error(response.status); }}
        return response.text();
      }})
      .then(function (body) {{
        delay = 1000;
        if (body.trim() !== String(seen)) {{ location.reload(); return; }}
        poll();
      }})
      .catch(function () {{
        // The server is restarting, or the network hiccuped. Back off rather
        // than spinning, and keep trying so the page recovers on its own.
        setTimeout(poll, delay);
        delay = Math.min(delay * 2, 10000);
      }});
  }}

  poll();
}})();
</script>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &str = "/srv/docs";

    #[test]
    fn a_build_or_a_commit_is_not_a_change_a_reader_can_see() {
        for path in [
            "/srv/docs/target/debug/adocers",
            "/srv/docs/target/debug/deps/x.rlib",
            "/srv/docs/.git/index",
            "/srv/docs/node_modules/x/index.js",
            "/srv/docs/docs/.cache/thing",
        ] {
            assert!(
                is_ignored(Path::new(path), Path::new(ROOT)),
                "`{path}` should not reload a page"
            );
        }
    }

    #[test]
    fn an_edit_to_something_served_is() {
        for path in [
            "/srv/docs/guide.adoc",
            "/srv/docs/chapters/one.adoc",
            "/srv/docs/images/diagram.svg",
            "/srv/docs/targets/notes.adoc",
        ] {
            assert!(
                !is_ignored(Path::new(path), Path::new(ROOT)),
                "`{path}` should reload a page"
            );
        }
    }

    #[test]
    fn the_root_may_itself_sit_in_a_dotted_directory() {
        // Judging the whole path rather than the part below the root would
        // ignore everything here and never reload at all.
        let root = Path::new("/home/me/.local/docs");

        assert!(!is_ignored(
            Path::new("/home/me/.local/docs/guide.adoc"),
            root
        ));
        assert!(is_ignored(
            Path::new("/home/me/.local/docs/.git/index"),
            root
        ));
    }

    #[test]
    fn a_path_outside_the_root_is_nothing_to_do_with_us() {
        assert!(is_ignored(Path::new("/etc/passwd"), Path::new(ROOT)));
    }
}
