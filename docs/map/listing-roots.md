# Listing roots

How `gltree roots` lists the tracker's roots for tools that act on finished features
(`src/roots.rs`, `closed_at` / `url` in `parse_page` of `src/gitlab.rs`).

## Design model

- A root is an item with no Parent line, as the relation scheme reads it. An item whose Parent
  line names a deleted item is not a root here; `tree` draws it as one so it is not lost, and
  `check` reports it as `missing-parent`.
- Filters combine with AND: state; `--label`, every one present; `--without-label`, none present,
  a trailing `*` matching a prefix (`nit::*` drops `nit::registered`, keeps `nit` and `xnit::y`);
  `--closed-since` / `--closed-until` on the UTC date of `closed_at` (its first ten characters),
  inclusive at both ends, implying closed.
- The output carries `first_line` without a trailing CR, so a caller can read its own marker on
  that line (a registration line, say) without fetching the body. `origin` is the number the
  root's origin line names (ADR-0003), or null: a find that stands alone is a root of its own.
- gltree knows no external tracker: label names and what to do with the roots are the caller's.
  The first callers are a weekly report and an unregistered-feature sweep.

## Known holes / open

- A date filter over a closed item without `closed_at` (a snapshot taken before #12) is an error
  naming the field, not an empty list.
- A full read takes about 50 s on a tracker of a few thousand items; `--snapshot` reuses one.
- `wontfix` and other closing reasons are not filtered unless the caller asks
  (`--without-label wontfix`).
