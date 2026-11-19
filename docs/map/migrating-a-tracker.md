# Migrating a tracker

## What it is
`gltree migrate`: a tracker written in the first-line format turned into headers, every derived list
filled and every missing link planned, as a dry run, a saved snapshot, or written to the server.

## Governing decisions
- [ADR-0004](../adr/0004-references-live-in-a-toml-header.md) — gltree reads the header only; a
  first-line tracker is turned into headers by `migrate`, which alone still reads the old lines.

## Design model
- **The old head** (`read_head`): from the top of a body, every head line, blank lines between them
  allowed, up to the first line that is not one. A head line (`read_line`, split at `: `) is
  `Parent: #N` with exactly one number and nothing after; a from word with one number; a related word,
  `Blocked by` or `Depends on` with one or more numbers joined by `, `; or a line whose key, lowercased
  with spaces as `_`, is a declared extra key, taken whole as its string. A key line that names no
  number ends the head and stays in the body.
- **Notes**: text after the numbers becomes the key's note, the whole value as written; two lines of
  one key join lists and notes with `; `. A `from` equal to the `parent` keeps the parent and moves its
  note there.
- **The body** is everything after the last head line, leading blank lines dropped; the header adds one
  blank line back; the rest is byte for byte.
- **The plan** (`plan`): heads turned into headers over a copy, every derived list filled from that copy
  (`derived::expected` / `rebuilt`), then each reference the old tracker lacks a link for, each pair
  once — only where the old item recorded links. Only items whose description changes are listed, so a
  migrated tracker plans nothing.
- **Modes**: `--snapshot` plans from a file; `--out` saves the tracker as it would read afterwards
  (`applied`) for `check --snapshot`; `--apply` (repo config needed) writes every description, then
  every link, and a failure says how far it got — a rerun picks up, since written items plan nothing.
- **Words are options, not config**: once a tracker is migrated nothing reads them again; `From` and
  `Related` are always known. `check`'s `old-header` reads the old format with its own reader and the
  repo config's `old_head_keys`: it is the detector, this is the mover, and a line this reader refuses
  (`Parent: #4 (a reason)`) is one `check` reports for a person to move
  ([checking-the-tracker](checking-the-tracker.md); reading, 2026-11-19).
- Measured: dry runs on each consumer's snapshot — one migrated to a clean `check`; the other left only
  links between related issues that no head line names (commit 11bddfb, 2026-10-22).

## Code
- `src/migrate.rs — Words, Change, Plan, numbers, Line, read_line, read_head, plan, applied, apply, counts, render, to_json, JSON_VERSION`
- `src/main.rs — Command::Migrate`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [Every reference has a link](every-reference-has-a-link.md) — the plan adds the links references lack.
- [Not recorded is not none](not-recorded-is-not-none.md) — no link is planned for an item whose links were not recorded.

## Blast radius
- [holding-references-in-the-header](holding-references-in-the-header.md) — a migrated description that `parse` refuses keeps its new header but gets no derived lists, silently (`plan`'s `let Ok(split) … else`).
- [keeping-the-derived-lists](keeping-the-derived-lists.md) — the plan's lists are `rebuilt`.
- [checking-the-tracker](checking-the-tracker.md) — `old-header` and `stray-link` are what a migration leaves for a person.
- [keeping-a-snapshot](keeping-a-snapshot.md) — `--out` writes through `replace_file`.
- [the-command-line-contract](the-command-line-contract.md) — the dry run's text and `--json` (version 1) are output; a new kind of planned item changes the shape.

## Known holes / open
- The note keeps the numbers, and a value with no reason still gets one. Tracked: #23.
- A number after a reason on the same key line is dropped (`numbers` stops at the first non-`#` text). Tracked: #22.
- A link between related items with no related line in either head is not explained; `check` reports it as `stray-link` and the consumer decides each one.
- A head line in the middle of a body is not read, and `check` does not report it either.
