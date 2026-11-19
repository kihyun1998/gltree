# Not recorded is not none

## The fact
An item's `links`, `comments` and `closed_at` are `None` when the source did not record them, and an
empty list (or an open item's absent date) only when it did; nothing reads `None` as "there are none".

## Why it is cross-cutting
`Item` carries the distinction as `Option` (`src/item.rs`), but the type does not enforce the meaning:
each site that fills an item decides whether it recorded the field, and each site that reads one
decides what `None` means there. They do not call each other. A reader that treats `None` as empty, or
a source that writes `Some(vec![])` for a field it never asked for, makes a command report a falsehood
— every link stray, or "no comments" for an item that has them.

## Territories it holds in
- [reading-the-server](reading-the-server.md) — `parse_page` sets `links` to `Some` (it asked); `brief` leaves them `None`; `read_item_with_comments` sets them `None`, since it does not ask.
- [keeping-a-snapshot](keeping-a-snapshot.md) — `parse_comments` returns `None` without the comments widget; the file omits `None` fields and an older file reads them as `None`.
- [checking-the-tracker](checking-the-tracker.md) — `unlinked` and `stray-link` run only `if let Some(links)`.
- [migrating-a-tracker](migrating-a-tracker.md) — `plan` plans a link only where `old.links.is_some()`.
- [showing-an-item](showing-an-item.md) — `render` prints "comments not recorded" for `None` and "no comments" for `Some([])`.
- [listing-roots](listing-roots.md) — a date filter over a closed item without `closed_at` is an error, not a skip.

## What a violation looks like
`check --snapshot` over a file without links reports every reference as `unlinked`; `show` says "no
comments" for a file taken before comments were saved; a weekly `roots` query silently drops items
closed in its range.

## Discovery history
- Commit 6be32e0 (#4, 2026-10-22): `unlinked` only where links were recorded.
- Commit e664d18 (#12, 2026-10-22): a date filter refuses an item without `closed_at`.
- Commit 4cec8b2 (#24, 2026-11-06): `comments` follows the same rule; the old map note on snapshots named it.
- This map (2026-11-19): written as one fact across these sites.

## Where it will recur
If a function adds an optional field to `Item`, fills an item from a new query, or reads an optional
field, it is subject to this: say whether the source asked for it, and give `None` its own outcome.
