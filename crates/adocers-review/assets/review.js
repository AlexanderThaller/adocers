// The review layer over a rendered document.
//
// Tapping a block selects it; the bar at the foot of the screen then offers
// to comment on it. A block with open comments is marked with how many. The
// comments live on the server, which knows nothing about blocks: each one is
// stored with the fingerprint of the text it was left on, and found again on
// the next render by looking for a block with the same fingerprint. A comment
// whose text has since changed finds nothing, and is listed as outdated rather
// than lost.
(() => {
  "use strict";

  const settings = JSON.parse(document.getElementById("review-settings").textContent);
  const content = document.getElementById("content");

  if (!content) {
    return;
  }

  // What a comment can be left on. A tap picks the innermost of these, so a
  // paragraph inside a list item is its own block.
  const BLOCKS = [
    "div.paragraph",
    "div.listingblock",
    "div.literalblock",
    "div.admonitionblock",
    "div.imageblock",
    "div.quoteblock",
    "div.verseblock",
    "div.stemblock",
    "div.videoblock",
    "div.audioblock",
    "div.dlist",
    "div.hdlist",
    "table.tableblock",
    "li",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
  ].join(", ");

  // A tap on one of these does what it does, and selects nothing.
  const INTERACTIVE = "a, button, input, textarea, select, summary, label, video, audio";

  const NAME_KEY = "adocers-review:name";
  const query = "?document=" + encodeURIComponent(settings.document);
  const relative = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

  let blocks = [];
  let comments = [];
  let outdated = [];
  let selected = null;
  let sheet = null; // null, "block" or "all"
  let showResolved = false;
  let draft = "";
  let error = "";

  // ---- Finding blocks -------------------------------------------------------

  // cyrb53: short, fast and well spread, which is all a fingerprint needs.
  function hash(text) {
    let h1 = 0xdeadbeef;
    let h2 = 0x41c6ce57;

    for (let i = 0; i < text.length; i++) {
      const code = text.charCodeAt(i);
      h1 = Math.imul(h1 ^ code, 2654435761);
      h2 = Math.imul(h2 ^ code, 1597334677);
    }

    h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507);
    h1 ^= Math.imul(h2 ^ (h2 >>> 13), 3266489909);
    h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507);
    h2 ^= Math.imul(h1 ^ (h1 >>> 13), 3266489909);

    return (4294967296 * (2097151 & h2) + (h1 >>> 0)).toString(16);
  }

  // A block's own text: without the blocks inside it, which are blocks of
  // their own, and without buttons another script may or may not have added
  // yet.
  function textOf(element) {
    const copy = element.cloneNode(true);

    for (const inner of copy.querySelectorAll(BLOCKS + ", button")) {
      inner.remove();
    }

    const text = copy.textContent.replace(/\s+/g, " ").trim();

    if (text) {
      return text;
    }

    // A picture has no text, but it has a name.
    const image = element.querySelector("img");
    return image ? image.getAttribute("src") || "" : "";
  }

  function kindOf(element) {
    return element.tagName === "DIV" ? element.classList[0] : element.tagName.toLowerCase();
  }

  function index() {
    const seen = new Map();
    blocks = [];

    for (const element of content.querySelectorAll(BLOCKS)) {
      if (element.closest("#toc, .toc")) {
        continue;
      }

      const text = textOf(element);
      const fingerprint = hash(text);
      const occurrence = seen.get(fingerprint) || 0;
      seen.set(fingerprint, occurrence + 1);

      element.classList.add("review-block");

      blocks.push({
        element,
        fingerprint,
        occurrence,
        quote: text.slice(0, 160),
        kind: kindOf(element),
        comments: [],
      });
    }
  }

  function blockFor(anchor) {
    return (
      blocks.find(
        (block) =>
          block.fingerprint === anchor.fingerprint && block.occurrence === anchor.occurrence,
      ) ||
      blocks.find((block) => block.fingerprint === anchor.fingerprint) ||
      null
    );
  }

  // Put every comment on its block, and mark the blocks that have open ones.
  function place() {
    for (const block of blocks) {
      block.comments = [];
    }

    outdated = [];

    for (const comment of comments) {
      const block = blockFor(comment.anchor);
      (block ? block.comments : outdated).push(comment);
    }

    for (const block of blocks) {
      const open = block.comments.filter((comment) => !comment.resolved).length;
      block.element.classList.toggle("review-has", open > 0);

      if (open > 0) {
        block.element.dataset.reviewCount = String(open);
      } else {
        delete block.element.dataset.reviewCount;
      }
    }

    draw();
  }

  // ---- Talking to the server ------------------------------------------------

  async function call(method, path, body) {
    const response = await fetch(settings.api + path + query, {
      method,
      headers: body ? { "Content-Type": "application/json" } : {},
      body: body ? JSON.stringify(body) : undefined,
    });

    const answer = await response.json().catch(() => ({}));

    if (!response.ok) {
      throw new Error(answer.error || `The server answered ${response.status}.`);
    }

    return answer;
  }

  async function attempt(work) {
    error = "";

    try {
      await work();
    } catch (failure) {
      error = failure.message || String(failure);
    }

    place();
  }

  function load() {
    return attempt(async () => {
      comments = await call("GET", "");
    });
  }

  function post(block, body, author) {
    return attempt(async () => {
      const comment = await call("POST", "", {
        anchor: {
          fingerprint: block.fingerprint,
          occurrence: block.occurrence,
          quote: block.quote,
          kind: block.kind,
        },
        author,
        body,
      });

      comments.push(comment);
      draft = "";
    });
  }

  function change(comment, fields) {
    return attempt(async () => {
      const changed = await call("PATCH", "/" + encodeURIComponent(comment.id), fields);
      comments = comments.map((each) => (each.id === changed.id ? changed : each));
    });
  }

  function remove(comment) {
    return attempt(async () => {
      await call("DELETE", "/" + encodeURIComponent(comment.id));
      comments = comments.filter((each) => each.id !== comment.id);
    });
  }

  // ---- Drawing --------------------------------------------------------------

  // Build an element. Text is always set as text, never parsed as markup.
  function make(tag, attributes, ...children) {
    const element = document.createElement(tag);

    for (const [name, value] of Object.entries(attributes || {})) {
      if (name.startsWith("on")) {
        element.addEventListener(name.slice(2), value);
      } else if (value !== false && value != null) {
        element.setAttribute(name, value === true ? "" : value);
      }
    }

    for (const child of children.flat()) {
      if (child != null && child !== false) {
        element.append(child instanceof Node ? child : document.createTextNode(String(child)));
      }
    }

    return element;
  }

  function ago(seconds) {
    const delta = seconds - Date.now() / 1000;
    const units = [
      ["year", 31536000],
      ["month", 2592000],
      ["week", 604800],
      ["day", 86400],
      ["hour", 3600],
      ["minute", 60],
    ];

    for (const [unit, size] of units) {
      if (Math.abs(delta) >= size) {
        return relative.format(Math.round(delta / size), unit);
      }
    }

    return relative.format(0, "second");
  }

  function savedName() {
    try {
      return localStorage.getItem(NAME_KEY) || "";
    } catch {
      return "";
    }
  }

  function saveName(name) {
    try {
      localStorage.setItem(NAME_KEY, name);
    } catch {
      // Remembering the name is a convenience; without storage it is asked
      // for again, which is all.
    }
  }

  const bar = make("div", { class: "review-ui review-bar" });
  const panel = make("div", { class: "review-ui review-sheet", role: "dialog", hidden: true });
  document.body.append(panel, bar);
  document.body.classList.add("review-page");

  function drawBar() {
    const open = comments.filter((comment) => !comment.resolved).length;
    const all = make(
      "button",
      { type: "button", class: "review-all", onclick: () => openSheet("all") },
      open === 1 ? "1 open" : `${open} open`,
    );

    let action;

    if (selected) {
      const count = selected.comments.length;
      action = make(
        "button",
        { type: "button", class: "review-primary", onclick: () => openSheet("block") },
        count === 0 ? "Comment" : count === 1 ? "1 comment" : `${count} comments`,
      );
    } else {
      action = make("span", { class: "review-hint" }, "Tap a block to comment");
    }

    bar.replaceChildren(all, action);
  }

  function drawComment(comment, withQuote) {
    const block = blockFor(comment.anchor);
    let confirming = false;

    const deleteButton = make("button", { type: "button", class: "review-danger" }, "Delete");

    deleteButton.addEventListener("click", () => {
      if (confirming) {
        remove(comment);
        return;
      }

      // Asking twice in place, rather than in a dialog, keeps the thumb where
      // it already is.
      confirming = true;
      deleteButton.textContent = "Really delete?";

      setTimeout(() => {
        confirming = false;
        deleteButton.textContent = "Delete";
      }, 3000);
    });

    return make(
      "li",
      { class: comment.resolved ? "review-comment review-resolved" : "review-comment" },
      withQuote &&
        (block
          ? make(
              "button",
              { type: "button", class: "review-quote", onclick: () => select(block, true) },
              comment.anchor.quote || "(a block without text)",
            )
          : make(
              "p",
              { class: "review-quote review-outdated" },
              make("strong", {}, "Outdated: "),
              comment.anchor.quote || "(a block without text)",
            )),
      make(
        "p",
        { class: "review-meta" },
        make("strong", {}, comment.author),
        " · ",
        ago(comment.created),
        comment.resolved ? " · resolved" : "",
      ),
      make("p", { class: "review-body" }, comment.body),
      make(
        "div",
        { class: "review-actions" },
        make(
          "button",
          { type: "button", onclick: () => change(comment, { resolved: !comment.resolved }) },
          comment.resolved ? "Reopen" : "Resolve",
        ),
        deleteButton,
      ),
    );
  }

  function drawForm(block) {
    const name = make("input", {
      type: "text",
      class: "review-name",
      placeholder: "Your name",
      autocomplete: "name",
      value: savedName(),
    });

    const text = make("textarea", {
      class: "review-text",
      rows: 3,
      placeholder: "Say something about this block",
      oninput: (event) => {
        draft = event.target.value;
      },
    });

    text.value = draft;

    const send = make("button", { type: "submit", class: "review-primary" }, "Post");

    return make(
      "form",
      {
        class: "review-form",
        onsubmit: (event) => {
          event.preventDefault();

          if (!text.value.trim()) {
            text.focus();
            return;
          }

          saveName(name.value.trim());
          send.disabled = true;
          post(block, text.value, name.value.trim());
        },
      },
      text,
      make("div", { class: "review-row" }, name, send),
    );
  }

  function drawSheet() {
    if (!sheet) {
      panel.hidden = true;
      panel.replaceChildren();
      return;
    }

    const close = make(
      "button",
      { type: "button", class: "review-close", "aria-label": "Close", onclick: closeSheet },
      "✕",
    );

    const children = [];

    if (sheet === "block" && selected) {
      children.push(
        make(
          "div",
          { class: "review-head" },
          make("p", { class: "review-title" }, selected.quote || "(a block without text)"),
          close,
        ),
      );

      if (selected.comments.length > 0) {
        children.push(
          make(
            "ul",
            { class: "review-list" },
            selected.comments.map((comment) => drawComment(comment, false)),
          ),
        );
      }

      children.push(drawForm(selected));
    } else {
      const shown = comments.filter((comment) => showResolved || !comment.resolved);

      // In the order of the document, with what can no longer be placed last.
      const order = (comment) => {
        const block = blockFor(comment.anchor);
        return block ? blocks.indexOf(block) : Infinity;
      };

      shown.sort((a, b) => order(a) - order(b) || a.created - b.created);

      const toggle = make("input", {
        type: "checkbox",
        checked: showResolved,
        onchange: (event) => {
          showResolved = event.target.checked;
          drawSheet();
        },
      });

      children.push(
        make(
          "div",
          { class: "review-head" },
          make("p", { class: "review-title" }, "All comments"),
          make("label", { class: "review-toggle" }, toggle, " resolved"),
          close,
        ),
      );

      children.push(
        shown.length > 0
          ? make(
              "ul",
              { class: "review-list" },
              shown.map((comment) => drawComment(comment, true)),
            )
          : make(
              "p",
              { class: "review-empty" },
              comments.length > 0
                ? "Every comment is resolved."
                : "No comments yet. Tap a block to start.",
            ),
      );
    }

    if (error) {
      children.push(make("p", { class: "review-error", role: "alert" }, error));
    }

    panel.replaceChildren(...children);
    panel.hidden = false;
  }

  function draw() {
    drawBar();
    drawSheet();
  }

  // ---- Selecting ------------------------------------------------------------

  // Bring a block into view above the sheet, which covers the lower half of a
  // phone's screen.
  function reveal(block) {
    const top = block.element.getBoundingClientRect().top + window.scrollY;
    window.scrollTo({ top: top - window.innerHeight * 0.12, behavior: "smooth" });
  }

  function select(block, openThread) {
    if (selected) {
      selected.element.classList.remove("review-selected");
    }

    if (selected !== block) {
      draft = "";
    }

    selected = block;

    if (block) {
      block.element.classList.add("review-selected");
    }

    if (openThread && block) {
      sheet = "block";
      reveal(block);
    } else if (sheet === "block" && !block) {
      sheet = null;
    }

    error = "";
    draw();
  }

  function openSheet(mode) {
    sheet = mode;
    error = "";

    if (mode === "block" && selected) {
      reveal(selected);
    }

    draw();
  }

  function closeSheet() {
    sheet = null;
    error = "";
    draw();
  }

  content.addEventListener("click", (event) => {
    if (event.target.closest(INTERACTIVE)) {
      return;
    }

    // Someone selecting text to copy it is not choosing a block.
    if (String(window.getSelection() || "")) {
      return;
    }

    const element = event.target.closest(".review-block");
    const block = element && blocks.find((each) => each.element === element);

    if (!block) {
      return;
    }

    select(selected === block ? null : block, sheet === "block");
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && sheet) {
      closeSheet();
    }
  });

  // Coming back to the tab, perhaps after someone else has commented.
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) {
      load();
    }
  });

  index();
  draw();
  load();
})();
