# Checking the tracker

How `gltree check` reads every item and reports where the tracker breaks the convention, and how
`snapshot` saves it (`src/check.rs`, `src/header.rs`, `src/config.rs`, `parse_page` in
`src/gitlab.rs`, `src/snapshot.rs`).

## Design model

- Items by number; per item, findings in this order: `header`, `old-header`, `task`,
  `missing-parent`, `missing-reference`, `unlinked` (the parent's, then the others' in key order),
  `stray-link`, `derived`, `blocker-cycle`, `cycle`, then the state policy's finding. Text:
  `{kind:<26} #N  detail`, then `N anomalies`; exit 1 when any.
- `header` is the header read strictly (`header::parse`): a block that never closes, TOML that does
  not parse, an unknown key, a value that is not an issue number, a list out of form (unsorted, a
  number twice, empty), a `[notes]` key that is not a reference key or names a key the header does
  not hold, an `[extra]` key the repo config does not declare, `from` equal to `parent`, and a
  reference key that names the item itself. The detail is the reason, so one finding names one fix.
- References are read loosely (`header::parent_of`, `header::refs_of`): a value in form counts
  even beside a key `check` refuses, so one stray key does not also detach a subtree or turn every
  link stray. A first line `Parent: #N` is no parent.
- `old-header`: the body's first non-blank line is still a first-line head (`header::old_head`) — a
  key of the old format (`Parent`, `From`, `Related`, `Blocked by`, `Depends on`, plus the repo's
  `old_head_keys`), `: #`, a number, then nothing but a list, a reason in brackets or punctuation.
  `Parent: #1 is quoted here` is prose and passes; so does a head below a heading. The keys are a
  list, not a shape, because a body can open with a line like `Fixed with: #2 (…) + #3 (…)` —
  the same shape as `Blocked by: #2550 (…), #2551.`, but prose that no migration would move.
- `missing-reference` and `unlinked` cover `from`, `related` and `blocked_by` as `missing-parent`
  and `unlinked` cover `parent`. `unlinked` needs to know links were recorded: an item's `links` is
  `None` in a snapshot that never recorded them, and then nothing is reported.
- `stray-link`: a link that no reference on either end explains (ADR-0004), reported once per pair
  at the lower number. With related items holding a key, every repo checks every link, so a child
  that lost its `parent` but kept its link is caught.
- `blocker-cycle`: following `blocked_by` from an item comes back to it; each member is reported.
- `derived`: a list gltree keeps (`children`, `blocks`, `related`) that the truth on the other end
  does not give, one finding per list, with both lists and the `sync` that rewrites it
  (`src/derived.rs`, shared with `sync` so the two never disagree). `related` is judged as the
  union of both ends, so only a missing mirror is reported. A header `check` refuses is not judged.
- `cycle` follows parents up from each item, so an item whose ancestors loop is reported along
  with the loop's members.
- `check` hands every item, with whether it is a root and all its descendants, to the state
  policy, which alone decides; a future policy that judges non-roots needs nothing from `check`.
- State policy `roots-only`: only an item with no parent at all is judged — a root closed with an
  open descendant, or open with every descendant closed. An item under a missing parent is not a
  root. Descendants are walked to any depth.
- State policy `every-parent`: every item with children is judged on its **direct** children —
  closed with an open child, or open with every child closed. A closed grandparent over a closed
  child and an open grandchild is not reported; the child is. This suits a consumer whose `parent`
  means "part of".
- The full read is paged GraphQL (100 items a page) with type, labels and link ends; a node whose
  links overflow one page is an error, since `unlinked` needs every link.
- The repo config `.gltree.toml` sits at `git rev-parse --show-toplevel`: `root_label` (default
  `SPEC`), `relation_scheme` (`front-matter`), `state_policy` (`roots-only`, or `every-parent`),
  `extra_keys` (bare TOML keys) and `old_head_keys`. Unknown keys and unknown values fail, listing the known values,
  so a config still naming the first-line settings (`origin_word`, `links`, `parent-line`) stops
  every command. Absent file: defaults, and `present = false`, which the write commands read as
  "this repo has not opted in".

## Known holes / open

- `root_label` has no reader yet.
- `old-header` knows only the listed keys: a repo whose old format used another word reports it
  only once that word is in `old_head_keys`.
- A full read takes about 45 s for a few thousand items: the pages are fetched one after another,
  each waiting for the previous page's cursor.

## Traps

- A tracker being written to while it is read gives a torn picture: a child created a moment
  before its link reads as `unlinked` on one pass and clean on the next. Measured when two tools
  read one tracker a minute apart while another session created children.
