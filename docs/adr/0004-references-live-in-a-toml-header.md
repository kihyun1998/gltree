# References live in a TOML header at the top of the body

Four things came to want an item's first line: the parent, where it was found, what blocks it, and
a consumer's own marker. Each rule fixed a position ("line 1", "under the Parent line"), so one more
line pushed another out of place — a consumer's marker moved seven origin lines to line 3 in one
afternoon, and `check` reported each. Free text after the numbers (`Blocked by: #2550 (the state),
#2551.`) could not be checked at all, and nothing on a parent showed its children.

The description opens with a TOML block between `+++` lines, which GitLab renders as a box above the
body. An item with no references has none.

```
+++
parent = 2527
from = 2495
children = [2528, 2529]
related = [2530]
blocked_by = [2541]
blocks = [2560]

[notes]
from = "found while reproducing over SSH"

[extra]
marker = "…"
+++
```

- **Values.** Numbers are integers without `#`. Lists are arrays, ascending, with no number twice;
  an empty list is left out. A key gltree does not know is refused.
- **What a person writes:** `parent` (part of), `from` (found while working, not part of — ADR-0003),
  `related` (see also), `blocked_by` (cannot start until these close), and `[notes]`, one string per
  reference key.
- **What gltree writes:** `children` (the items whose `parent` is this), `blocks` (whose `blocked_by`
  lists this), and the `related` mirror on the other end. The truth stays on the writing end; a
  derived list that disagrees is reported and rebuilt, so two writers editing one parent at once lose
  nothing for good.
- **`[extra]`** holds keys the repo config declares, kept as written and never read, so a consumer's
  marker has a place that collides with nothing.
- **Links.** Every number under `parent`, `from`, `related` and `blocked_by` has a `relates_to` link,
  and every link is explained by one of them, in every repo. This retires the `links` setting of
  ADR-0001: once related issues have a key, no link needs to go unexplained.
- **Part of, or only found there.** If #P can be called done without this work, it is `from`; if #P
  is not done until it is, it is `parent`. Judged by #P's goal, not by a shared subject.

gltree reads this format only. A tracker in the first-line format is turned into headers by
`migrate`, which alone still reads the old lines.

## Considered Options

- **Keep the first lines, unordered: a block of `Key: value` lines up to the first blank line.** No
  migration, but the boundary is a blank line anyone can insert, and the grammar of each value —
  where a reason starts, how a list is written — would be gltree's own to define and defend.
- **A Markdown list between HTML-comment fences.** Renders cleanly with clickable numbers (measured
  on the server's renderer), but again a grammar of our own.
- **JSON or YAML.** JSON has no comments and is hard to edit by hand; YAML has indentation and
  implicit-type traps. Hidden in an HTML comment, either is invisible to the people who need to see
  the relation.
- **A `Child:` line a person writes on the parent.** A description is replaced whole on every edit,
  so two children created at once lose one line silently; deriving the list keeps one truth.

The maintainer chose TOML front matter, English keys shared by every consumer, `related` mirrored
on both ends, one note per key, and the derived `children` and `blocks` (2026-10-22). Shown: the
options above with the server's rendering of each, and the cost of each in migration and grammar.
These are theirs to reverse; the rest — the value rules, the fixed key order, the refusal of unknown
keys — is derived and falls to a better derivation.

## Consequences

The numbers in the box are not clickable; the issue page's linked items are, with title and state,
since every reference is linked. A consumer upgrades and migrates in one sitting: this gltree reads
no first line, and an older one reads no header.
