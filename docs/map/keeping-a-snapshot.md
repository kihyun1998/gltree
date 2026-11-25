# Keeping a snapshot

## What it is
`gltree snapshot`: every item of the tracker, with links and comments, saved to a JSON file that every
read command takes as `--snapshot`, so the tracker can be read while the server is unreachable; and
the safe replace that `snapshot` and `migrate --out` share.

## Governing decisions
**None.** The calls below are the maintainer's on #24.

## Design model
- **The file** is a JSON list of `Item` by number (`SnapshotFile::to_bytes`): `iid`, `title`, `state`,
  `type`, `desc`, `labels`, and optionally `links`, `closed_at`, `url`, `comments`. Unknown fields are
  ignored and missing optional ones read as not recorded, so older files still read
  ([not recorded is not none](not-recorded-is-not-none.md)). A snapshot tracker is read only: every
  write refuses.
- **Comments have their own read.** `items_with_comments` and `item_with_comments` ask for them;
  `all_items` and `level` do not, so `tree`, `check`, `roots`, `migrate` and `sync` send the requests
  they always sent. Comments travel in the page query as
  `WorkItemWidgetNotes.notes(first:100, filter:ONLY_COMMENTS)`; an item with more is followed one
  request per page by number and cursor. A failed page, a page that says there is more but gives no
  cursor, and a further page without the widget fail the whole read, so a short list never reaches
  the file. Source: `read_items_with_comments`, `all_comments`, `parse_comments`.
- **The floor is GitLab 17.11**: `WorkItemWidgetNotes.notes` is in `notes_type.rb` at v17.11.0-ee and
  not at v17.3.0-ee or v17.10.0-ee, which have `discussions` only. The maintainer's call on #24,
  reversible: flat `notes` over `discussions` → `notes`, which would reach further back at the cost of
  paging two levels (commit 4cec8b2, 2026-11-06).
- **What is kept**: system notes are dropped by their `system` flag as well as the filter; the list is
  ordered by `created_at` after every page is in. Internal comments are kept and marked — the
  maintainer's call on #24, reversible: a person reading during an outage needs them, and the mark
  tells them not to pass the comment on (commit 4cec8b2).
- **Replacing the file** (`replace_file`): a temporary `.<name>.<pid>.tmp` beside the target, filled,
  synced, renamed over it; on any failure the temporary file is removed and the target is as it was.
  A read-only target is refused. A replaced file keeps the old file's permissions; a new one is `0600`
  on Unix. `migrate --out` goes through it too — the maintainer's call on #24.
- **Windows access is the folder's**: `Permissions` carries only the read-only flag, so an ACL narrowed
  on the file is lost on replace (measured: one user before, five entries after). Documented in the README,
  not kept — the maintainer's call on #24; keeping it would take `ReplaceFileW` (commit 4cec8b2).

## Code
- `src/snapshot.rs — SnapshotFile, SnapshotFile::open, SnapshotFile::to_bytes, SnapshotFile::parse, SnapshotFile::save, replace_file, Tracker for SnapshotFile`
- `src/gitlab.rs — GitLab::items_with_comments, read_items_with_comments, read_item_with_comments, all_comments, parse_comments, notes_fragment, page_query_with_comments, item_query, comments_query`
- `src/item.rs — Item, Comment`
- `src/main.rs — Command::Snapshot`

## Reference behaviour
- GitLab's `notes_type.rb` at v17.3.0-ee, v17.10.0-ee and v17.11.0-ee, for the floor (commit 4cec8b2).
- On gitlab.com the page query shape with `discussions` and `notes` both at 100 was refused to an
  anonymous caller at complexity 233 against 200; on a self-hosted CE the shipped shape was accepted
  (commit 4cec8b2, 2026-11-06).

## Cross-cutting invariants
- [Not recorded is not none](not-recorded-is-not-none.md) — `comments`, `links`, `closed_at`.
- [An error names what it read](an-error-names-what-it-read.md) — only an error about the snapshot file names the snapshot as what was read.

## Blast radius
- [showing-an-item](showing-an-item.md) — `show --snapshot` prints what this file holds; a comment field not saved cannot be shown.
- [listing-roots](listing-roots.md) — date filters need `closed_at` in the file.
- [checking-the-tracker](checking-the-tracker.md) — the link checks need `links` in the file.
- [migrating-a-tracker](migrating-a-tracker.md) — `--out` writes through `replace_file` and, read from the server, holds no comments.

## Known holes / open
- A full `snapshot` took about 1.4 times `check`'s full read, one run each (commit 4cec8b2).
- No item with more than one page of comments, and no internal comment, has been seen on a real server; both are tested over fixture answers only (commit 4cec8b2).
- A crash between create and rename leaves `.<name>.<pid>.tmp`; the target is intact.
- A symbolic-link target is replaced by a regular file, not written through.
- On Unix, a target owned by someone else in a writable directory is replaced: `readonly()` reads the mode bits only. Not run. The read-only refusal was measured on Windows only (commit 4cec8b2).
- A device as the target (`--out NUL`) fails: the rename cannot go over it.
- `created_at` is ordered as text; a time zone that changed partway would order by the text.
- Every snapshot reads every comment: an item's update time is not shown to be a safe filter.
