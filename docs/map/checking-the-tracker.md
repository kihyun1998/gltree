# Checking the tracker

How `gltree check` reads every item and reports where the tracker breaks the convention, and how
`snapshot` saves it (`src/check.rs`, `src/config.rs`, `parse_page` in `src/gitlab.rs`,
`src/snapshot.rs`).

## Design model

- Items by number; per item, findings in this order, matching the consumers' Python tool:
  `parent-line`, `origin-line`, `task`, `missing-parent`, `missing-origin`, `unlinked` (the
  parent's, then the origin's), `stray-link`, `cycle`, then the state policy's finding. Text:
  `{kind:<26} #N  detail`, then `N anomalies`; exit 1 when any.
- `parent-line`: more than one line that parses as a Parent line, or one that is not the first.
- The origin line (ADR-0003) is read only under the repo's `origin_word`. `origin-line`: more than
  one line that parses as one, or one not in its place (directly under the Parent line, else
  first). An origin counts toward nobody's state: no policy sees it.
- `unlinked` needs to know links were recorded: an item's `links` is `None` in a snapshot that
  never recorded them, and then nothing is reported.
- `stray-link` is gltree's own, and only in a repo whose `links` is `tree-only` (ADR-0001). It is
  reported once per pair, at the lower number. A link between an item and its origin is not
  stray, so a repo that records finds keeps `tree-only` and with it the report of a child that
  lost its Parent line.
- `cycle` follows parents up from each item, so an item whose ancestors loop is reported along
  with the loop's members — as the Python tool does.
- `check` hands every item, with whether it is a root and all its descendants, to the state
  policy, which alone decides; a future policy that judges non-roots needs nothing from `check`.
- State policy `roots-only`: only an item with no parent at all is judged — a root closed with an
  open descendant, or open with every descendant closed. An item under a missing parent is not a
  root. Descendants are walked to any depth.
- State policy `every-parent`: every item with children is judged on its **direct** children —
  closed with an open child, or open with every child closed. A closed grandparent over a closed
  child and an open grandchild is not reported; the child is. This matches the tool of the
  consumer whose `Parent:` means "part of".
- The full read is paged GraphQL (100 items a page) with type, labels and link ends; a node whose
  links overflow one page is an error, since `unlinked` and `stray-link` need every link.
- The repo config `.gltree.toml` sits at `git rev-parse --show-toplevel`: `root_label` (default
  `SPEC`), `relation_scheme` (`parent-line`), `state_policy` (`roots-only`, or `every-parent`), `links` (`tree-only`,
  or `shared` for a repo that also links related issues), and `origin_word`, required, with no
  default: one word, not `Parent`, without spaces, `:` or `#`. Unknown keys and
  unknown values fail, listing the known values. Absent file: defaults, no origin line, and
  `present = false`, which #5's write commands read as "this repo has not opted in".

## Known holes / open

- `root_label` has no reader yet.
- The Python tool's `conflict` (hierarchy parent against the line) is not carried: gltree reads no
  Task hierarchy.
- A full read takes about 45 s for a few thousand items: the pages are fetched one after another,
  each waiting for the previous page's cursor.

## Traps

- A tracker being written to while it is read gives a torn picture: a child created a moment
  before its link reads as `unlinked` on one pass and clean on the next. Measured when two tools
  read one tracker a minute apart while another session created children.
