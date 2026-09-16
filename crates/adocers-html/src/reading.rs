//! Marking the outline entry for the section being read.
//!
//! An outline says what a document contains. Beside a long one it can also say
//! *where the reader is in it*, which is the difference between a list of links
//! and a sense of place — and it is the reason a docked outline is worth having
//! at all rather than one the reader scrolls away from.
//!
//! The mark is applied by a script rather than rendered in, for the same reason
//! the copy button is: a page with scripting switched off still gets an outline
//! that works, and a `--fragment` is someone else's page to furnish. Its style
//! travels with it, so a page rendered with `--no-css` — or against a
//! stylesheet written for Asciidoctor's class names — still shows the mark.
//!
//! # Which entry
//!
//! The entry marked is the one for the *last heading the reader has scrolled
//! past*, which is the only choice that moves in one direction as the page
//! does. Marking whichever heading is on screen instead looks right until a
//! section title and its first subsection title arrive together — and then the
//! mark jumps up a level and back down again as the two pass, on every section
//! boundary of the document.

/// Whether a rendered body has an outline to mark anything in.
///
/// A document with no sections has no outline, and one rendered with `:toc!:`
/// has none either; neither should carry the script.
pub(crate) fn wanted(body: &str) -> bool {
    // The container's class is `toc` for an outline in the flow and `toc2` for
    // a docked one. Both are worth marking: the docked one because the reader
    // can see it, and the one in the flow because a reader who scrolls back up
    // to it finds out where they had got to.
    body.contains("class=\"toc\"") || body.contains("class=\"toc2\"")
}

/// The markup appended to a page whose outline can be marked.
pub(crate) const SCRIPT: &str = r##"<style>
#toc a.is-active { color: var(--accent, #1565a8); font-weight: 600; }
@media print { #toc a.is-active { font-weight: inherit; } }
</style>
<script>
(function () {
  var links = [].slice.call(document.querySelectorAll('#toc a[href^="#"]'));

  if (!links.length) { return; }

  // An entry whose target is gone cannot be marked, and a document can carry
  // an outline entry for a section whose id an author removed by hand.
  var entries = [];

  for (var i = 0; i < links.length; i++) {
    var id = decodeURIComponent(links[i].hash.slice(1));
    var heading = document.getElementById(id);

    if (heading) { entries.push({ heading: heading, link: links[i] }); }
  }

  if (!entries.length) { return; }

  var marked = null;

  // How far down the viewport a heading counts as reached. A standalone page
  // has nothing fixed across its top, so this is only enough to keep a heading
  // resting exactly at the edge from flickering between two entries.
  var LINE = 24;

  function update() {
    var current = entries[0];

    for (var i = 0; i < entries.length; i++) {
      if (entries[i].heading.getBoundingClientRect().top > LINE) { break; }
      current = entries[i];
    }

    // The last section of a document may be too short to ever reach the line,
    // which would leave its entry unreachable.
    if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2) {
      current = entries[entries.length - 1];
    }

    if (current === marked) { return; }

    if (marked) { marked.link.classList.remove("is-active"); }

    marked = current;
    current.link.classList.add("is-active");

    // A docked outline scrolls on its own, so the marked entry has to be
    // brought into *its* view rather than the page's.
    var panel = current.link.closest("#toc");

    if (panel && panel.scrollHeight > panel.clientHeight) {
      var entry = current.link.getBoundingClientRect();
      var frame = panel.getBoundingClientRect();

      if (entry.top < frame.top || entry.bottom > frame.bottom) {
        panel.scrollTop += entry.top - frame.top - frame.height / 3;
      }
    }
  }

  // Scroll fires far more often than the page can change what it shows, so the
  // work is done once per frame at most.
  var pending = false;

  function schedule() {
    if (pending) { return; }

    pending = true;

    window.requestAnimationFrame(function () {
      pending = false;
      update();
    });
  }

  window.addEventListener("scroll", schedule, { passive: true });
  window.addEventListener("resize", schedule);
  update();
})();
</script>"##;
