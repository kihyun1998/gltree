# The territory map

<!-- grill-map build stamp: 8696d71 -->
<!-- grill-map synced at: b52eba1 (2026-11-06) -->

A linked note graph of what gltree does. It answers two questions before a change:

- **If I touch this, what else moves?** Open the territory that holds the code, then follow its
  `## Blast radius` and `## Cross-cutting invariants`.
- **What decision is this shape derived from?** Read the territory's `## Governing decisions`; a
  **None.** there is itself the answer — the shape rests on the reasons in `## Design model`.

## Territories
- [Reading the server](reading-the-server.md)
- [Holding references in the header](holding-references-in-the-header.md)
- [Keeping the derived lists](keeping-the-derived-lists.md)
- [Drawing a tree](drawing-a-tree.md)
- [Checking the tracker](checking-the-tracker.md)
- [Judging an item's state](judging-an-items-state.md)
- [Listing roots](listing-roots.md)
- [Writing the tree](writing-the-tree.md)
- [Migrating a tracker](migrating-a-tracker.md)
- [Keeping a snapshot](keeping-a-snapshot.md)
- [Showing an item](showing-an-item.md)
- [Reading the repo config](reading-the-repo-config.md)
- [The command-line contract](the-command-line-contract.md)
- [Shipping a release](shipping-a-release.md)

## Invariants
- [Every reference has a link, and every link has a reference](every-reference-has-a-link.md)
- [Not recorded is not none](not-recorded-is-not-none.md)

## How to read it
- `## Blast radius` is a checklist: each edge names what goes wrong over there. An item with nothing to do is a correct outcome.
- Every claim carries its evidence (a symbol, a record, a commit) or its source and date — `(maintainer, 2026-11-19)`, `(commit 4cec8b2, 2026-11-06)`, `(reading, 2026-11-19)` for a judgement made from the code.
- Section headings are the same in every note; an empty one holds **None.**
- Symbols, never line numbers. A fact another file owns is linked, not copied.
- A note keeps measured values, outside constraints and the reasons behind design judgements; history stays in commits.

## What it cannot answer
Source files and issues are text inside notes, not nodes: the map does not list every caller of a
symbol, and an issue's state is GitHub's. The callers outside this repository are described in
[the command-line contract](the-command-line-contract.md) without their names.

## Scope
Everything under `src/`, `tests/`, `.github/` and the crate's manifests and licences. Out of scope:
`docs/agents/` (how agents work in this repo), `.claude/`, `target/`. A territory appears when
something in the repo commits to it — code, a decision record, or an open issue with a design; an
absent note means nothing does.

## The shape
A pure core — the header, the derived lists, check, tree, roots, migrate, show — works on `Item`
values behind one port (`Tracker`), with three adapters: the GitLab server, a snapshot file, and an
in-memory fake for tests. `src/main.rs` reads the repo config once and dispatches. The largest gap:
callers outside the repository read its JSON and flags, and nothing in the repository checks those
fields against what they read.
