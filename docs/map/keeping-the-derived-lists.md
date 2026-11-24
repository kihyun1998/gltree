# Keeping the derived lists

## What it is
The lists gltree writes on the other end of a relation — `children` from `parent`, `blocks` from
`blocked_by`, and the `related` mirror — worked out from the truth, compared with what a header
holds, and written back by `gltree sync`.

## Governing decisions
- [ADR-0004](../adr/0004-references-live-in-a-toml-header.md) — the truth stays on the writing end;
  a derived list that disagrees is reported and rebuilt, so two writers on one parent lose nothing
  for good.

## Design model
- **One calculation.** `expected` lists, for item N, the items whose `parent`, `blocked_by` or
  `related` names N (read with `refs_of`); `rebuilt` sets the lists from it; `changes` names each list
  that differs. `check` (the `derived` finding), `sync`, every write's sync step and `migrate`'s plan
  all call these, so they cannot disagree. Source: `src/derived.rs` and its callers.
- **`related` is the union of both ends**: a mirror is added, never taken away. Removing a related
  item is therefore an edit on both ends ([writing-the-tree](writing-the-tree.md)), not a sync.
  Source: `rebuilt`.
- **`sync N` reads one `level`**: N and the items linked to it. That is every item that can name N
  only because every reference has a link; a reference without its link is not seen until `check`'s
  `unlinked` is fixed. `sync` with no item reads the whole tracker once and indexes who names whom
  (`naming_index`). Source: `sync_one`, `sync_all`.
- **Only items that change are written**, through `with_header`, so the truth, `[notes]`, `[extra]`
  and the body stay; a second run writes nothing. A header out of form is refused, since rewriting it
  would drop what it cannot read. Source: `derived::write`.
- `sync` without `--dry-run` needs the repo config; `--dry-run` does not (`main`, `Command::Sync`).
- Measured on a real tracker's snapshot with parents turned into headers: `check` reported one
  `derived` per parent; an in-memory sync wrote those, and a second sync wrote nothing
  (commit 643a6c2, 2026-10-22).

## Code
- `src/derived.rs — Expected, expected, Change, rebuilt, changes, sync_one, sync_all, naming_index, write, render, to_json, JSON_VERSION`
- `src/main.rs — Command::Sync`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [Every reference has a link](every-reference-has-a-link.md) — `sync_one` sees only linked items.

## Blast radius
- [checking-the-tracker](checking-the-tracker.md) — the `derived` finding is `changes`; a change to the calculation changes what `check` reports.
- [writing-the-tree](writing-the-tree.md) — every write ends with `sync_one` on the other ends; it must leave no `derived` finding.
- [migrating-a-tracker](migrating-a-tracker.md) — the plan fills every list with `expected` / `rebuilt`.
- [the-command-line-contract](the-command-line-contract.md) — `sync --json` is versioned output.
- [drawing-a-tree](drawing-a-tree.md) — every tree node carries `blocks` as the header holds it, nothing computed in reverse, so a list `sync` has not repaired shows as it stands, on the `serve` page's relations box too.

## Known holes / open
- Two writers that sync the same item at once each write the whole description; the later one wins, and the next `sync` repairs the lists (ADR-0004's reasoning; not measured).
