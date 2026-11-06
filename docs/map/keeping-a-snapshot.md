# Keeping a snapshot

How `gltree snapshot` saves the tracker so it can be read while the server is unreachable, and how
`gltree show` prints one item from it or from the server (`src/snapshot.rs`, `src/show.rs`,
`read_items_with_comments` and `read_item_with_comments` in `src/gitlab.rs`).

## Design model

- **Comments have their own read.** `items_with_comments` and `item_with_comments` ask for them;
  `all_items` and `level` do not, so `tree`, `check`, `roots`, `migrate` and `sync` send the requests
  they always sent.
- **The query.** Comments travel in the page query as `WorkItemWidgetNotes.notes(first:100,
  filter:ONLY_COMMENTS)`. An item with more than one page is followed with one request per further
  page, by its number and the cursor. A failed page, a page that says there is more but gives no
  cursor, and a further page without the comments widget all fail the whole read, so a short list
  does not reach the file. `show` from the server reads one item with its own query, since `level`
  brings no labels; it does not ask for links, which it does not print, so more than 100 of them
  does not stop it.
- **The floor is GitLab 17.11.** `WorkItemWidgetNotes.notes` is in `notes_type.rb` at v17.11.0-ee, and
  not at v17.3.0-ee or v17.10.0-ee, which have `discussions` only. The maintainer's call on #24,
  reversible: flat `notes` over `discussions` → `notes`, which would reach further back at the cost of
  paging two levels. It was decided knowing that floor.
- **What is kept.** System notes are dropped by their `system` flag as well as the filter, and the
  list is ordered by creation time after every page is in. Internal comments are kept and marked:
  the maintainer's call on #24, reversible. A person reading during an outage needs them, and the mark
  tells them not to pass the comment on.
- **"Not recorded" is not "none".** `Item.comments` is `None` for a file older than comments, as
  `links` and `closed_at` already are, and for a work item the server gives no comments widget; `show`
  says so instead of printing "no comments". A file `migrate --out` wrote from the server holds no
  comments, even over a file that did: that command reads none.
- **`show`'s text cannot be forged by what it prints.** gltree's own lines sit at the margin and what
  people wrote is indented, with control characters taken out, so a body cannot pass for a comment head
  and its `[internal]` mark, and cannot drive the terminal. `--json` keeps the text as written. `body`
  keeps the header, since during an outage it holds the references. `tree` and `roots` still print
  titles raw.
- **Replacing the file.** `replace_file` writes a temporary file beside the target, syncs it, then
  renames it over the target. `snapshot` and `migrate --out` both go through it (the maintainer's
  call on #24 for `migrate --out`). A read-only target is refused, as `std::fs::write` refused it. On
  Windows the rename refuses it even without the check (measured by removing it); on Unix a rename
  depends on the directory's permissions and not the file's, so there the check should be what refuses.
  The test passes on Ubuntu, but removing the check has not been tried there. On Unix a new file is
  `0600` and a replaced one keeps its mode; those tests run on Unix only, in the release workflow's
  gate.
- **Windows access is the folder's.** A replaced file takes its folder's inherited access: `Permissions`
  carries only the read-only flag, so an ACL narrowed on the file is lost (measured: one user before,
  five entries after). The maintainer's call on #24: documented in the README, not kept. Keeping it
  would take `ReplaceFileW`, through FFI or a dependency.

## Measured

- On a self-hosted CE, the page query with the notes fragment was accepted, with no complexity error
  (the server does not report the value on success). On gitlab.com the same shape, with `discussions`
  and `notes` both at 100, was refused to an anonymous caller at a complexity of 233 against its limit
  of 200.
- A full `snapshot` took about 1.4 times the full read `check` does, in one run each. One page
  measured alone took about twice as long with comments, so a page is not a fair estimate of the
  whole.
- On a real tracker, one run: `check --snapshot` printed what `check` printed from the server, and
  `show` printed the same bytes from the file and from the server, in text and in `--json`.
- Following the cursor was run against the server by walking one item's comments at `notes(first:1)`
  in the shape `comments_query` sends: the walk gave the same comments as the file.
- In one request, the flat `notes` field and `discussions` → `notes` returned the same comments item
  by item, oldest first with no system notes. This holds for that sample; the code does not rely on
  it (see above).

## Known holes / open

- `show` from the server stopped asking for links after the real-tracker run above. That query's shape
  was accepted by gitlab.com, written out by hand; it has not run on a self-hosted server.
- No item with more than one page of comments and no internal comment has been seen on a real server:
  the tracker read had none. `read_items_with_comments` following a real overflow and the shape of a
  real internal comment are tested over fixture answers only.
- A crash between creating the temporary file and renaming it leaves `.<name>.<pid>.tmp` beside the
  target. The target itself is intact.
- A target that is a symbolic link is replaced by a regular file, not written through.
- On Unix, a target owned by someone else in a writable directory is replaced, where a write into it
  would have been refused: `readonly()` reads the mode bits only. Not run.
- A device as the target (`--out NUL`) now fails: the rename cannot go over it.
- `created_at` is ordered as text. The server's timestamps read as UTC in the tracker measured; a zone
  that changed partway through would order by the text.
- Every snapshot reads every comment: an item's update time is not shown to be a safe filter for them.
