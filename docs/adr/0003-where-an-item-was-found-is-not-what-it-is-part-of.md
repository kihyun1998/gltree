# Where an item was found is not what it is part of

`parent` means "part of": both state policies count a child toward its parent's state. Trackers also
used it for a second relation, the item a find turned up while working — a bug found in a slice hung
under that slice. The two pull apart: a finished feature cannot close over an open find, so a repo
either reopens features for work that is not theirs, or writes an exception into its policy ("a
slice closes with follow-ups open beneath it").

gltree records the second relation apart: the header's `from` (ADR-0004). The relations are
independent; an item may carry both (part of one feature, found while working another), but never
the same number in both. A `from` counts toward nobody's state.

Because `parent` now means only "part of", an open item may not go under a closed parent where the
policy would then report it: `new` and `move` refuse before writing and offer `from` instead.

## Considered Options

- **A line of its own under the Parent line, its word set by each repo.** Built first, then
  replaced by the header when four such lines began to collide (ADR-0004).
- **Drawing finds in `tree`.** Left out: a find is not part of the tree it came from; `roots --json`
  carries `from` for callers that want the relation.

## Consequences

Finds already hung under a slice in the first-line format stay where they are unless a consumer
moves them; `check` judges them by the repo's state policy as before.
