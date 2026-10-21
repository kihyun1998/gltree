# Writing the tree

How gltree adds to a tracker without leaving a child its link (`src/write.rs`, the write half of
the tracker port, `create_item` / `add_link` in `src/gitlab.rs`).

## Design model

- `new` refuses before touching the tracker when the repo has no repo config
  (`require_opt_in`, shared by the core and the CLI so the refusal comes before any login), when
  the body already opens with a Parent line (it would be a second one), or when the parent does
  not exist (one `level` read).
- Then, in this order: create the item with the body `Parent: #P`, a blank line, the file's text
  (`Parent: #P` alone for an empty file; the text alone for a root), then link P and the item.
  A link failure after the create is `Error::Unfinished`: it names the new item and the link left
  to add, and exits 2.
- Labels are exactly those passed; each `--label` may hold several, separated by commas.
- Writes are REST under `api/v4/projects/<url-encoded path>`: `POST issues`, then
  `POST issues/<P>/links` with `link_type=relates_to`; 409 (already linked) counts as success.
  A create is judged by its status, never by whether the body carries an `iid`.

## Traps

- The server stores a description's CRLF as LF: a body sent with `\r\n` reads back with `\n`.
  Everything else came back byte for byte, Korean included (measured with a UTF-8 file).
- Writing a Korean test body with Git Bash `printf` stores replacement characters; write test
  bodies with a UTF-8-aware tool.
- REST `DELETE projects/:id/issues/:iid` answers 403 even to the issue's author on a Maintainer
  token; `glab work-items delete <iid>` (GraphQL) deletes it.
