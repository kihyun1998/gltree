# Checking the tracker

How `gltree check` reads every item and reports where the tracker breaks the convention, and how
`snapshot` saves it (`src/check.rs`, `src/header.rs`, `src/config.rs`, `parse_page` in
`src/gitlab.rs`, `src/snapshot.rs`).

## Design model

- Items by number; per item, findings in this order: `header`, `task`, `missing-parent`,
  `unlinked`, `cycle`, then the state policy's finding. Text: `{kind:<26} #N  detail`, then
  `N anomalies`; exit 1 when any.
- `header` is the header read strictly (`header::parse`): a block that never closes, TOML that does
  not parse, an unknown key, a value that is not an issue number, a list out of form (unsorted, a
  number twice, empty), a `[notes]` key that is not a reference key, an `[extra]` key the repo config
  does not declare, and a reference key that names the item itself. The detail is the reason, so
  one finding names one fix.
- The parent is read loosely (`header::parent_of`): any block whose TOML parses and whose `parent`
  is a positive integer. A header `check` refuses for another reason still places the item in the
  tree, so one stray key does not also detach a subtree. A first line `Parent: #N` is no parent.
- `unlinked` needs to know links were recorded: an item's `links` is `None` in a snapshot that
  never recorded them, and then nothing is reported.
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
  and `extra_keys` (bare TOML keys). Unknown keys and unknown values fail, listing the known values,
  so a config still naming the first-line settings (`origin_word`, `links`, `parent-line`) stops
  every command. Absent file: defaults, and `present = false`, which the write commands read as
  "this repo has not opted in".

## Known holes / open

- `root_label` has no reader yet.
- `from`, `related` and `blocked_by` are parsed but not yet checked against the tracker: a missing
  item, a missing link, a link no reference explains, a blocker cycle, and the derived lists.
- A first-line head left in a body is not reported yet.
- A full read takes about 45 s for a few thousand items: the pages are fetched one after another,
  each waiting for the previous page's cursor.

## Traps

- A tracker being written to while it is read gives a torn picture: a child created a moment
  before its link reads as `unlinked` on one pass and clean on the next. Measured when two tools
  read one tracker a minute apart while another session created children.
