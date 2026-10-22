# Listing roots

How `gltree roots` lists the tracker's roots for tools that act on finished features
(`src/roots.rs`, `closed_at` / `url` in `parse_page` of `src/gitlab.rs`).

## Design model

- A root is an item with no `parent`, as the relation scheme reads it. An item whose `parent`
  names a deleted item is not a root here; `tree` draws it as one so it is not lost, and
  `check` reports it as `missing-parent`.
- Filters combine with AND: state; `--label`, every one present; `--without-label`, none present,
  a trailing `*` matching a prefix (`nit::*` drops `nit::registered`, keeps `nit` and `xnit::y`);
  `--closed-since` / `--closed-until` on the UTC date of `closed_at` (its first ten characters),
  inclusive at both ends, implying closed.
- The JSON (version 2) carries `from` and `extra` from the header, so a caller reads its own
  marker (a registration key, say) from `extra` without fetching the body, and `first_line` — the
  body's first line after the header and its blank lines, without a trailing CR. `from` is null
  for a root that was not found while working another item; a find with no parent is a root.
- The header is read loosely here (`header::loose`): every `[extra]` key is taken whatever the
  repo config declares, and a header out of form yields no `from` or `extra` rather than an error,
  since listing is not checking.
- gltree knows no external tracker: label names and what to do with the roots are the caller's.
  The first callers are a weekly report and an unregistered-feature sweep.

## Known holes / open

- A date filter over a closed item without `closed_at` (a snapshot taken before #12) is an error
  naming the field, not an empty list.
- A full read takes about 50 s on a tracker of a few thousand items; `--snapshot` reuses one.
- `wontfix` and other closing reasons are not filtered unless the caller asks
  (`--without-label wontfix`).
