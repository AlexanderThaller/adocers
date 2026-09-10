//! Resolution of `include::` directives against the local file system.

use std::{
    cell::RefCell,
    collections::BTreeSet,
    io::ErrorKind,
    path::{
        Component,
        Path,
        PathBuf,
    },
    rc::Rc,
};

use asciidoc_parser::{
    Parser,
    SafeMode,
    attributes::Attrlist,
    parser::{
        IncludeContent,
        IncludeFileHandler,
        IncludeResolution,
    },
};

/// The set of files a single render read, shared between the include handler
/// and the caller that wants to watch them.
#[derive(Clone, Debug, Default)]
pub struct Dependencies(Rc<RefCell<BTreeSet<PathBuf>>>);

impl Dependencies {
    /// Record `path` as having been read.
    pub fn insert(&self, path: PathBuf) {
        self.0.borrow_mut().insert(path);
    }

    /// The recorded files, in sorted order.
    pub fn snapshot(&self) -> Vec<PathBuf> {
        self.0.borrow().iter().cloned().collect()
    }
}

/// Reads `include::` targets from the file system, relative to the directory of
/// the file that contains the directive.
///
/// The parser hands us the *including* file's name and the target exactly as
/// written; joining them — and enforcing the safe-mode jail — is this handler's
/// job. Everything else about the directive (`lines=`, `tags=`, `leveloffset=`,
/// `indent=`) is applied by the parser to the content we return.
#[derive(Debug)]
pub struct FsIncludeHandler {
    /// The primary document's path, used as the base for top-level directives.
    primary: PathBuf,

    /// Directory that includes may not escape when jailing is in effect.
    jail: Option<PathBuf>,

    /// Files successfully read, recorded for `--watch`.
    deps: Dependencies,
}

impl FsIncludeHandler {
    /// Build a handler for `primary`, jailing includes to its directory when
    /// `safe_mode` is anything stricter than [`SafeMode::Unsafe`].
    pub fn new(primary: &Path, safe_mode: SafeMode, deps: Dependencies) -> Self {
        let base = parent_dir(primary);

        Self {
            primary: primary.to_path_buf(),
            jail: (safe_mode > SafeMode::Unsafe).then(|| normalize(&base)),
            deps,
        }
    }

    /// The directory a directive found in `source` resolves against. `source`
    /// is `None` for the primary document itself.
    fn base_dir(&self, source: Option<&str>) -> PathBuf {
        match source {
            Some(source) => parent_dir(Path::new(source)),
            None => parent_dir(&self.primary),
        }
    }

    /// Whether `path` stays inside the jail, if there is one.
    fn is_permitted(&self, path: &Path) -> bool {
        match &self.jail {
            Some(jail) => path.starts_with(jail),
            None => true,
        }
    }
}

impl IncludeFileHandler for FsIncludeHandler {
    fn resolve_target<'src>(
        &self,
        source: Option<&str>,
        target: &str,
        _attrlist: &Attrlist<'src>,
        _parser: &Parser,
    ) -> IncludeResolution {
        // Fetching a target over the network is out of scope; report it the way
        // a missing file is reported rather than silently emitting nothing.
        if target.contains("://") {
            return IncludeResolution::NotFound;
        }

        let target = Path::new(target);
        let joined = if target.is_absolute() {
            target.to_path_buf()
        } else {
            self.base_dir(source).join(target)
        };

        let path = normalize(&joined);
        if !self.is_permitted(&path) {
            return IncludeResolution::NotFound;
        }

        match std::fs::read(&path) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(content) => {
                    self.deps.insert(path);
                    IncludeContent::new(content).into()
                }
                Err(_) => IncludeResolution::NotDecodable,
            },

            Err(e) if e.kind() == ErrorKind::NotFound => IncludeResolution::NotFound,
            Err(_) => IncludeResolution::NotReadable,
        }
    }
}

/// The directory containing `path`, as a path that can be joined against.
fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Collapse `.` and `..` components lexically.
///
/// This deliberately avoids [`std::fs::canonicalize`]: the target may not exist
/// yet (a watched build renders before every include is written), and resolving
/// symlinks would let a link inside the jail point outside of it.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}

            Component::ParentDir => {
                // Only pop a component we actually descended into; a leading
                // `..` has to survive so it still refers outside the base.
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

    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}
