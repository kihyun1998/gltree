# One rule for a finished family

## The fact
A family is finished when its root is closed and its `closed` equals its `total`: every item drawn
under it is closed. `gltree tree` with no number leaves such a family out, and `serve`'s page hides
it behind the same toggle; both must use this one rule.

## Why it is cross-cutting
The rule lives twice and the two do not call each other: `tree::with_open_work` in Rust, and
`finished` in `src/serve.html`, which filters the kept full read in the browser without asking the
server again (issue #27: the page applies the toggle itself). Nothing tests that they agree; a change
to one leaves the page and `tree` disagreeing about which families are done, with no test failing.

## Territories it holds in
- [drawing-a-tree](drawing-a-tree.md) — `with_open_work`, called by `Command::Tree` without a number.
- [serving-the-viewer](serving-the-viewer.md) — `finished` in the page, read by `pick` for the toggle (every count on the page spans every family) and by `branch` to fold a parent's done children.

## What a violation looks like
A family `gltree tree` prints shows up as finished on the page, or the page's default view lists a
family `gltree tree` leaves out.

## Discovery history
- #25 (commit a5d21d9): the rule written for `tree`.
- #27 (2026-11-19): the page copies it.

## Where it will recur
Any other reader of `tree --all --json` that picks the families with open work — a script, another
page — carries a third copy. Reading `closed`, `total` and `state` of the root is the whole rule.
