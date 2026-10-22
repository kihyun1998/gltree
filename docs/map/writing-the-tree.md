# Writing the tree

How gltree adds to a tracker without leaving a child its link (`src/write.rs`, the write half of
the tracker port, `create_item` / `add_link` in `src/gitlab.rs`).

## Design model

- `new` refuses before touching the tracker when the repo has no repo config
  (`require_opt_in`, shared by the core and the CLI so the refusal comes before any login), when
  the body already opens with a Parent or origin line (it would be a second one), or when the
  parent or `--from` item does not exist (one `level` read for both).
- `new --parent P` and `move --to P` refuse to hang an open item under P when the state policy
  would then report P, or an item above it, closed over an open one (`refuse_closed_above`: the
  policy itself judges P and each ancestor with the new item as a child of P and a descendant of
  all, one `level` per ancestor). The refusal names both ways out: reopen it, or record P as the
  item's origin. Under `roots-only` a closed slice still takes a child; a closed root above it
  does not.
- Then, in this order: create the item with the body `Parent: #P`, `<word>: #F`, a blank line,
  the file's text (each head line only when given; the head alone for an empty file; the text
  alone for a root with no origin), then link P, then F (once when F is P). A link failure after
  the create is `Error::Unfinished`: it names the new item and the links left to add, and exits 2.
- Labels are exactly those passed; each `--label` may hold several, separated by commas.
- Writes are REST under `api/v4/projects/<url-encoded path>`: `POST issues`, then
  `POST issues/<P>/links` with `link_type=relates_to`; 409 (already linked) counts as success.
  A create is judged by its status, never by whether the body carries an `iid`.
- `move` reads the item (one `level`), and does nothing when it already has the target parent.
  It refuses, before any write, a parent that is the item or below it — found by walking the
  item's subtree along links, so an unlinked descendant would slip past (`check` reports those) —
  and a parent that does not exist. Then: the body first (the truth), the new link, the old link
  removed. A failure after the body names only the steps still left.
- The body: a child's first line is replaced, keeping its CR if it had one; a root gains
  `Parent: #P` and a blank line, as `new` writes it — or, when it opens with an origin line, the
  Parent line alone above it, in the origin line's line ending; `--to-root` drops the Parent line
  and the one blank line under it (none when the origin line follows), so `new` then
  `move --to-root` gives the text back. `move` never changes an origin.
- `remove_link` finds the link's id in the item's links answer and deletes it; no link is success.

## Traps

- The server stores a description's CRLF as LF: a body sent with `\r\n` reads back with `\n`.
  Everything else came back byte for byte, Korean included (measured with a UTF-8 file).
- Writing a Korean test body with Git Bash `printf` stores replacement characters; write test
  bodies with a UTF-8-aware tool.
- REST `DELETE projects/:id/issues/:iid` answers 403 even to the issue's author on a Maintainer
  token; `glab work-items delete <iid>` (GraphQL) deletes it.
