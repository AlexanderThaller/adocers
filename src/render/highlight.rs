//! Syntax highlighting for source blocks, with tree-sitter.
//!
//! A highlighter that works from a grammar knows what it is looking at, so
//! `impl` in Rust is a keyword and `impl` in a comment is not. The grammars are
//! compiled in, which means highlighting happens while the page is rendered
//! rather than in the reader's browser: a page carries its colours as markup
//! and needs no script to show them.
//!
//! What comes out is Asciidoctor's `<pre class="highlight"><code>` with `<span
//! class="hl-…">` inside it, so a document renders the same whether or not this
//! feature was compiled in — a build without it simply leaves the code plain.

use std::{
    collections::HashMap,
    sync::OnceLock,
};

use tree_sitter_highlight::{
    HighlightConfiguration,
    Highlighter,
    HtmlRenderer,
};

/// The highlight captures recognized in a grammar's queries, and the class each
/// one is rendered with.
///
/// Order matters: tree-sitter identifies a capture by its index in this list.
/// Several captures deliberately share a class — a builtin function is still a
/// function to a reader — so that the stylesheet stays small enough to read.
const CAPTURES: &[(&str, &str)] = &[
    ("attribute", "hl-attr"),
    ("comment", "hl-comment"),
    ("constant", "hl-constant"),
    ("constant.builtin", "hl-constant"),
    ("constructor", "hl-type"),
    ("escape", "hl-escape"),
    ("function", "hl-function"),
    ("function.builtin", "hl-function"),
    ("function.method", "hl-function"),
    ("keyword", "hl-keyword"),
    ("label", "hl-label"),
    ("module", "hl-type"),
    ("number", "hl-number"),
    ("operator", "hl-operator"),
    ("property", "hl-property"),
    ("punctuation", "hl-punctuation"),
    ("punctuation.bracket", "hl-punctuation"),
    ("punctuation.delimiter", "hl-punctuation"),
    ("string", "hl-string"),
    ("string.special", "hl-string"),
    ("tag", "hl-tag"),
    ("type", "hl-type"),
    ("type.builtin", "hl-type"),
    ("variable", "hl-variable"),
    ("variable.builtin", "hl-keyword"),
    ("variable.parameter", "hl-parameter"),
];

/// Highlight `source` as `language`.
///
/// Returns `None` when the language is not one of the compiled-in grammars, or
/// when the source defeats the parser — in both cases the caller falls back to
/// plain escaped text, which is what an unhighlighted listing has always been.
pub fn highlight(language: &str, source: &str) -> Option<String> {
    let configuration = grammars().get(canonical_name(language).as_str())?;

    let mut highlighter = Highlighter::new();
    let events = highlighter
        .highlight(configuration, source.as_bytes(), None, None, |name| {
            // An injected language — the JavaScript inside an HTML document,
            // say — is highlighted with its own grammar when one is compiled in.
            grammars().get(canonical_name(name).as_str())
        })
        .ok()?;

    let mut renderer = HtmlRenderer::new();
    renderer
        .render(events, source.as_bytes(), &|highlight, attributes| {
            let class = CAPTURES
                .get(highlight.0)
                .map_or("hl-unknown", |(_, class)| *class);

            attributes.extend_from_slice(format!("class=\"{class}\"").as_bytes());
        })
        .ok()?;

    // The renderer escapes the source as it goes, so what comes back is markup.
    Some(renderer.lines().collect())
}

/// The name a language is registered under, resolving the aliases in common
/// use.
///
/// Case is folded, so `[source,Rust]` finds the same grammar `[source,rust]`
/// does: no grammar is told apart from another by case.
fn canonical_name(language: &str) -> String {
    let folded = language.to_lowercase();

    match folded.as_str() {
        "sh" | "shell" | "zsh" | "console" | "terminal" => "bash",
        "js" | "mjs" | "cjs" | "node" => "javascript",
        "ts" => "typescript",
        "py" | "python3" => "python",
        "rs" => "rust",
        "golang" => "go",
        "h" => "c",
        "yml" => "yaml",
        "htm" | "xhtml" => "html",
        "jsonc" => "json",
        _ => return folded,
    }
    .to_string()
}

/// One compiled-in grammar: the name it is registered under, its language, and
/// the highlight, injection and locals queries that drive it.
///
/// The language is a function rather than a value because a grammar is loaded
/// through a C symbol, which is not something a table can hold directly.
type Grammar = (
    &'static str,
    fn() -> tree_sitter::Language,
    &'static str,
    &'static str,
    &'static str,
);

/// Every grammar compiled into this binary.
fn table() -> [Grammar; 14] {
    // Queries a grammar does not ship are empty, which the highlighter reads as
    // "nothing to do" rather than as an error.
    [
        (
            "rust",
            || tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ),
        (
            "python",
            || tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        (
            "javascript",
            || tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ),
        (
            "typescript",
            || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ),
        (
            "tsx",
            || tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ),
        (
            "go",
            || tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        (
            "c",
            || tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
            "",
            "",
        ),
        (
            "bash",
            || tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::HIGHLIGHT_QUERY,
            "",
            "",
        ),
        (
            "java",
            || tree_sitter_java::LANGUAGE.into(),
            tree_sitter_java::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        (
            "json",
            || tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        (
            "toml",
            || tree_sitter_toml_ng::LANGUAGE.into(),
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        (
            "yaml",
            || tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
        (
            "html",
            || tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::HIGHLIGHTS_QUERY,
            tree_sitter_html::INJECTIONS_QUERY,
            "",
        ),
        (
            "css",
            || tree_sitter_css::LANGUAGE.into(),
            tree_sitter_css::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    ]
}

/// The compiled-in grammars, prepared once.
///
/// Preparing one means compiling its highlight queries, which is far too slow
/// to do per block, and the result is immutable and shareable, so the whole set
/// is built on first use and then read from by every render.
fn grammars() -> &'static HashMap<&'static str, HighlightConfiguration> {
    static GRAMMARS: OnceLock<HashMap<&'static str, HighlightConfiguration>> = OnceLock::new();

    GRAMMARS.get_or_init(|| {
        let names: Vec<&str> = CAPTURES.iter().map(|(name, _)| *name).collect();

        table()
            .into_iter()
            .filter_map(|(name, language, highlights, injections, locals)| {
                // A grammar whose queries do not compile is left out rather
                // than taking the whole set down with it: every other language
                // still highlights, and this one falls back to plain text.
                let mut configuration =
                    HighlightConfiguration::new(language(), name, highlights, injections, locals)
                        .ok()?;

                configuration.configure(&names);
                Some((name, configuration))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grammar_compiles_its_queries() {
        // `add` drops a grammar whose queries fail, so a missing name here is a
        // grammar that was compiled in but cannot actually be used.
        for name in [
            "rust",
            "python",
            "javascript",
            "typescript",
            "tsx",
            "go",
            "c",
            "bash",
            "java",
            "json",
            "toml",
            "yaml",
            "html",
            "css",
        ] {
            assert!(
                highlight(name, "x").is_some(),
                "grammar `{name}` did not load"
            );
        }
    }

    #[test]
    fn highlights_rust_keywords_and_strings() {
        let html = highlight("rust", "fn main() { let x = \"hi\"; }").unwrap_or_default();

        assert!(html.contains("hl-keyword"), "no keyword span: {html}");
        assert!(html.contains("hl-string"), "no string span: {html}");
        assert!(html.contains("main"));
    }

    #[test]
    fn resolves_the_aliases_people_write() {
        for (alias, code) in [("rs", "fn f() {}"), ("sh", "echo hi"), ("py", "x = 1")] {
            assert!(
                highlight(alias, code).is_some(),
                "`{alias}` was not recognized"
            );
        }
    }

    #[test]
    fn escapes_the_source_it_highlights() {
        let html = highlight("rust", "let a = b < c && d > e;").unwrap_or_default();

        assert!(html.contains("&lt;"), "unescaped `<`: {html}");
        assert!(html.contains("&gt;"), "unescaped `>`: {html}");
        assert!(html.contains("&amp;"), "unescaped `&`: {html}");
    }

    #[test]
    fn an_unknown_language_is_left_alone() {
        assert!(highlight("brainfuck", "+++.").is_none());
    }

    #[test]
    fn every_capture_has_a_class() {
        assert!(
            CAPTURES
                .iter()
                .all(|(name, class)| { !name.is_empty() && class.starts_with("hl-") })
        );
    }
}
