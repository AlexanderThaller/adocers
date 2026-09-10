#!/usr/bin/env bash
#
# Regenerate the Asciidoctor reference output the doctest suite compares against.
#
# The examples come from the `asciidoctor-doctest` submodule, where each file
# holds many examples separated by `// .<name>` header lines. This splits them
# apart, converts the whole batch in one `asciidoctor` run, and writes the
# result to `fixtures/<file>__<name>.html`.
#
# Run this only when deliberately moving to a new Asciidoctor, and say which
# version in the commit message. The fixtures currently in the tree came from:
#
#     Asciidoctor 2.0.26
#
# Usage: tests/doctest/regenerate.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
corpus="$here/../../resources/asciidoctor-doctest/data/examples/asciidoc"
fixtures="$here/fixtures"

if [[ ! -d $corpus ]]; then
    echo "no corpus at $corpus — run: git submodule update --init" >&2
    exit 1
fi

command -v asciidoctor >/dev/null || { echo "asciidoctor is not on PATH" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Split each corpus file into one file per example.
python3 - "$corpus" "$work" <<'PY'
import os, re, sys, glob

corpus, work = sys.argv[1], sys.argv[2]

for path in sorted(glob.glob(os.path.join(corpus, "*.adoc"))):
    stem = os.path.basename(path)[:-5]
    name, body = None, []

    def flush():
        if name is None or not "".join(body).strip():
            return
        with open(os.path.join(work, f"{stem}__{name}.adoc"), "w") as out:
            out.write("".join(body).strip("\n") + "\n")

    for line in open(path):
        header = re.match(r"^// \.(\S+)\s*$", line)
        if header:
            flush()
            name, body = header.group(1), []
        else:
            body.append(line)

    flush()
PY

# One Asciidoctor run for the lot: it is the Ruby startup that costs, not the
# conversion.
asciidoctor --embedded --destination-dir "$work" "$work"/*.adoc

rm -f "$fixtures"/*.html
for html in "$work"/*.html; do
    cp "$html" "$fixtures/$(basename "$html")"
done

echo "wrote $(ls -1 "$fixtures"/*.html | wc -l) fixtures to $fixtures"
