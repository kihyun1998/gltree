# Judging an item's state

## What it is
The state policy: the rule for when an item may be closed or open against the items below it. The
repo config chooses one; `check` asks it about every item, and the write commands ask it before
hanging an open item under a closed one.

## Governing decisions
- [ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md) — each consumer keeps its own state rule
  through the repo config (`roots-only`, or `every-parent`).
- [ADR-0003](../adr/0003-where-an-item-was-found-is-not-what-it-is-part-of.md) — `parent` means
  "part of", so both policies count a child; `from` counts toward nobody's state, and an open item
  may not go under a closed parent where the policy would then report it.

## Design model
- **The policy alone decides.** `check` hands it the item and a `Family` — whether it is a root, its
  direct children, all its descendants — and reports what it returns. A future policy that judges
  differently needs nothing from `check`. Source: `StatePolicy`, `Family`, `check_with`.
- **`roots-only`**: only an item with no `parent` at all is judged — closed with an open descendant, or
  open with every descendant closed; descendants at any depth. An item whose parent is missing is not
  a root here. Source: `RootsOnly::judge`.
- **`every-parent`**: every item with children, on its **direct** children only. A closed grandparent
  over a closed child and an open grandchild is not reported; the child is. Source:
  `EveryParent::judge`, test `every_parent_judges_direct_children_only`.
- **The write-time guard** (`refuse_closed_above` in `src/write.rs`): the policy judges the target
  parent and each ancestor, with the moving item as a direct child of the target and a descendant of
  all, one `level` per ancestor. Under `roots-only` a closed slice still takes a child; a closed root
  above it does not (test `a_closed_item_may_go_under_a_closed_parent` and the roots-only case).

## Code
- `src/check.rs — StatePolicy, Family, RootsOnly, EveryParent, descendants`
- `src/config.rs — POLICIES, RepoConfig::policy`
- `src/write.rs — refuse_closed_above`

## Reference behaviour
**None.**

## Cross-cutting invariants
**None.**

## Blast radius
- [checking-the-tracker](checking-the-tracker.md) — the last finding per item is the policy's; its kind names (`closed-parent-open-child`, `open-parent-all-closed`) are output.
- [writing-the-tree](writing-the-tree.md) — `refuse_closed_above` matches the policy's `closed-parent-open-child` kind by name; a policy that renames it disables the guard.
- [reading-the-repo-config](reading-the-repo-config.md) — a new policy needs its name in `POLICIES` and an arm in `RepoConfig::policy`.

## Known holes / open
- The guard walks ancestors through `header::parent_of`, while `check` walks them through the configured relation scheme; with one scheme they agree (reading, 2026-11-19).
