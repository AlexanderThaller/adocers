//! Renders Asciidoctor's own test corpus and compares the result against
//! Asciidoctor's.
//!
//! The examples live in the `asciidoctor-doctest` submodule, where each file
//! holds many of them separated by `// .<name>` header lines. The markup
//! Asciidoctor produces for each is committed under `tests/doctest/fixtures`,
//! so this suite needs neither Ruby nor a network;
//! `tests/doctest/regenerate.sh` rebuilds them when moving to a new
//! Asciidoctor.
//!
//! [`KNOWN_FAILURES`] is the part of Asciidoctor this back end does not match
//! yet. It is checked from both sides — an example outside the list must match,
//! and an example inside it must still differ — so closing a gap fails the
//! suite until the name is taken out, and the list cannot quietly go stale.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

/// Examples this back end does not render the way Asciidoctor does.
///
/// Each entry is `<corpus file>__<example name>`.
const KNOWN_FAILURES: &[&str] = &[
    // Deliberate. Asciidoctor hands an equation to the reader as the notation
    // it was written in, wrapped in delimiters for MathJax to find and rewrite.
    // This renderer converts it to `MathML` instead, which the browser draws
    // itself — so a page carries the equation rather than a typesetter.
    "stem__asciimath",
    "stem__latexmath",
    "stem__with-id-and-role",
    "stem__with-title",
    // Not this crate's to fix: a role on curly-quote quoting — `[why]"`text`"`
    // — should become a `<span>`, but inline content is rendered by
    // `asciidoc-parser` and it returns the text without one. `[.role]#text#`
    // and `[.role]_em_` do work, so the hole is specific to this form.
    "inline_quoted__double-with-role",
    "inline_quoted__single-with-role",
];

/// One example: the AsciiDoc that was written and the markup Asciidoctor makes
/// of it.
struct Example {
    name: String,
    source: String,
    expected: String,
}

/// Read every example in the corpus, pairing it with its committed fixture.
///
/// Returns `None` when the submodule is not checked out, which is the one
/// reason to skip rather than fail: a fresh clone has an empty directory there
/// until `git submodule update --init` has run.
fn examples() -> Option<Vec<Example>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus = root.join("resources/asciidoctor-doctest/data/examples/asciidoc");
    let fixtures = root.join("tests/doctest/fixtures");

    if !corpus.is_dir() {
        return None;
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&corpus)
        .expect("corpus is readable")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "adoc"))
        .collect();
    paths.sort();

    let mut examples = Vec::new();

    for path in paths {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("corpus file has a name");

        for (name, source) in split(&fs::read_to_string(&path).expect("corpus file is readable")) {
            let name = format!("{stem}__{name}");
            let fixture = fixtures.join(format!("{name}.html"));

            let expected = fs::read_to_string(&fixture).unwrap_or_else(|_| {
                panic!("no fixture for `{name}`; run tests/doctest/regenerate.sh")
            });

            examples.push(Example {
                name,
                source,
                expected,
            });
        }
    }

    Some(examples)
}

/// Split one corpus file into its examples.
///
/// A `// .<name>` line opens an example and runs until the next one. An example
/// with nothing but whitespace in it is not one.
fn split(file: &str) -> Vec<(String, String)> {
    let mut examples = Vec::new();
    let mut current: Option<(String, String)> = None;

    for line in file.lines() {
        if let Some(name) = line.strip_prefix("// .")
            && !name.is_empty()
            && !name.contains(char::is_whitespace)
        {
            if let Some(example) = current.take()
                && !example.1.trim().is_empty()
            {
                examples.push(example);
            }

            current = Some((name.to_string(), String::new()));
            continue;
        }

        if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }

    if let Some(example) = current
        && !example.1.trim().is_empty()
    {
        examples.push(example);
    }

    examples
}

/// Render one example the way the fixtures were made.
///
/// The three flags line the two renderers up on the axes where this one
/// diverges by design: Asciidoctor highlights in the browser rather than at
/// render time, and marks an admonition with a label rather than an icon.
fn render(name: &str, source: &str) -> String {
    let dir = std::env::temp_dir().join(format!("adocers-doctest-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch directory is writable");

    // Named for the example, because the two tests run at the same time and a
    // shared name would have each deleting the other's file mid-render.
    let path = dir.join(format!("{name}.adoc"));
    fs::write(&path, source).expect("scratch file is writable");

    let output = Command::new(env!("CARGO_BIN_EXE_adocers"))
        .args(["render", "--fragment", "--no-highlight", "--no-icons", "-q"])
        .arg("-o")
        .arg("-")
        .arg(&path)
        .output()
        .expect("the binary runs");

    let _ = fs::remove_file(&path);

    assert!(
        output.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8(output.stdout).expect("output is UTF-8")
}

/// Reduce markup to what is being compared.
///
/// The two renderers disagree harmlessly about vertical space — a blank line
/// inside a `sectionbody`, a trailing newline — and that noise would bury the
/// differences worth knowing about.
fn normalize(html: &str) -> String {
    html.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A unified-ish diff, enough to see which line went wrong.
fn diff(ours: &str, theirs: &str) -> String {
    let ours: Vec<&str> = ours.lines().collect();
    let theirs: Vec<&str> = theirs.lines().collect();
    let mut out = String::new();

    for index in 0..ours.len().max(theirs.len()) {
        match (ours.get(index), theirs.get(index)) {
            (Some(a), Some(b)) if a == b => {}
            (a, b) => {
                if let Some(a) = a {
                    let _ = writeln!(out, "  ours       {index:3}: {a}");
                }
                if let Some(b) = b {
                    let _ = writeln!(out, "  asciidoctor{index:3}: {b}");
                }
            }
        }
    }

    out
}

#[test]
fn matches_asciidoctor() {
    let Some(examples) = examples() else {
        eprintln!("skipped: the asciidoctor-doctest submodule is not checked out");
        return;
    };

    let known: BTreeSet<&str> = KNOWN_FAILURES.iter().copied().collect();
    let mut failures = String::new();
    let mut count = 0;

    for example in &examples {
        if known.contains(example.name.as_str()) {
            continue;
        }

        let ours = normalize(&render(&example.name, &example.source));
        let theirs = normalize(&example.expected);

        if ours != theirs {
            count += 1;
            let _ = write!(failures, "\n{}\n{}", example.name, diff(&ours, &theirs));
        }
    }

    assert!(
        failures.is_empty(),
        "{count} example(s) no longer match Asciidoctor:\n{failures}"
    );
}

#[test]
fn known_failures_are_still_failing() {
    let Some(examples) = examples() else {
        eprintln!("skipped: the asciidoctor-doctest submodule is not checked out");
        return;
    };

    let known: BTreeSet<&str> = KNOWN_FAILURES.iter().copied().collect();
    let names: BTreeSet<&str> = examples
        .iter()
        .map(|example| example.name.as_str())
        .collect();

    let unknown: Vec<&&str> = known
        .iter()
        .filter(|name| !names.contains(**name))
        .collect();
    assert!(
        unknown.is_empty(),
        "KNOWN_FAILURES names examples that do not exist: {unknown:?}"
    );

    let fixed: Vec<&str> = examples
        .iter()
        .filter(|example| known.contains(example.name.as_str()))
        .filter(|example| {
            normalize(&render(&example.name, &example.source)) == normalize(&example.expected)
        })
        .map(|example| example.name.as_str())
        .collect();

    assert!(
        fixed.is_empty(),
        "these now match Asciidoctor — take them out of KNOWN_FAILURES: {fixed:#?}"
    );
}
