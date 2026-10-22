# gltree owns the tree convention

The tree rules — where the parent is written, what a root is, when an item may close — were
stated in three places (each consuming repo's `issue-tracker.md` and the shared skills) and
enforced by two drifting copies of a Python tool, so one change meant editing all of them. gltree
holds the convention and is its one implementation; a consuming repo's docs keep a short summary
that points here, and the per-repo choices (root label, relation scheme, state policy) live in
that repo's committed config file.

## Consequences

The Python tools are deleted from the consuming repos once gltree's `tree` and `check` match
their output on the live server. The consumers keep their own state rules through the repo
config (`roots-only`, or `every-parent` where every parent stays open while a child is open). A
find is recorded under `from` rather than `parent` (ADR-0003), so `parent` means "part of"
wherever gltree reads it.
