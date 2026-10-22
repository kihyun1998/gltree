# Drawing a tree

How gltree turns a tracker's items into the tree text and JSON (`src/tree.rs`, `src/scheme.rs`,
`src/snapshot.rs`, `src/main.rs`).

## Design model

- The output is held to the consumers' Python tree tool byte for byte: `#N [state] title`, then
  `  closed/total` only when the node has descendants, two spaces per depth, children by number,
  roots by number. The goldens under `tests/golden/` were printed by that tool for the invented
  `tests/fixtures/tracker.json`; regenerate them with it, never by hand.
- Without a root, a root is an item with no `parent` **or** whose parent is not in the tracker
  (a deleted parent), as in the Python tool.
- The goldens were printed from the fixture in the first-line format; the fixture now holds the
  same relations as headers (#14 a header that does not parse, as its misplaced line did before),
  and every tree golden is unchanged byte for byte.
- A cycle ends where the path meets itself: each node remembers the items from the root down to
  it, not every item seen, so the counts equal the Python tool's per-path counting.
- An item with an open blocker ends its line with `  blocked by #B, #C`, after `closed/total`; a
  closed blocker, and one the tracker does not hold, are left out. A fixture with no `blocked_by`
  draws exactly what the Python tool drew.
- A live tree knows its own items' states; the blockers outside it cost one more `level` request,
  for all of them at once.
- The JSON (version 2) is `{version, roots: [node]}` with `node = {number, title, state, closed,
  total, blocked_by_open, children}`. Raise `JSON_VERSION` on any change a reader could notice.

## Known holes / open

- The Python tool falls back to the old Task hierarchy (`hparent`) when a body has no Parent line;
  gltree reads the header only. A snapshot that still holds Tasks draws differently. No consumer
  holds Tasks now.
- A snapshot is read whole into memory; fine at a few thousand items.

## Traps

- `u64::from_str` accepts a leading `+`, so `Parent: #+7` would parse; the Python regex refused it.
  The digits check in `ParentLine::parse` (kept for turning a first-line tracker into headers)
  exists for that line. In the header, TOML itself refuses `parent = #7` and `parent = "7"`.
- The Python tool's stdout on Windows writes CRLF; the goldens are stored LF (`.gitattributes`
  pins them), and gltree prints LF everywhere.
- Mutation-testing a copy of the crate: a copy keeps file mtimes, so cargo reuses the previous
  mutation's build for unchanged files and the results lie. Touch every file after copying.
