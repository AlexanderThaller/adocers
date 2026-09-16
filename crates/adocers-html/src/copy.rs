//! The button that copies a code block to the clipboard.
//!
//! The button is put into the page by a script rather than rendered into the
//! markup, for two reasons. A listing's markup is Asciidoctor's byte for byte,
//! and a `--fragment` is someone else's page to furnish; and a control that
//! only a script can make work has no business being in a page where scripting
//! is switched off.
//!
//! Its style travels with it instead of living in [`css`](crate::css), because
//! a page rendered with `--no-css`, or with a stylesheet written for
//! Asciidoctor's class names, still has to put the button in the right corner.
//! The colours are read from the built-in stylesheet's variables where they
//! exist and fall back to fixed values where they do not.

/// Whether a rendered body has anything worth copying.
///
/// A page with no verbatim block in it — a directory listing, a short status
/// page — should not carry the script at all.
pub(crate) fn wanted(body: &str) -> bool {
    body.contains("listingblock") || body.contains("literalblock")
}

/// The markup appended to a page that has a block to copy.
pub(crate) const SCRIPT: &str = r#"<style>
.adocers-copyable { position: relative; }
.adocers-copyable > pre { padding-right: 2.75rem; }
.adocers-copy {
  position: absolute; top: 0.5rem; right: 0.5rem;
  display: flex; align-items: center; justify-content: center;
  width: 1.9rem; height: 1.9rem; padding: 0;
  border: 1px solid var(--rule, #d0d7de); border-radius: 5px;
  background: var(--bg, #ffffff); color: var(--muted, #656d77);
  cursor: pointer; opacity: 0; transition: opacity 0.15s ease, color 0.15s ease;
}
.adocers-copy:hover { color: var(--accent, #1565a8); }
.adocers-copyable:hover > .adocers-copy,
.adocers-copy:focus-visible,
.adocers-copy.adocers-copied { opacity: 1; }
.adocers-copy.adocers-copied { color: var(--accent, #1565a8); }
/* There is no hovering on a touch screen, so there the button is simply
   there. */
@media (hover: none) { .adocers-copy { opacity: 1; } }
@media print { .adocers-copy { display: none; } }
</style>
<script>
(function () {
  var blocks = document.querySelectorAll(
    ".listingblock > .content, .literalblock > .content"
  );

  if (!blocks.length) { return; }

  var COPY = '<svg viewBox="0 0 16 16" width="16" height="16" fill="none" ' +
    'stroke="currentColor" stroke-width="1.4" stroke-linecap="round" ' +
    'stroke-linejoin="round" aria-hidden="true">' +
    '<rect x="6" y="6" width="8" height="8" rx="1.5"/>' +
    '<path d="M10 4V3a1 1 0 0 0-1-1H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h1"/></svg>';

  var DONE = '<svg viewBox="0 0 16 16" width="16" height="16" fill="none" ' +
    'stroke="currentColor" stroke-width="1.8" stroke-linecap="round" ' +
    'stroke-linejoin="round" aria-hidden="true">' +
    '<path d="M3 8.5l3.5 3.5L13 5"/></svg>';

  // What the reader would have selected by hand. A callout mark is drawn from
  // an attribute, but the `(1)` beside it that a page without a stylesheet
  // shows is real text, and neither belongs in the clipboard.
  function source(pre) {
    var clone = pre.cloneNode(true);
    var marks = clone.querySelectorAll(".conum, .conum + b");

    for (var i = 0; i < marks.length; i++) {
      marks[i].parentNode.removeChild(marks[i]);
    }

    return clone.textContent.replace(/[ \t]+$/gm, "");
  }

  // The clipboard API needs a secure context, which a page opened from disk or
  // served over plain HTTP may not be, and it can refuse even there. The old
  // selection-and-execCommand dance is what is left when it does.
  function write(text) {
    if (navigator.clipboard && window.isSecureContext) {
      return navigator.clipboard.writeText(text).catch(function () {
        return legacy(text);
      });
    }

    return legacy(text);
  }

  function legacy(text) {
    return new Promise(function (resolve, reject) {
      var area = document.createElement("textarea");

      area.value = text;
      area.setAttribute("readonly", "");
      area.style.position = "fixed";
      area.style.top = "0";
      area.style.left = "-9999px";
      document.body.appendChild(area);
      area.select();

      var copied = false;

      try { copied = document.execCommand("copy"); } catch (error) { copied = false; }

      document.body.removeChild(area);
      if (copied) { resolve(); } else { reject(new Error("copy refused")); }
    });
  }

  function attach(content) {
    var pre = content.querySelector("pre");

    if (!pre) { return; }

    var button = document.createElement("button");
    var pending = null;

    button.type = "button";
    button.className = "adocers-copy";
    button.innerHTML = COPY;
    button.title = "Copy to clipboard";
    button.setAttribute("aria-label", "Copy to clipboard");

    button.addEventListener("click", function () {
      write(source(pre)).then(function () {
        say("Copied", DONE, true);
      }, function () {
        say("Copying failed", COPY, false);
      });
    });

    function say(label, icon, copied) {
      button.innerHTML = icon;
      button.title = label;
      button.setAttribute("aria-label", label);
      button.classList.toggle("adocers-copied", copied);

      clearTimeout(pending);
      pending = setTimeout(function () {
        button.innerHTML = COPY;
        button.title = "Copy to clipboard";
        button.setAttribute("aria-label", "Copy to clipboard");
        button.classList.remove("adocers-copied");
      }, 1600);
    }

    content.classList.add("adocers-copyable");
    content.appendChild(button);
  }

  for (var i = 0; i < blocks.length; i++) { attach(blocks[i]); }
})();
</script>"#;
