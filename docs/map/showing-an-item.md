# Showing an item

## What it is
`gltree show N`: one item with its labels, body and comments, from a snapshot or the server, as text
a person reads during an outage or as JSON.

## Governing decisions
**None.**

## Design model
- **From the server** it reads one item with its own query (`item_query_with_comments`), since
  `level` brings no labels; it does not ask for links, which it does not print, so more than 100 links
  does not stop it (`read_item_with_comments` sets `links` to `None`).
- **The text cannot be forged by what it prints**: gltree's own lines sit at the margin and what people
  wrote is indented, with control characters taken out (a tab stays), so a body cannot pass for a
  comment head and its `[internal]` mark, and cannot drive the terminal. Source: `clean`, `body_lines`,
  `render`.
- **"Not recorded" is said**: `comments: None` prints `comments not recorded; take a fresh snapshot`,
  `Some([])` prints `no comments`; JSON gives `null` and `[]`.
- **JSON** version 1: `{version, number, title, state, labels, body, comments}`, the text as written.
  `body` keeps the header, since during an outage it holds the references.
- A number the source does not hold exits 2 and names it (`Error::NotFound`).
- Measured on a real tracker, one run: `show` printed the same bytes from the file and from the
  server, in text and `--json` (commit 4cec8b2, 2026-11-06).

## Code
- `src/show.rs — clean, body_lines, render, comment_head, to_json, JSON_VERSION`
- `src/gitlab.rs — read_item_with_comments, item_query_with_comments`
- `src/main.rs — Command::Show`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [Not recorded is not none](not-recorded-is-not-none.md) — the two comment lines.

## Blast radius
- [keeping-a-snapshot](keeping-a-snapshot.md) — `show --snapshot` and the snapshot share `Item` and the comment reads; a field added to one is owed to the other.
- [the-command-line-contract](the-command-line-contract.md) — `--json` is versioned output.

## Known holes / open
- The server query without links was changed after the real-tracker run above; its shape was accepted by gitlab.com written out by hand, and has not run on a self-hosted server (commit 4cec8b2).
- `tree` and `roots` still print titles raw, without `clean`.
