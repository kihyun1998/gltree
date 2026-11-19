# Holding references in the header

## What it is
The TOML block between `+++` lines that opens an item's description: the only record of an item's
references, how gltree reads it (strictly to judge or rewrite, loosely to follow or display), and how
it writes it back without touching the body after it.

## Governing decisions
- [ADR-0004](../adr/0004-references-live-in-a-toml-header.md) — the block, its keys, the value
  rules, the fixed key order, unknown keys refused.
- [ADR-0003](../adr/0003-where-an-item-was-found-is-not-what-it-is-part-of.md) — `from` apart from
  `parent`, never the same number in both.

## Design model
- **Four readers, one fence.** Every reader starts from `fenced` (an opening `+++` line, the block up
  to the next `+++` line; an opening fence with no closing one is an error):
  `parse` is strict and returns the reason a block is out of form; `refs_of` takes the four keys a
  person writes (`parent`, `from`, `related`, `blocked_by`) as far as each value can be read, so one
  bad key hides nothing else; `parent_of` reads `parent` alone; `loose` takes every `[extra]` key the
  block holds and reads a block out of form as none, for display. Source: `src/header.rs`.
- **Strict means** (`parse`): valid TOML; only the keys of `REFS` plus `notes` and `extra`; numbers
  are positive integers; lists ascending, no number twice, never empty; `[notes]` keys from `NOTED`,
  each naming a key the header holds; `[extra]` keys the repo config declares; `from` not equal to
  `parent`. Each refusal is one sentence naming the fix. Source: `parse`, `list`, `notes`, `extras`.
- **Writing** (`render`, `with_header`): keys in the fixed order `parent`, `from`, `children`,
  `related`, `blocked_by`, `blocks`, then `[notes]`, then `[extra]`. The body after the closing
  fence is kept byte for byte. A block added in front of a body gets one blank line; an emptied
  header goes with that one blank line, so adding then removing a header gives the text back.
  Source: `with_header`.
- **A note goes with its key**: a write that empties a key drops its note
  (`Header::drop_orphan_notes`), because a note for an absent key is out of form. Found by the edit
  tests (commit 971f508, 2026-10-22: "the edit tests found an emptied `related` leaving its note behind").
- **The first-line format** is recognised here only to report it: `old_head` is the body's first
  non-blank line when it is one of `OLD_HEAD_KEYS` or the repo's `old_head_keys`, `: #`, digits, and
  then nothing, or a list, a reason in brackets or punctuation. The keys are a list, not a shape: a
  first cut that judged by shape also flagged a prose line (`Covers gaps: #2 (…) + #3 (…)`)
  (commit 49a434f, 2026-10-22). Moving such lines is [migrating-a-tracker](migrating-a-tracker.md)'s,
  with its own reader.
- Input bound: a description is whatever the server returns; nothing here bounds its size.

## Code
- `src/header.rs — FENCE, REFS, NOTED, Header, Header::drop_orphan_notes, Header::has, Header::naming, Split, fenced, parse, refs_of, OLD_HEAD_KEYS, old_head, loose, parent_of, render, with_header`
- `src/scheme.rs — FrontMatter::parent_of, ParentLine::parse` (`ParentLine` has no caller outside its test; ADR-0004 says only `migrate` reads the old format, and `migrate` has its own reader)

## Reference behaviour
**None.**

## Cross-cutting invariants
- [Every reference has a link](every-reference-has-a-link.md) — the references are what `refs_of` returns.

## Blast radius
- [checking-the-tracker](checking-the-tracker.md) — `header` findings are `parse`'s refusal text, and every other check reads references through `refs_of` and `parent_of`.
- [keeping-the-derived-lists](keeping-the-derived-lists.md) — the expected lists are built from `refs_of`, and the rewrite goes through `with_header`.
- [writing-the-tree](writing-the-tree.md) — every write reads the item strictly and rewrites it with `with_header`; a rule added to `parse` makes items that were writable refuse.
- [migrating-a-tracker](migrating-a-tracker.md) — its output must pass `parse`, or the plan keeps the old description.
- [listing-roots](listing-roots.md) — `from` and `extra` in its JSON come from `loose`.
- [drawing-a-tree](drawing-a-tree.md) — the tree is built from `parent_of`, and `blocked by #B` from `refs_of`.

## Known holes / open
- `refs_of` and `parent_of` each repeat the "positive integer" rule that `number` holds for `parse`; a change to one is not seen by the others (reading, 2026-11-19).
- `ParentLine::parse` is kept for a migration that does not call it (reading, 2026-11-19).
