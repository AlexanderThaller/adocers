//! Runs `check` over the example documents in `tests/lints` and compares
//! what it reports with what each document says to expect.
//!
//! Each document opens with a line `// expect: <Code>@<line> …`, or
//! `// expect: nothing`, and then a comment saying what it demonstrates. The
//! documents double as the examples to reach for when trying a check by hand:
//!
//! ```
//! adocers check tests/lints/*.adoc
//! ```

use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

/// Every `.adoc` under `tests/lints`, in name order.
fn examples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/lints");

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/lints exists")
        .map(|entry| entry.expect("directory entry is readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "adoc")
        })
        .collect();

    paths.sort();
    paths
}

/// What a document's first line says `check` should report, as `Code@line`
/// entries, sorted.
fn expected(source: &str) -> Vec<String> {
    let first = source.lines().next().unwrap_or_default();

    let list = first
        .strip_prefix("// expect:")
        .unwrap_or_else(|| panic!("the first line must be `// expect: …`, not `{first}`"))
        .trim();

    let mut items: Vec<String> = match list {
        "nothing" => Vec::new(),
        list => list.split_whitespace().map(str::to_string).collect(),
    };

    items.sort();
    items
}

/// What `check` reported, as `Code@line` entries, sorted, and whether it
/// passed.
fn reported(path: &Path) -> (Vec<String>, bool) {
    let output = Command::new(env!("CARGO_BIN_EXE_adocers"))
        .args(["check", "--color", "never"])
        .arg(path)
        .output()
        .expect("the binary runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut items = Vec::new();
    let mut code: Option<String> = None;

    // A report starts with `[Code] Warning: …` and its next line names the
    // position as `╭─[ path:line:column ]`.
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix('[')
            && let Some((name, _)) = rest.split_once(']')
        {
            code = Some(name.to_string());
            continue;
        }

        if let Some(name) = code.take()
            && let Some((_, position)) = line.split_once("╭─[ ")
        {
            let position = position.trim_end_matches(" ]");
            let mut parts = position.rsplit(':');
            let _column = parts.next();
            let line_number = parts.next().expect("position has a line number");

            items.push(format!("{name}@{line_number}"));
        }
    }

    items.sort();
    (items, output.status.success())
}

#[test]
fn every_example_reports_what_it_says_it_will() {
    let mut failures = Vec::new();

    for path in examples() {
        let source = fs::read_to_string(&path).expect("example is readable");
        let expected = expected(&source);
        let (reported, passed) = reported(&path);
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if reported != expected {
            failures.push(format!(
                "{name}: expected {expected:?}, `check` reported {reported:?}"
            ));
        }

        if passed != expected.is_empty() {
            failures.push(format!(
                "{name}: `check` {} but {} expected",
                if passed { "passed" } else { "failed" },
                if expected.is_empty() {
                    "nothing was"
                } else {
                    "warnings were"
                }
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn there_is_an_example_for_every_check() {
    let all: String = examples()
        .iter()
        .map(|path| fs::read_to_string(path).expect("example is readable"))
        .map(|source| expected(&source).join(" "))
        .collect::<Vec<_>>()
        .join(" ");

    for code in [
        "AttributeSetAfterHeader",
        "ImageNotFound",
        "SkippingReferenceToMissingAttribute",
    ] {
        assert!(
            all.contains(code),
            "no example in tests/lints reports {code}"
        );
    }
}
