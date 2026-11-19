# Writing the tree

## What it is
`gltree new`, `move` and `edit`: adding to and changing a tracker so that each write leaves the header,
the links and the lists on the other ends in step, and a failure partway says exactly what is left.

## Governing decisions
- [ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md) — creating or
  moving a child writes the header and the link together.
- [ADR-0003](../adr/0003-where-an-item-was-found-is-not-what-it-is-part-of.md) — `new` and `move`
  refuse an open item under a closed parent the policy would then report, and offer `from`.
- [ADR-0004](../adr/0004-references-live-in-a-toml-header.md) — gltree writes the derived lists; a
  person writes the truth.

## Design model
- **Refuse before any write.** Every command needs the repo config (`require_opt_in`, also called by
  `main` before any login), reads its item strictly (`read`: a header out of form is refused, so a write
  never rewrites a block it cannot read whole), and checks every named item exists in one read
  (`require_all`). `new` also refuses a body opening with `+++` and `--from` equal to `--parent`; `move`
  a target that is the item's `from`, the item or below it (walked with `tree::subtree`, so an unlinked
  descendant slips past — `check` reports those); `edit` the item itself, an undeclared `--set-extra`
  key, and a blocker the item already blocks at any depth (`blocked_through`, one `level` per step).
  `new` and `move` ask the state policy ([judging-an-items-state](judging-an-items-state.md)).
- **The first write is the truth**: `new` creates the item with its header; `move` and `edit` rewrite the
  item's own description. Only the keys the command changes move; the rest of the header and the body
  stay byte for byte (`header::with_header`). `move` changes `parent` only; a header left empty by
  `--to-root` goes with its blank line, so `new` then `move --to-root` gives the text back.
- **Then named steps** (`run`): links, unlinks, mirror drops and a `gltree sync M` for every other end.
  A failure is `Error::Unfinished` naming itself and every step after it — the commands still to run —
  and exits 2. The syncs are `derived::sync_one`, the calculation `check` uses, so a write leaves no
  `derived` finding (commit 971f508, 2026-10-22).
- **Unlink only what nothing explains** (`unlink_unexplained`): read fresh, neither end names the other
  under any reference key. Moving a child off a parent it is also related to keeps the link.
- **`edit` removes a related item on both ends**, because `related` syncs as the union of both ends and a
  mirror left on one end would come back; removing a mirror the item only holds as a mirror drops it on
  the other end (`mirror_only`). An edit that changes nothing writes nothing.
- **Labels** are exactly those passed; each `--label` may hold several, separated by commas
  (`split_labels`).
- Measured against a real server: the server stores a description's CRLF as LF; everything else came
  back byte for byte, Korean included (commit 544d3df, 2026-10-22). On a real tracker's snapshot, a
  move, a new item with related and blocked_by, an edit removing both and a move back each left
  `check` at its baseline (commit 971f508).

## Code
- `src/write.rs — Rules, Steps, run, link, sync, unlink_unexplained, sorted, refuse_closed_above, require_all, split_labels, require_opt_in, Refs, New, new_item, new_json, Moved, read, move_item, moved_json, Edits, Edited, blocked_through, edit_item, edited_json, JSON_VERSION, NEW_JSON_VERSION`
- `src/main.rs — Command::New, Command::Edit, Command::Move, parse_extra`
- `src/tracker.rs — FakeTracker (fail_links, fail_unlinks, fail_descs)`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [Every reference has a link](every-reference-has-a-link.md) — every write adds the links its references need and removes only unexplained ones.

## Blast radius
- [keeping-the-derived-lists](keeping-the-derived-lists.md) — every write ends in `sync_one`; a change to the sync changes what each write leaves.
- [drawing-a-tree](drawing-a-tree.md) — a child written without its link disappears from `tree N`.
- [the-command-line-contract](the-command-line-contract.md) — the `Unfinished` message, exit 2 and the `--json` shapes are what callers read after a write.
- [reading-the-server](reading-the-server.md) — the steps rely on REST link calls being safe to repeat.

## Known holes / open
- `refuse_closed_above` and `move`'s cycle walk use `FrontMatter` and `header::parent_of` directly, not the configured scheme; with one scheme this is the same (reading, 2026-11-19).
- Writing a Korean test body with Git Bash `printf` stores replacement characters; write test bodies with a UTF-8-aware tool (commit 544d3df).
- REST `DELETE projects/:id/issues/:iid` answers 403 even to the author on a Maintainer token; `glab work-items delete` (GraphQL) deletes a test item (commit 544d3df).
