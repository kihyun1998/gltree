# The child records its parent, and links are the way down

GitLab CE's hierarchy stops at Issue → Task and a Task holds no child, so a tracker that needs
follow-ups of follow-ups records the parent in the item's own body — the header's `parent` key
(ADR-0004). Nothing the server indexes points the other way, so finding an item's children would mean
reading every body (about 35 s for a tracker of a few thousand items, growing with the tracker). A
`relates_to` link therefore joins every parent and child: a tree is walked down along links, one
request per depth (about 2.5 s, growing with the tree's depth only), and a linked item counts as a
child only when its own header names the node, because a link has no direction.

## Considered Options

- **The CE hierarchy.** Two levels only; a follow-up of a slice cannot exist.
- **Reading every item per tree.** Correct, but its cost grows with the whole tracker.
- **The issue search API** for the parent's number. Rate-limited on the server ("requested with the
  search argument too many times"), and it matches the text anywhere in a body.
- **The link as the truth.** It has no direction, so it cannot say which end is the parent.

## Consequences

A child without its link is invisible to a tree walk, so creating or moving a child writes the
header and the link together, and `check` reports a `parent` without its link.

Links were first reserved for the tree, then made a repo's choice when a second tracker linked
related issues too. ADR-0004 gives related issues a key of their own, so every link is explained by
a reference again, in every repo, and the choice is gone.
