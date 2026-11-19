# Drawing a tree

## What it is
`gltree tree`: the tree under an item, or every root's tree that holds an open item (every one with
`--all`), as text or JSON, with closed/total
descendants per node and open blockers — read from a snapshot, from the whole tracker, or by walking
the server one depth at a time.

## Governing decisions
- [ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md) — the child
  records its parent; links are the way down, one request per depth; a linked item is a child only
  when its own header names the node.

## Design model
- **Three sources.** `tree N --snapshot F` and `tree --snapshot F` draw from the file; `tree` with no
  number reads every item from the server; `tree N` from the server walks (`main`, `Command::Tree`).
- **The walk** (`subtree`): one `level` per depth — the level's items and every item linked to them,
  bodies included. A linked item joins only when the scheme names the node as its parent; a link has
  no direction, so the parent itself and unrelated links come back and are dropped. A seen-set stops a
  cycle. `live_tree` then asks one more `level` for the blockers outside the tree, all at once, for
  their state. Cost grows with the tree, never with the tracker: about 2.5 s against about 35 s for
  reading every body ([ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md)).
  `move` reuses `subtree` to refuse a target below the item.
- **The text**: `#N [state] title`, then `  closed/total` only with descendants, then
  `  blocked by #B, #C` while a blocker is open (a closed one, and one the tracker does not hold, are
  left out); two spaces per depth; children and roots by number. Source: `render_into`, `open_blockers`.
- **Roots without a number**: an item with no `parent` **or** whose parent is not in the tracker, so a
  child of a deleted item is not lost (`draw`). [listing-roots](listing-roots.md) and the state policy
  count only the first kind; `check` reports the second as `missing-parent`.
- **Finished families are left out without a number** (#25): after drawing and before rendering, a
  root whose state is closed and whose `closed` equals its `total` is dropped with everything under
  it (`with_open_work`, `Command::Tree`). The rule reads the drawn `Node`, so a cycle or a deleted
  parent cannot make a family half visible, and `render`, `to_json` and the counts are unchanged.
  `--all` turns it off and conflicts with a number; `tree N` always draws, so naming an item never
  gives an empty answer. Nothing inside a family is filtered and nothing marks what was left out.
- **A cycle ends where the path meets itself**: each node carries the items from the root down to it,
  not every item seen, so counts follow each path (`node`). This matched the Python tool's per-path
  counting (commit 2a78fa5, 2026-10-21).
- **JSON** version 2: `{version, roots: [node]}`, `node = {number, title, state, closed, total,
  blocked_by_open, children}`. Titles are printed raw.
- **Goldens**: `tests/golden/tree-*.txt` for `tests/fixtures/tracker.json`, stored LF and pinned by
  `.gitattributes`; gltree prints LF everywhere. They were printed by the consumers' Python tool
  (commit 2a78fa5), which is deleted; a deliberate output change is reviewed and the goldens are
  rewritten from gltree's own output (maintainer, 2026-11-19). `tree-all.txt` is the default
  (finished families left out) and `tree--all.txt` is `tree --all`; fixture #17–#18 is the finished
  family, #7 the lone closed root, #12 and #15 the families that stay (#25). The fixture moved to headers in #16
  and every tree golden stayed byte for byte (commit aa86291, 2026-10-22).
- Input bound: a snapshot is read whole into memory; the walk is bounded by the tree.

## Code
- `src/tree.rs — Node, tree, live_tree, with_open_work, states, blockers, open_blockers, subtree, draw, node, render, render_into, to_json, node_json, JSON_VERSION`
- `src/scheme.rs — RelationScheme, FrontMatter`
- `src/main.rs — Command::Tree`
- `tests/cli.rs`; `tests/golden/tree-1.txt, tree-3.txt, tree-9.txt, tree-all.txt, tree--all.txt`; `.gitattributes`

## Reference behaviour
- The consumers' Python tree tool, byte for byte, until it was deleted (commit 2a78fa5; live on both
  consumers' servers, commit 2bcb3f2, 2026-10-21). No longer a reference.

## Cross-cutting invariants
- [Every reference has a link](every-reference-has-a-link.md) — the walk sees only linked children.

## Blast radius
- [reading-the-server](reading-the-server.md) — the walk's cost and correctness rest on `level`: 100 items a request, and an overflow of linked items is an error rather than a short tree.
- [writing-the-tree](writing-the-tree.md) — `move` refuses a cycle through `subtree`; a walk that misses children lets a move under its own descendant through.
- [the-command-line-contract](the-command-line-contract.md) — the text and `--json` are read by outside skills.

## Known holes / open
- An item with more than 100 links cannot be walked; the error says to draw it from a snapshot.
