//! Where the time goes when a document is rendered.
//!
//! The pipeline is three steps — read the source, parse it, turn the block tree
//! into markup — and syntax highlighting sits inside the third. Each is timed
//! separately, because they are worth very different amounts of attention: a
//! change to the renderer cannot be judged against a number that is mostly
//! parsing, and highlighting has a fixed cost that swamps both until it is
//! measured on its own.
//!
//! ```
//! cargo bench                      # everything
//! cargo bench -- highlight         # one group
//! cargo bench -- --save-baseline before
//! cargo bench -- --baseline before # after a change
//! ```

use std::{
    fmt::Write as _,
    hint::black_box,
    path::Path,
};

use adocers::render::{
    self,
    Options,
};
use asciidoc_parser::Parser;
use criterion::{
    BenchmarkId,
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};

/// The documents measured, and what each is here to say.
///
/// `showcase` is the realistic case: every construct this tool renders, in the
/// proportions a real document uses them. The other two isolate the two things
/// that turned out to dominate, so a change to either can be seen without the
/// rest of the page moving around it.
fn corpus() -> Vec<(&'static str, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut documents = vec![
        ("showcase", read(&root.join("resources/showcase.adoc"))),
        ("one-source-block", ONE_SOURCE_BLOCK.to_string()),
        ("prose-only", prose(400)),
        ("tables", tables(40)),
        ("lists", lists(200)),
    ];

    // The writer's guide is a real 2,300-line document, which the doctest
    // submodule is not guaranteed to have been checked out for.
    let guide = root.join("resources/asciidoctor.org/docs/asciidoc-writers-guide.adoc");

    if guide.is_file() {
        documents.push(("writers-guide", read(&guide)));
    }

    documents
}

/// The smallest document that pays the whole cost of highlighting.
const ONE_SOURCE_BLOCK: &str = "= One block\n\n[source,rust]\n----\nfn main() {}\n----\n";

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A document of nothing but paragraphs, as the floor for everything else.
fn prose(paragraphs: usize) -> String {
    let mut out = String::from("= Prose\n\n");

    for index in 0..paragraphs {
        let _ = write!(
            out,
            "Paragraph {index} carries *bold*, _italic_, `monospace` and a link to\n\
             https://asciidoc.org[the syntax reference], which is about as much inline\n\
             substitution as ordinary writing asks for.\n\n"
        );
    }

    out
}

/// Tables, which do the most markup-building per line of source.
fn tables(count: usize) -> String {
    let mut out = String::from("= Tables\n\n");

    for index in 0..count {
        let _ = write!(
            out,
            "[cols=\"1,2,1\",options=\"header\"]\n|===\n|Name |Description |Count\n\n|Alpha \
             {index} |The first entry |1\n|Beta {index} |The second entry |2\n|Gamma {index} |The \
             third entry |3\n|===\n\n"
        );
    }

    out
}

/// Deeply nested lists, which are the other shape that recurses.
fn lists(items: usize) -> String {
    let mut out = String::from("= Lists\n\n");

    for index in 0..items {
        let _ = write!(
            out,
            "* item {index}\n** nested {index}\n*** deeper {index}\n"
        );
    }

    out
}

/// Parsing alone, which is `asciidoc-parser`'s work rather than this crate's.
///
/// It is measured anyway: it is the floor under every render, and knowing where
/// it sits is what says whether a slow document is this crate's fault.
fn parse(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("parse");

    for (name, source) in corpus() {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, source| {
            b.iter(|| black_box(Parser::default().parse(black_box(source.as_str()))));
        });
    }

    group.finish();
}

/// Rendering a parsed document, with highlighting off.
///
/// This is the number to watch when changing the back end: it is the markup
/// building and nothing else.
fn render_plain(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("render");

    let options = Options {
        fragment: true,
        highlight: false,
        ..Options::default()
    };

    for (name, source) in corpus() {
        let mut parser = Parser::default();
        let document = parser.parse(source.as_str());

        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &document,
            |b, document| {
                b.iter(|| black_box(render::render(black_box(document), &options)));
            },
        );
    }

    group.finish();
}

/// Rendering with highlighting on, against the same documents.
///
/// The difference from `render` is what the grammars cost. Criterion runs a
/// warm-up first, so the one-off grammar compilation is not in these numbers —
/// see `highlight_cold` for that.
#[cfg(feature = "highlight")]
fn render_highlighted(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("render-highlighted");

    let options = Options {
        fragment: true,
        highlight: true,
        ..Options::default()
    };

    for (name, source) in corpus() {
        let mut parser = Parser::default();
        let document = parser.parse(source.as_str());

        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &document,
            |b, document| {
                b.iter(|| black_box(render::render(black_box(document), &options)));
            },
        );
    }

    group.finish();
}

/// What the first highlighted block in a process costs.
///
/// Every grammar is compiled the first time any of them is wanted, and that
/// cost lands on whichever document asks first. It cannot be measured in
/// process — criterion runs many iterations in one, and the second is already
/// warm — so each iteration is a fresh run of the real binary.
///
/// That means process startup is in the number. The `no-highlight` case is
/// here to subtract it: the difference between the two is what the grammars
/// cost a one-shot render. A `serve` process pays it once and never again.
#[cfg(feature = "highlight")]
fn cold_start(criterion: &mut Criterion) {
    use std::process::{
        Command,
        Stdio,
    };

    let binary = env!("CARGO_BIN_EXE_adocers");
    let document = std::env::temp_dir().join("adocers-bench-one-block.adoc");
    std::fs::write(&document, ONE_SOURCE_BLOCK).expect("scratch file is writable");

    let mut group = criterion.benchmark_group("cold-start");

    // A whole process per iteration, so there is no point taking hundreds.
    group.sample_size(20);

    for (name, flag) in [
        ("highlight", &[][..]),
        ("no-highlight", &["--no-highlight"][..]),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                let status = Command::new(binary)
                    .args(["render", "--fragment", "-q"])
                    .args(flag)
                    .arg("-o")
                    .arg("-")
                    .arg(&document)
                    .stdout(Stdio::null())
                    .status()
                    .expect("the binary runs");

                assert!(status.success());
            });
        });
    }

    group.finish();

    let _ = std::fs::remove_file(&document);
}

/// The whole pipeline as the command line runs it: read, parse, render, page.
fn pipeline(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("pipeline");

    let options = Options {
        stylesheet: Some(render::default_stylesheet()),
        icons: true,
        highlight: cfg!(feature = "highlight"),
        ..Options::default()
    };

    for (name, source) in corpus() {
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, source| {
            b.iter(|| {
                let mut parser = Parser::default();
                let document = parser.parse(black_box(source.as_str()));

                black_box(render::render(&document, &options))
            });
        });
    }

    group.finish();
}

#[cfg(feature = "highlight")]
criterion_group!(
    benches,
    parse,
    render_plain,
    render_highlighted,
    cold_start,
    pipeline
);

#[cfg(not(feature = "highlight"))]
criterion_group!(benches, parse, render_plain, pipeline);

criterion_main!(benches);
