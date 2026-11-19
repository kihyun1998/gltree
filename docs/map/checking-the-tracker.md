# Checking the tracker

## What it is
`gltree check`: every way a tracker breaks the convention, read from the whole tracker (server or
snapshot), one finding per break, in a fixed order; exit 1 when there is any.

## Governing decisions
- [ADR-0004](../adr/0004-references-live-in-a-toml-header.md) — what is in form, and every link
  explained by a reference.
- [ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md) — a `parent`
  without its link is invisible to a tree walk, so it is reported.
- [ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md) — gltree is the convention's one
  implementation; state rules are the repo's.

## Design model
- **Order.** Items by number; per item: `header`, `old-header`, `task`, `missing-parent`,
  `missing-reference`, `unlinked` (the parent's, then the others' in `refs_of` order: `from`, `related`,
  `blocked_by`), `stray-link`, `derived`, `blocker-cycle`, `cycle`, then the state policy's finding.
  Text `{kind:<26} #N  detail`, then `N anomalies`. Source: `check_with`, `render`.
- **`header`** is `header::parse`'s refusal, word for word, plus a reference key naming the item
  itself (`Header::naming`). The detail is the reason, so one finding names one fix. A header `check`
  refuses is not judged for `derived`.
- **References are read loosely** (`refs_of`, and `parent_of` through the scheme): a value in form
  counts even beside a key `check` refuses, so one stray key does not also detach a subtree or turn
  every link stray. A first line `Parent: #N` is no parent (test `an_old_first_line_makes_no_parent`).
- **`old-header`** reports a body still opening with a first-line head
  ([holding-references-in-the-header](holding-references-in-the-header.md), `old_head`). It is a
  detector, wider than the mover: `migrate` leaves `Parent: #4 (a reason)` in the body (its
  `read_line` takes a Parent line only with nothing after the number), and `check` reports that same
  line, so a person moves it by hand. The two readers take their words from different places — the
  repo config's `old_head_keys` here, `migrate`'s one-off flags there — and are not one rule
  (reading of `old_head` and `migrate::read_line`, 2026-11-19).
- **Links**: `unlinked` for every reference without its `relates_to` link; `stray-link` for a link no
  reference on either end explains, once per pair at the lower number. Both need the item's links to
  have been recorded: `links: None` reports nothing
  ([not recorded is not none](not-recorded-is-not-none.md)).
- **`derived`** is [keeping-the-derived-lists](keeping-the-derived-lists.md)' `changes`, one finding
  per list, naming the `gltree sync N` that fixes it. `related` is judged as the union of both ends,
  so only a missing mirror is reported.
- **Cycles**: `blocker-cycle` when following `blocked_by` comes back to the item; `cycle` when
  following parents up does, so an item under a loop is reported with the loop's members.
- **The full read** is paged GraphQL, 100 items a page, with type, labels and link ends; an item
  whose links overflow one page fails the read, since `unlinked` needs every link
  ([reading-the-server](reading-the-server.md), `parse_page`).
- **Tests**: `tests/golden/check.txt` against `tests/fixtures/tracker.json` (`tests/cli.rs`). The
  golden was first printed by the consumers' Python tool, since deleted; a deliberate output change
  is now reviewed and the golden rewritten from gltree's own output (maintainer, 2026-11-19).

## Code
- `src/check.rs — Finding, check, check_with, render, to_json, JSON_VERSION`
- `src/main.rs — Command::Check`
- `tests/cli.rs — check_matches_the_python_tool_byte_for_byte_and_exits_1, check_json_lists_each_finding`; `tests/golden/check.txt`; `tests/fixtures/tracker.json`

## Reference behaviour
- The consumers' Python tree tool: the kinds, order and text of the first-line era's findings were
  matched to it, live, before it was deleted ([ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md);
  commit 6be32e0, 2026-10-22). It is no longer a reference: nothing is left to compare against.

## Cross-cutting invariants
- [Every reference has a link](every-reference-has-a-link.md) — `unlinked` and `stray-link` are its check.
- [Not recorded is not none](not-recorded-is-not-none.md) — `links: None` silences the link findings.

## Blast radius
- [the-command-line-contract](the-command-line-contract.md) — kind names, the text line and exit 1 are read by callers; `--json` is versioned.
- [migrating-a-tracker](migrating-a-tracker.md) — a migration is judged by `check --snapshot` on its `--out` file; `old-header` is what it leaves for a person.
- [judging-an-items-state](judging-an-items-state.md) — the policy gets `check`'s `Family`; a change to how roots, children or descendants are built changes every state finding.

## Known holes / open
- A full read takes about 45 s for a few thousand items: pages are fetched one after another, each waiting for the previous cursor (commit 6be32e0, 2026-10-22).
- A tracker written to while it is read gives a torn picture: a child created a moment before its link reads as `unlinked` on one pass and clean on the next (commit 6be32e0, 2026-10-22).
- `old-header` knows only listed keys: an old word reports only once it is in `old_head_keys`; a head below a heading is not judged.
