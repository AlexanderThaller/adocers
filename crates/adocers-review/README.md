# adocers-review

Read a directory of AsciiDoc on a phone, and leave comments on its blocks.

```
adocers-review docs/ --bind 0.0.0.0:8080
```

Open the printed address on the phone. A directory lists its documents and how
many open comments each has. In a document, tap a paragraph, list item, listing,
table or heading to select it, then use the bar at the bottom of the screen to
comment on it. **N open** lists every comment in the document; tapping a
comment's quote jumps to its block.

Documents are rendered by the same pipeline as `adocers serve`, and take the
same rendering options (`-a`, `--safe-mode`, `--css`, `--no-highlight`, …).

## Where comments go

Each document's comments are kept in a JSON file under `.adocers-review/` at
the root of the tree, at the document's own path:
`guide/install.adoc` → `.adocers-review/guide/install.adoc.json`. Commit the
directory to share a review, or ignore it. A file goes away when its last
comment is deleted.

A comment is anchored to the text of its block, not to a line number, so it
follows the block when the document around it changes. When the block's own
text changes, the comment no longer finds it; it is then listed as
**outdated**, with the quote it was left on, rather than lost.

## Who can reach it

The default address is only reachable from the same machine. `--bind 0.0.0.0:PORT`
listens on the network: anyone who can reach that address can read the
documents and add, resolve or delete comments. There is no sign-in. Names
beginning with a dot — `.git`, `.env`, the comment files — are never served.
