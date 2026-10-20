# The release chain, one recipe per link:
#
#   just check            what CI runs, run here first
#   just prepare 0.4.0    bump, lock, date the changelog, commit and tag
#   just package          what `cargo publish` would send, without sending it
#   just publish          send it to crates.io and push the commit and tag
#
# `just release 0.4.0` runs all four in that order. Write the release notes
# under `## Unreleased` in CHANGELOG.md before starting: `prepare` turns that
# heading into the version's and refuses to release an empty section.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List the recipes.
default:
    @just --list --unsorted

# Run everything CI runs: formatting, both lint passes and both test passes.
check:
    test -d resources/asciidoctor-doctest/data/examples/asciidoc \
        || { echo 'the doctest corpus is missing: git submodule update --init' >&2; exit 1; }
    cargo +nightly fmt --all --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo clippy --workspace --all-targets --locked --no-default-features -- -D warnings
    cargo test --workspace --locked
    cargo test --workspace --locked --no-default-features

# Bump every crate to VERSION, date the changelog, commit and tag.
prepare version: _clean _on-main
    #!/usr/bin/env bash
    set -euo pipefail
    version='{{ version }}'

    [[ $version =~ ^([0-9]+)\.([0-9]+)\.[0-9]+$ ]] \
        || { echo "not a release version: $version" >&2; exit 1; }
    requirement="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}"

    if git rev-parse -q --verify "refs/tags/$version" >/dev/null; then
        echo "tag $version already exists" >&2
        exit 1
    fi

    # The section under `## Unreleased` is what the release says about itself,
    # so an empty one is a release nobody has written up yet.
    awk '
        /^## Unreleased$/ { inside = 1; next }
        inside && /^## / { exit }
        inside && NF { found = 1 }
        END { exit !found }
    ' CHANGELOG.md || { echo 'CHANGELOG.md has nothing under ## Unreleased' >&2; exit 1; }

    # The version lives once, in `[workspace.package]`. The crates also name
    # each other by requirement, and a minor bump on 0.x that left those
    # behind would have the new `adocers` resolve the old back ends.
    sed -i -E \
        -e "s/^version = \"[^\"]+\"/version = \"$version\"/" \
        -e "s/^(adocers-[a-z-]+ = \{ version = )\"[^\"]+\"/\1\"$requirement\"/" \
        Cargo.toml
    cargo update --workspace

    sed -i "s/^## Unreleased\$/## Unreleased\n\n## $version/" CHANGELOG.md

    git add Cargo.toml Cargo.lock CHANGELOG.md
    git commit -m "chore(release): prepare $version"
    git tag "$version"

# Package and verify every crate the way crates.io would receive it.
package: (_cargo-publish "--dry-run")

# Publish every crate to crates.io, then push the release commit and its tag.
[confirm("Publish to crates.io? This cannot be undone.")]
publish: _clean _on-main _tagged package
    just _cargo-publish
    git push origin main "$(just _version)"

# Run the whole chain for VERSION: check, prepare, package and publish.
release version: check (prepare version) publish

# `cargo publish --workspace` checks each crate against the siblings it has just
# packaged by laying them over crates.io, and a `[source.crates-io]`
# replacement — a local mirror — sits in front of that overlay, so the second
# crate cannot find the first. Configuration can add a replacement but not take
# one away, so this runs cargo in a home of its own: the same configuration and
# credentials with the replacement cut out, and the same download cache.
_cargo-publish *args:
    #!/usr/bin/env bash
    set -euo pipefail
    home="${CARGO_HOME:-$HOME/.cargo}"
    direct="$(mktemp -d)"
    trap 'rm -rf "$direct"' EXIT
    chmod 700 "$direct"
    for entry in credentials.toml registry; do
        if [[ -e "$home/$entry" ]]; then ln -s "$home/$entry" "$direct/$entry"; fi
    done
    if [[ -e "$home/config.toml" ]]; then
        awk '
            /^\[/ { skip = ($0 == "[source.crates-io]") }
            !skip
        ' "$home/config.toml" > "$direct/config.toml"
    fi
    CARGO_HOME="$direct" cargo publish --workspace --locked {{ args }}

_version:
    @sed -n -E 's/^version = "([^"]+)"$/\1/p' Cargo.toml

_clean:
    @git diff --quiet HEAD && test -z "$(git status --porcelain)" \
        || { echo 'the working tree is not clean' >&2; exit 1; }

_on-main:
    @test "$(git branch --show-current)" = main \
        || { echo 'releases are made from main' >&2; exit 1; }

_tagged:
    @test "$(git tag --points-at HEAD)" = "$(just _version)" \
        || { echo "HEAD is not tagged $(just _version); run just prepare first" >&2; exit 1; }
