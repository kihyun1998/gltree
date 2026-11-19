# Every reference has a link, and every link has a reference

## The fact
For every number under `parent`, `from`, `related` or `blocked_by` in an item's header there is a
`relates_to` link between the two items; and every link between two items is explained by such a
reference on at least one end.

## Why it is cross-cutting
GitLab indexes links, not headers: the only way from an item to the items that name it, without
reading every body, is its links ([ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md)).
Several sites lean on that independently — each reads references through `header::refs_of` or
`parent_of`, but the rule "a link for each, and only for those" is written out at each site, not held
by one function. A site that changes what counts as a reference, or when a link is made or removed,
without the others, leaves items that some command cannot see. Stated in
[ADR-0004](../adr/0004-references-live-in-a-toml-header.md).

## Territories it holds in
- [checking-the-tracker](checking-the-tracker.md) — enforces it: `check_with`'s `unlinked` and `stray-link`.
- [writing-the-tree](writing-the-tree.md) — keeps it on every write: `new_item` links each reference, `move_item` and `edit_item` link the new and call `unlink_unexplained` for the old.
- [migrating-a-tracker](migrating-a-tracker.md) — establishes it: `plan` adds a link for each reference the old tracker lacks.
- [drawing-a-tree](drawing-a-tree.md) — relies on it: `subtree` finds children only among linked items.
- [keeping-the-derived-lists](keeping-the-derived-lists.md) — relies on it: `sync_one` computes N's lists from the items linked to N only.
- [holding-references-in-the-header](holding-references-in-the-header.md) — defines what a reference is: `refs_of`.

## What a violation looks like
A child whose link was not made does not appear in `tree N`, and `sync P` leaves it out of P's
`children`, while `tree` from a snapshot and `sync` over the whole tracker show it — the server and the
file disagree. A link left after its reference went makes `check` report `stray-link`.

## Discovery history
- [ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md): parents and children are linked so a tree can be walked by depth.
- Commit 49a434f (#17, 2026-10-22): extended to `from`, `related` and `blocked_by`, and every link must be explained.
- This map (2026-11-19): found as a fact held at the sites above by reading them.

## Where it will recur
If a function adds a reference key, writes or removes a reference, or walks the tracker along links,
it is subject to this: it must make or remove the link with the reference, or say why it may rely on
links being complete.
