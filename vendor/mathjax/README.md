# Vendored MathJax

`tex-mml-svg.js` and `input/asciimath.js` are [MathJax](https://www.mathjax.org)
**3.2.2**, taken from

    https://cdn.jsdelivr.net/npm/mathjax@3.2.2/es5/tex-mml-svg.js
    https://cdn.jsdelivr.net/npm/mathjax@3.2.2/es5/input/asciimath.js

and compiled into the binary by `src/render/math.rs`, so that a page this tool
renders draws its equations without reaching the network.

This is the SVG output build rather than one of the CHTML ones. CHTML needs web
fonts fetched at run time, which is exactly what vendoring is meant to avoid;
the SVG build draws the glyphs itself and needs nothing but the script.

`input/asciimath.js` is a second file because MathJax 3 ships no combined build
with AsciiMath in it, and AsciiMath is what a plain `:stem:` means. MathJax's
loader finds it relative to the main bundle, so the two have to keep this
layout wherever they are written or served.

That is also why a page with the module *inlined* — `adocers -o -`, which has
nowhere to put a file beside itself — handles LaTeX but not AsciiMath: the
loader insists on fetching the second file, and there is no URL to fetch it
from. `--mathjax-url` points at a full copy for that case.

MathJax is Apache-2.0 licensed; `LICENSE` is its license text, taken from the
same release.

## Updating

```
curl -sL https://cdn.jsdelivr.net/npm/mathjax@<version>/es5/tex-mml-svg.js \
    -o vendor/mathjax/tex-mml-svg.js
curl -sL https://cdn.jsdelivr.net/npm/mathjax@<version>/es5/input/asciimath.js \
    -o vendor/mathjax/input/asciimath.js
curl -sL https://cdn.jsdelivr.net/npm/mathjax@<version>/LICENSE \
    -o vendor/mathjax/LICENSE
```

The version is part of the file name the renderer serves it under, in
`src/render/math.rs`; change it there to match.
