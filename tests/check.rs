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
