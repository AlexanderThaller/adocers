# Vendored mermaid

`mermaid.min.js` is [mermaid](https://mermaid.js.org) **12.0.0**, taken from

    https://cdn.jsdelivr.net/npm/mermaid@12.0.0/dist/mermaid.min.js

and compiled into the binary by `src/render/diagram.rs`, so that a page this
tool renders draws its diagrams without reaching the network.

This is the UMD build, not the ESM one. The ESM entry point is a small loader
that pulls in several dozen chunk files at run time, which can only be vendored
as a whole directory; the UMD build is one self-contained file that defines
`window.mermaid`.

mermaid is MIT licensed; `LICENSE` is its license text, taken from the same
release.

## Updating

```
curl -sL https://cdn.jsdelivr.net/npm/mermaid@<version>/dist/mermaid.min.js \
    -o vendor/mermaid/mermaid.min.js
curl -sL https://cdn.jsdelivr.net/npm/mermaid@<version>/LICENSE \
    -o vendor/mermaid/LICENSE
```

Then update the version in this file and in `BUNDLE_FILE` and `ASSET_HREF` in
`src/render/diagram.rs`. The version belongs in the file name: the asset is
served with a long lifetime, so reusing a name across an upgrade would leave
every browser that cached the old bundle holding it.
