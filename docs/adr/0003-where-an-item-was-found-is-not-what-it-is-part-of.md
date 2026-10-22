# Where an item was found is not what it is part of

A Parent line means "part of": both state policies count a child toward its parent's state. Trackers
also used it for a second relation, the item a find turned up while working — a bug found in a
slice hung under that slice. The two pull apart: a finished feature cannot close over an open find,
so a repo either reopens features for work that is not theirs, or writes an exception into its
policy ("a slice closes with follow-ups open beneath it"). A repo that recorded finds another way —
its own first line naming the origin, plus a link — had to share links, and so lost `stray-link`.

gltree records the second relation in its own line: `<word>: #N`, optionally followed by free text,
directly under the Parent line or else first. The relations are independent; an item may carry
both (part of one feature, found while working another). An origin counts toward nobody's state.
Its link is not stray. The word is each repo's, required in the repo config, so a repo writing in
another language keeps its own word and no default decides it silently.

Because the Parent line now means only "part of", an open item may not go under a closed parent
where the policy would then report it: `new` and `move` refuse before writing and offer the origin
line instead.

## Considered Options

- **A fixed word for every repo.** Rejected: a consumer already had origin lines in its own
  language, and a word baked into gltree reads as a foreign token in those trackers.
- **A default word, overridable.** Rejected by the maintainer: a repo should say which word it
  uses rather than inherit one by omission.
- **Drawing origins in `tree`.** Left out: a find is not part of the tree it came from; `roots`
  carries `origin` for callers that want the relation.

## Consequences

A repo config without `origin_word` is refused, and a gltree older than this refuses the key, so a
consumer upgrades and commits the key together. Finds already hung under a slice stay as they are;
`check` judges them by the repo's state policy as before.
