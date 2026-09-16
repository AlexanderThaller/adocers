# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Layout

A Cargo workspace whose root is *also* the `adocers` binary package. The CLI is a
thin shell; everything that turns a parsed document into output is a library:

- `crates/adocers-html` — the HTML5 back end, deliberately mirroring
  Asciidoctor's converter (same wrapper `div`s and class names).
- `crates/adocers-typst` — the PDF back end. `pdf()` takes a base directory
  because a document names its images relative to where it was read from.
- `crates/adocers-render-core` — what the two back ends must agree on: section
  numbering, header metadata, admonition icons, mermaid SVG, escaping.

Neither back end depends on the other. Shared behaviour belongs in
`adocers-render-core`, not in one back end reaching into the other.

`src/lib.rs` does `pub use adocers_html as render;` — **`adocers::render` is a
re-export of the `adocers-html` crate, not a module.** Edit the crate.

## Commands

```
cargo test --workspace
cargo clippy --workspace --all-targets
cargo +nightly fmt --all            # plain `cargo fmt` inside `nix develop`
```

`.rustfmt.toml` uses nightly-only options, so **formatting needs nightly**. The
flake devshell installs a nightly-rustfmt-only toolchain beside a stable one
without rustfmt, so `cargo fmt` resolves correctly there.

`nix develop` gives the pinned toolchain. `nix fmt` (nixfmt) formats `.nix`.

Before calling a change done, run all three commands above. For a change to
either back end, also render `resources/showcase.adoc` before and after and
confirm the output is byte-identical — or say in the commit what moved and why.

## Style

`.rustfmt.toml` departs from rustfmt's defaults in ways worth knowing before
writing code, so that the formatter has nothing to undo:

- `imports_granularity = "Crate"` with `imports_layout = "Vertical"` — one `use`
  per crate, and **every braced item on its own line, even when there are two.**
- `format_strings` and `wrap_comments` — long string literals and comments are
  rewrapped to the width. Never hand-wrap either; let the formatter do it.

Prose in comments and docs is written as full sentences. Match it.

## Lints

`[workspace.lints]` is strict and every crate opts in with `[lints] workspace = true`.

- `unsafe_code` is **forbidden**, not merely denied.
- clippy `pedantic`, plus `unwrap_used`, `todo` and `dbg_macro` warn. Tests are
  exempt from the first two via `.clippy.toml`.
- `allow_attributes` and `allow_attributes_without_reason` warn: write
  `#[expect(lint, reason = "…")]`, never a bare `#[allow]`.
- `.clippy.toml` disallows `str::eq_ignore_ascii_case` and
  `str::to_ascii_lowercase` — "Always use unicode methods."
- `missing_docs` and `unreachable_pub` warn, so anything `pub` is documented
  *and* actually reachable. Reach for `pub(crate)` first and widen only when
  something outside the crate needs it.

A crate cannot merge `[lints] workspace = true` with a local `[lints]` table —
the manifest fails to load. Per-crate lints go in `lib.rs` as `#![warn(…)]`,
which is where the three libraries forbid themselves a console:
`clippy::print_stderr` and `print_stdout`. A library has no business writing to
a stream its caller does not control; hand the caller something to report
instead, as `adocers_typst::Pdf::fallback` does.

## Tests

Two git submodules under `resources/` feed the suites; initialise with
`git submodule update --init`.

- `tests/doctest.rs` renders Asciidoctor's own corpus against committed
  fixtures. **Without the `resources/asciidoctor-doctest` submodule it passes
  silently** — the skip is only visible under `cargo test -- --nocapture`.
- Its `KNOWN_FAILURES` list is checked in both directions: closing a gap makes
  the build fail until the name is removed from the list. `FEATURE_FAILURES` is
  gated on the `math` feature, so `--no-default-features` expects a different
  set.
- `tests/lints.rs` drives the built binary over `tests/lints/*.adoc`. Each
  example's **first line must be `// expect: Code@line …` or `// expect: nothing`**
  or the test panics. A new check needs a new example document.
- `tests/doctest/regenerate.sh` rebuilds fixtures and needs a real `asciidoctor`
  on PATH; name the Asciidoctor version in the commit message.

## Gotchas

- `.cargo/config.toml` sets `-Ctarget-cpu=x86-64-v3`. Binaries built here will
  not run on older x86 and the flag is invalid on aarch64 — which is why
  `flake.nix` excludes the file from its source fileset.
- `target/` is a **symlink** to a cache outside the tree. Do not `rm -rf` it.
- `resources/*.html` is a build product and is gitignored. Never commit it.
- The version lives only in `[workspace.package]`; the flake reads it from
  there. Bump it in that one place.
- Two dependencies need rustc 1.96, which nixpkgs release branches lag behind.
  That is why the flake pins its toolchain through rust-overlay — do not
  "simplify" it back to `pkgs.rustPlatform`.

## Commits

Commit to `main` directly; do not branch or open a PR unless asked.

Messages follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/),
lowercase and in the imperative, with a scope where one fits (`serve`, `pdf`,
`render`, `mermaid`, `check`, `nix`, …). Subjects here read as English sentences
rather than terse fragments — `fix(mermaid): scope the theme by selector, not by
full stop` — with no trailing period.

Add a body when the reasoning is not evident from the diff: what was wrong, why
the fix is shaped the way it is, and what was done to verify it. Wrap it at
about 70 columns. A trivial change needs only its subject.
