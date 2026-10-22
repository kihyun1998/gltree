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
config: one where a follow-up hangs under the slice it came from checks roots only
(`roots-only`); one where `Parent:` means "part of" and a follow-up carries no Parent line keeps
every parent open while a child is open (`every-parent`). An earlier plan to move the second to
`roots-only` was made before it settled on "part of", and was dropped (#13).
