# Listing roots

## What it is
`gltree roots`: the tracker's roots filtered by state, labels and closing date, for tools that act on
finished features.

## Governing decisions
- [ADR-0003](../adr/0003-where-an-item-was-found-is-not-what-it-is-part-of.md) — a find is not drawn
  in `tree`; `roots --json` carries `from` for callers that want the relation.

## Design model
- **A root is an item with no `parent`**, as the scheme reads it. An item whose `parent` names a deleted
  item is not a root here; `tree` draws it as one so it is not lost, and `check` reports it as
  `missing-parent`. Source: `roots`, `tree::draw`.
- **Filters combine with AND**: `--open` / `--closed`; `--label`, every one present; `--without-label`,
  none present, a trailing `*` matching a prefix (`reg::*` drops `reg::registered`, keeps `reg` and
  `xreg::y`); `--closed-since` / `--closed-until` on the first ten characters of `closed_at`, inclusive,
  implying closed. Source: `roots`, `label_matches`, `check_date`.
- **JSON** version 2 per root: `number, title, state, labels, closed_at, first_line, url, from, extra,
  blocked_by_open`. `first_line` is the body's first line after the header and its blank lines, without a
  CR. `from` and `extra` come from `header::loose`, `extra` through `header::extra_json`, which tree
  nodes share: every `[extra]` key is taken whatever the repo config declares, and a header out of form gives none rather than an error, since listing is not checking.
  `blocked_by_open` is judged from the same full read (`tree::open_blockers`).
- gltree knows no outside tracker: label names, and what to do with the roots, are the caller's. Its
  callers are outside skills: a weekly report and an unregistered-feature sweep, which read
  `extra` for a registration marker and filter with `--closed-since` / `--closed-until` and
  `--without-label` prefixes (read in those skills, 2026-11-19; not named here, maintainer, 2026-11-19).
  `serve`'s badges keep this: the repo config names the label prefixes to show, and gltree knows none
  by name ([serving-the-viewer](serving-the-viewer.md)). A badge's `missing` counts every root `tree`
  draws, so a closed item whose parent is absent counts there and not here.

## Code
- `src/roots.rs — Filter, label_matches, check_date, roots, closed_day, render, to_json, JSON_VERSION`
- `src/main.rs — Command::Roots`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [Not recorded is not none](not-recorded-is-not-none.md) — a closed item without `closed_at` stops a date filter.

## Blast radius
- [the-command-line-contract](the-command-line-contract.md) — the JSON fields, `extra` above all, are read by outside skills.
- [holding-references-in-the-header](holding-references-in-the-header.md) — `from` and `extra` depend on `loose` reading a block it cannot parse strictly.
- [keeping-a-snapshot](keeping-a-snapshot.md) — `--snapshot` needs `closed_at` and `url` in the file.

## Known holes / open
- A date filter over a closed item without `closed_at` (a snapshot older than #12) is an error naming the field, not an empty list.
- A full read takes about 50 s on a tracker of a few thousand items; `--snapshot` reuses one (commit e664d18, 2026-10-22).
- Closing reasons (`wontfix` and others) are not filtered unless the caller asks.
