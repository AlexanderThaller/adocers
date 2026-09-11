//! The `check` subcommand, driven through the binary.

use std::{
    fs,
    path::PathBuf,
    process::Command,
};

/// Write `source` to a scratch file named for the test, and hand back its path.
fn scratch(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("adocers-check-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch directory is writable");

    let path = dir.join(format!("{name}.adoc"));
    fs::write(&path, source).expect("scratch file is writable");
    path
}

/// Build a scratch tree of its own for the calling test, with `files` written
/// at the relative paths given, and hand back the directory it all sits in.
fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir()
        .join(format!("adocers-check-{}", std::process::id()))
        .join(name);

    // A test that ran before could have left something behind that this one
    // would then find and report on.
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("scratch directory is writable");

    for (path, source) in files {
        let path = root.join(path);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("scratch directory is writable");
        }

        fs::write(&path, source).expect("scratch file is writable");
    }

    root
}

/// A document with one warning in it, which `check` reports as
/// `AttributeSetAfterHeader`.
const WARNS: &str = "= Title\n\n:axis: spec\n\nBody.\n";

/// A document `check` has nothing to say about.
const CLEAN: &str = "= Title\n\nBody.\n";

/// Run `adocers check` on `paths` and hand back the exit status and stderr.
fn check(paths: &[&PathBuf]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_adocers"))
        .args(["check", "--color", "never"])
        .args(paths)
        .output()
        .expect("the binary runs");

    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_clean_document_passes_quietly() {
    let path = scratch("clean", "= Title\n:axis: spec\n\nBody.\n");
    let (ok, stderr) = check(&[&path]);

    assert!(ok, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn a_warning_fails_the_run_and_is_shown() {
    let path = scratch("gap", "= Title\n\n:axis: spec\n\nBody.\n");
    let (ok, stderr) = check(&[&path]);

    assert!(!ok);
    assert!(stderr.contains("[AttributeSetAfterHeader]"), "{stderr}");
    assert!(stderr.contains("1 warning(s) in 1 file"), "{stderr}");
}

#[test]
fn nothing_is_written_beside_the_input() {
    let path = scratch("silent", "= Title\n\nBody.\n");
    let (ok, _) = check(&[&path]);

    assert!(ok);
    assert!(!path.with_extension("html").exists());
}

#[test]
fn every_file_is_checked_even_after_one_fails() {
    let first = scratch("first", "= Title\n\n:a: 1\n");
    let second = scratch("second", "= Title\n\n:b: 2\n");
    let (ok, stderr) = check(&[&first, &second]);

    assert!(!ok);
    assert!(stderr.contains("`:a:`"), "{stderr}");
    assert!(stderr.contains("`:b:`"), "{stderr}");
    assert!(stderr.contains("2 warning(s) in 2 files"), "{stderr}");
}

#[test]
fn json_puts_one_object_on_each_line() {
    let path = scratch("json", "= Title\n\n:axis: spec\n\nBody.\n");

    let output = Command::new(env!("CARGO_BIN_EXE_adocers"))
        .args(["check", "--message-format", "json"])
        .arg(&path)
        .output()
        .expect("the binary runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
        .collect();

    assert!(!output.status.success());
    assert_eq!(lines.len(), 2, "{stderr}");

    let diagnostic = &lines[0];
    assert_eq!(diagnostic["type"], "diagnostic");
    assert_eq!(diagnostic["severity"], "warning");
    assert_eq!(diagnostic["code"], "AttributeSetAfterHeader");
    assert_eq!(diagnostic["line"], 3);
    assert_eq!(diagnostic["column"], 1);
    assert!(diagnostic["help"].is_string());
    assert!(diagnostic["origin"].is_null());
    assert_eq!(
        diagnostic["file"].as_str(),
        Some(path.to_string_lossy().as_ref())
    );

    assert_eq!(
        lines[1],
        serde_json::json!({"type": "summary", "warnings": 1, "files": 1})
    );
}

#[test]
fn json_reports_an_error_as_an_object() {
    let output = Command::new(env!("CARGO_BIN_EXE_adocers"))
        .args([
            "check",
            "--message-format",
            "json",
            "/nonexistent/adocers-check.adoc",
        ])
        .output()
        .expect("the binary runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let line: serde_json::Value = serde_json::from_str(stderr.trim()).expect("one JSON object");

    assert!(!output.status.success());
    assert_eq!(line["type"], "error");
    assert!(
        line["message"]
            .as_str()
            .is_some_and(|m| m.contains("reading"))
    );
}

#[test]
fn a_directory_stands_for_every_document_under_it() {
    let root = tree(
        "nested",
        &[
            ("top.adoc", WARNS),
            ("deep/inner.adoc", WARNS),
            ("deep/deeper/innermost.asciidoc", WARNS),
            ("clean.adoc", CLEAN),
        ],
    );

    let (ok, stderr) = check(&[&root]);

    assert!(!ok);
    assert!(stderr.contains("top.adoc"), "{stderr}");
    assert!(stderr.contains("inner.adoc"), "{stderr}");
    assert!(stderr.contains("innermost.asciidoc"), "{stderr}");
    assert!(stderr.contains("3 warning(s) in 4 files"), "{stderr}");
}

#[test]
fn a_directory_of_clean_documents_passes_quietly() {
    let root = tree(
        "all-clean",
        &[("top.adoc", CLEAN), ("deep/inner.adoc", CLEAN)],
    );

    let (ok, stderr) = check(&[&root]);

    assert!(ok, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn hidden_files_and_hidden_directories_are_checked_too() {
    let root = tree(
        "hidden",
        &[
            (".hidden.adoc", WARNS),
            (".github/workflow.adoc", WARNS),
            ("visible.adoc", CLEAN),
        ],
    );

    let (ok, stderr) = check(&[&root]);

    assert!(!ok);
    assert!(stderr.contains(".hidden.adoc"), "{stderr}");
    assert!(stderr.contains("workflow.adoc"), "{stderr}");
    assert!(stderr.contains("2 warning(s) in 3 files"), "{stderr}");
}

#[test]
fn only_asciidoc_is_picked_up_out_of_a_directory() {
    let root = tree(
        "mixed",
        &[
            ("doc.adoc", CLEAN),
            ("doc.asciidoc", CLEAN),
            ("doc.ad", CLEAN),
            ("doc.asc", CLEAN),
            // Not AsciiDoc by name, and so not read: as a document either
            // would be one long warning.
            ("notes.txt", "= Title\n\n:axis: spec\n"),
            ("picture.png", "\u{fffd}not text at all"),
        ],
    );

    let (ok, stderr) = check(&[&root]);

    assert!(ok, "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn a_file_named_outright_is_checked_whatever_it_is_called() {
    let root = tree("named", &[("notes.txt", WARNS)]);
    let path = root.join("notes.txt");

    let (ok, stderr) = check(&[&path]);

    assert!(!ok);
    assert!(stderr.contains("notes.txt"), "{stderr}");
}

#[test]
fn a_directory_holding_no_documents_is_an_error() {
    let root = tree("empty", &[("README.md", "nothing to check here\n")]);
    let (ok, stderr) = check(&[&root]);

    assert!(!ok);
    assert!(stderr.contains("no AsciiDoc documents"), "{stderr}");
}

#[test]
fn documents_are_reported_in_path_order() {
    let root = tree(
        "ordered",
        &[
            ("b.adoc", "= B\n\n:b: 1\n"),
            ("a.adoc", "= A\n\n:a: 1\n"),
            ("a-dir/c.adoc", "= C\n\n:c: 1\n"),
        ],
    );

    let (_, first) = check(&[&root]);
    let (_, again) = check(&[&root]);

    assert_eq!(first, again);

    let order = |needle: &str| first.find(needle).unwrap_or_else(|| panic!("{first}"));
    assert!(order("`:c:`") < order("`:a:`"), "{first}");
    assert!(order("`:a:`") < order("`:b:`"), "{first}");
}
