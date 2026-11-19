# The command-line contract

## What it is
What a caller can rely on when it runs the gltree binary: the commands and flags, the exit status,
the text lines and the versioned JSON shapes, and the repo config a consumer commits. gltree is used
as a binary; its callers are consuming repositories and outside skills that parse its output.

## Governing decisions
**None.** [ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md) makes gltree the convention's one
implementation, which is why its output is what callers read; it does not decide the output.

## Design model
- **Exit status**: 0 on success; 1 when `check` finds anything; 2 on a usage, input, config or server
  error, and on a write left `Unfinished` (`main`, `run`).
- **The source**: every read command takes `--snapshot F` instead of the server (`source`); `tree N`
  without it walks, `tree` without a number reads every item. `serve` takes it too, and its refresh reads the file again.
- **`tree` without a number leaves out families whose every item is closed**; `--all` draws every
  family and is a usage error (exit 2) beside a number. Leaving families out gives fewer `roots`, not
  another shape; `tree N --json`, which outside skills read, draws as before (#25).
- **JSON shapes carry a version**, raised when the shape changes — a field added, removed or renamed; fewer items is not a shape change (#25): `tree` 3 (`url`, `labels` and `extra` on every node, #26), `check` 1,
  `roots` 2, `new` 2, `move` / `edit` 1, `sync` 1, `migrate` 1, `show` 1, `serve`'s API 1 (each module's `JSON_VERSION`,
  `NEW_JSON_VERSION`). The shapes are in the README and in each owning note.
- **`serve`** listens on 127.0.0.1 only, port 7420 unless `--port` (0 takes a free one), and prints
  `http://127.0.0.1:<port>/` once the first full read is done; a config or read error before that exits
  2 with no address printed. Its four routes and their JSON are in
  [serving-the-viewer](serving-the-viewer.md).
- **Item numbers** are accepted as `N` or `#N` (`parse_iid`).
- **What callers are known to read** (read in the callers, 2026-11-19; not named here, maintainer,
  2026-11-19): consuming repositories commit `.gltree.toml` with `state_policy` and `extra_keys` and
  state a minimum gltree version beside it; their docs tell people to create and change items only
  through `new`, `edit`, `move` and `sync`. Outside skills run `roots --json` with
  `--closed-since` / `--closed-until` / `--without-label` and read `extra` for a registration marker,
  run `tree N --json` for every descendant's number, title and state, and write the marker with
  `edit --set-extra`.

## Code
- `src/main.rs — Cli, Command, parse_iid, parse_extra, server, source, main, run`
- `tests/cli.rs`
- `README.md` (the user-facing shapes)

## Reference behaviour
**None.**

## Cross-cutting invariants
**None.**

## Blast radius
- [listing-roots](listing-roots.md) — `extra`, the label filters and the date filters are read outside; renaming one breaks a caller with no test here failing.
- [drawing-a-tree](drawing-a-tree.md) — `tree --json` fields are read outside.
- [writing-the-tree](writing-the-tree.md) — `edit --set-extra` is how outside skills write their marker.
- [reading-the-repo-config](reading-the-repo-config.md) — a config key added means consumers raise their minimum version.
- [serving-the-viewer](serving-the-viewer.md) — the API's JSON is part of the contract: `/api/families` embeds `tree --all --json`.
- [shipping-a-release](shipping-a-release.md) — a changed shape reaches callers only through a release, and the version they install.

## Known holes / open
- Shapes are tested (`show_as_json_carries_the_same_fields` in `tests/cli.rs`; `json_carries_from_and_extra_and_the_first_line_after_the_header` in `src/roots.rs`), but a test is edited along with a rename: nothing here marks which fields are read outside, besides this note.
