//! Re-rendering documents as their sources change.

use std::{
    collections::{
        BTreeSet,
        HashMap,
    },
    path::{
        Path,
        PathBuf,
    },
    sync::mpsc,
    time::Duration,
};

use anyhow::{
    Context,
    Result,
};
use notify_debouncer_full::{
    DebounceEventResult,
    new_debouncer,
    notify::{
        RecursiveMode,
        event::EventKind,
    },
};

use crate::{
    cli::CommonArgs,
    diagnostics::Reporter,
    job::{
        self,
        Job,
    },
    render::Options,
};

/// How long to wait for a burst of file-system events to settle.
///
/// Editors write a file in several steps — a temporary file, a rename, a
/// permissions change — and re-rendering after each one would be wasted work
/// and confusing output.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Render every job once, then keep re-rendering the ones whose sources change,
/// until the process is interrupted.
pub fn run(jobs: &[Job], common: &CommonArgs, options: &Options, reporter: Reporter) -> Result<()> {
    let mut sources = Sources::default();

    for (index, job) in jobs.iter().enumerate() {
        render(job, index, common, options, reporter, &mut sources);
    }

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        // A send failure only means the main loop has gone away.
        let _ = tx.send(result);
    })
    .context("starting the file watcher")?;

    // Watching the containing directories rather than the files themselves
    // survives the rename-into-place that most editors use to save, which
    // replaces the inode a per-file watch is holding.
    let mut watched = BTreeSet::new();
    for directory in sources.directories() {
        watch(&mut debouncer, &directory, &mut watched);
    }

    eprintln!(
        "adocers: watching {} file(s); press Ctrl-C to stop",
        sources.len()
    );

    for result in rx {
        let events = match result {
            Ok(events) => events,

            Err(errors) => {
                for error in errors {
                    eprintln!("adocers: watch error: {error}");
                }

                continue;
            }
        };

        let mut stale = BTreeSet::new();

        for event in events {
            // A read of a watched file is not a reason to rebuild.
            if matches!(event.kind, EventKind::Access(_)) {
                continue;
            }

            for path in &event.paths {
                stale.extend(sources.jobs_for(path));
            }
        }

        for index in stale {
            let Some(job) = jobs.get(index) else {
                continue;
            };

            render(job, index, common, options, reporter, &mut sources);
        }

        // A newly added include may live in a directory nothing was watching.
        for directory in sources.directories() {
            watch(&mut debouncer, &directory, &mut watched);
        }
    }

    Ok(())
}

/// Render one job and record what it read, reporting failure without stopping.
fn render(
    job: &Job,
    index: usize,
    common: &CommonArgs,
    options: &Options,
    reporter: Reporter,
    sources: &mut Sources,
) {
    match job::run(job, common, options, reporter) {
        Ok(outcome) => {
            sources.record(index, &outcome.dependencies);
            eprintln!("adocers: rendered {}", job.input.display());
        }

        // In a watch session a failure is a transient state — a file saved
        // half-written, a path not created yet — so it is reported and the
        // session continues.
        Err(error) => eprintln!("adocers: {}: {error:#}", job.input.display()),
    }
}

/// Begin watching a directory, unless it is already being watched.
fn watch<T: notify_debouncer_full::notify::Watcher, C: notify_debouncer_full::FileIdCache>(
    debouncer: &mut notify_debouncer_full::Debouncer<T, C>,
    directory: &Path,
    watched: &mut BTreeSet<PathBuf>,
) {
    if !watched.insert(directory.to_path_buf()) {
        return;
    }

    if let Err(error) = debouncer.watch(directory, RecursiveMode::NonRecursive) {
        eprintln!("adocers: cannot watch `{}`: {error}", directory.display());
        watched.remove(directory);
    }
}

/// Which jobs depend on which files.
#[derive(Debug, Default)]
struct Sources {
    /// For each source file, the jobs that read it.
    ///
    /// A file can feed several jobs — a shared include, or the same document
    /// rendered twice — so an edit rebuilds all of them.
    by_path: HashMap<PathBuf, BTreeSet<usize>>,
}

impl Sources {
    /// Replace the recorded dependencies of one job.
    fn record(&mut self, index: usize, dependencies: &[PathBuf]) {
        // Drop the previous set first: an include that was removed from the
        // document should stop triggering rebuilds.
        self.by_path.retain(|_, jobs| {
            jobs.remove(&index);
            !jobs.is_empty()
        });

        for path in dependencies {
            self.by_path
                .entry(absolute(path))
                .or_default()
                .insert(index);
        }
    }

    /// The jobs that read `path`.
    fn jobs_for(&self, path: &Path) -> BTreeSet<usize> {
        self.by_path
            .get(&absolute(path))
            .cloned()
            .unwrap_or_default()
    }

    /// The directories holding the watched files.
    fn directories(&self) -> BTreeSet<PathBuf> {
        self.by_path
            .keys()
            .filter_map(|path| path.parent().map(Path::to_path_buf))
            .collect()
    }

    /// How many distinct files are being tracked.
    fn len(&self) -> usize {
        self.by_path.len()
    }
}

/// Make a path absolute so that a watch event and a recorded dependency compare
/// equal even when they were spelled differently.
fn absolute(path: &Path) -> PathBuf {
    // `canonicalize` would be stronger but fails for a file that has just been
    // deleted and not yet recreated, which is exactly what a save looks like.
    match std::env::current_dir() {
        Ok(cwd) => normalize(&cwd.join(path)),
        Err(_) => path.to_path_buf(),
    }
}

/// Collapse `.` and `..` lexically.
fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}

            Component::ParentDir => {
                if out
                    .components()
                    .next_back()
                    .is_some_and(|c| matches!(c, Component::Normal(_)))
                {
                    out.pop();
                } else {
                    out.push(component);
                }
            }

            other => out.push(other),
        }
    }

    out
}
