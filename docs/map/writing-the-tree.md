# Writing the tree

How gltree adds to a tracker without leaving a child its link (`src/write.rs`, the write half of
the tracker port, `create_item` / `add_link` in `src/gitlab.rs`).

## Design model

- `new` refuses before touching the tracker when the repo has no repo config
  (`require_opt_in`, shared by the core and the CLI so the refusal comes before any login), when
  the body already opens with `+++` (it would be a second header), when `--from` names the parent,
  or when the parent or `--from` item does not exist (one `level` read for both).
- `new --parent P` and `move --to P` refuse to hang an open item under P when the state policy
  would then report P, or an item above it, closed over an open one (`refuse_closed_above`: the
  policy itself judges P and each ancestor with the new item as a child of P and a descendant of
  all, one `level` per ancestor). The refusal names both ways out: reopen it, or record P as the
  item's `from`. Under `roots-only` a closed slice still takes a child; a closed root above it
  does not.
- Then, in this order: create the item with the header (`parent`, `from`, each only when given), a
  blank line and the file's text — the header alone for an empty file, the text alone with neither
  flag — then link P, then F. A link failure after the create is `Error::Unfinished`: it names the
  new item and the links left to add, and exits 2.
- Labels are exactly those passed; each `--label` may hold several, separated by commas.
- Writes are REST under `api/v4/projects/<url-encoded path>`: `POST issues`, then
  `POST issues/<P>/links` with `link_type=relates_to`; 409 (already linked) counts as success.
  A create is judged by its status, never by whether the body carries an `iid`.
- `move` reads the item (one `level`), and does nothing when it already has the target parent.
  It refuses, before any write, a header out of form (read strictly, so `move` never rewrites a
  block it cannot read whole), a parent that is the item's `from`, the item or below it — found by
  walking the item's subtree along links, so an unlinked descendant would slip past (`check`
  reports those) — and a parent that does not exist. Then: the body first (the truth), the new
  link, the old link removed, and `children` synced on the old and the new parent. A failure after
  the body names only the steps still left.
- The body (`header::with_header`): only `parent` changes, and the block is written back in the
  fixed key order; the body after the closing fence stays byte for byte. A root gains a block and a
  blank line, as `new` writes them; a header left empty by `--to-root` goes with the one blank line
  under it, so `new` then `move --to-root` gives the text back. `move` never changes `from`.
- `remove_link` finds the link's id in the item's links answer and deletes it; no link is success.
- After the first write, `new`, `move` and `edit` run their remaining writes as named steps
  (`write::run`): links, unlinks, mirror drops and a `gltree sync M` for every other end whose
  derived lists change. A failure names itself and every step after it, so the message is the
  list of commands still to run. The other ends are synced through `derived::sync_one`, the same
  calculation `check` and `sync` use, so a write never leaves a `derived` finding behind.
- An unlink happens only when, read fresh, neither end still names the other under any reference
  key (`unlink_unexplained`): moving a child off a parent it is also related to keeps the link.
- A write that empties a key drops that key's note (`Header::drop_orphan_notes`), since a note
  for an absent key is out of form.
- `edit` removes a related item on both ends, because `related` is synced as the union of both
  ends and a mirror left on one end would come back. A blocker it adds must not already be blocked
  by the item at any depth (`blocked_through`, one `level` per step).
- `sync N` (`src/derived.rs`) reads one `level`: N and the items linked to it, which are every item
  that can name N, since a reference without its link is itself a finding. `sync` with no item
  reads the whole tracker once and indexes who names whom. Both rewrite only items whose lists
  change, through `with_header`, so the truth, `[notes]`, `[extra]` and the body stay as they are;
  a header out of form is refused, since rewriting it would drop what it cannot read.
- `related` is synced to the union of both ends: a mirror is added, never taken away, so removing
  a related item is an edit on both ends, not a sync.

## Traps

- The server stores a description's CRLF as LF: a body sent with `\r\n` reads back with `\n`.
  Everything else came back byte for byte, Korean included (measured with a UTF-8 file).
- Writing a Korean test body with Git Bash `printf` stores replacement characters; write test
  bodies with a UTF-8-aware tool.
- REST `DELETE projects/:id/issues/:iid` answers 403 even to the issue's author on a Maintainer
  token; `glab work-items delete <iid>` (GraphQL) deletes it.
