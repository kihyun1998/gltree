# The Parent line is the truth, and links are the way down

GitLab CE's hierarchy stops at Issue → Task and a Task holds no child, so a tracker that needs
follow-ups of follow-ups records the parent as the item's first body line `Parent: #N`. Nothing
points the other way, so finding an item's children would mean reading every body (about 35 s
for a tracker of a few thousand items, growing with the tracker). A `relates_to` link therefore joins every parent and
child and is reserved for that alone: a tree is walked down along links, one request per depth
(about 2.5 s, growing with the tree's depth only), and a linked item counts as a child only when
its own Parent line names the node, because a link has no direction.

## Considered Options

- **The CE hierarchy.** Two levels only; a follow-up of a slice cannot exist.
- **Reading every item per tree.** Correct, but its cost grows with the whole tracker.
- **The issue search API** for `Parent: #N`. Rate-limited on the server ("requested with the
  search argument too many times"), and it matches the text anywhere in a body.
- **The link as the truth.** It has no direction, so it cannot say which end is the parent.

## Consequences

A child without its link is invisible to a tree walk, so creating or moving a child writes the
line and the link together, and `check` reports a Parent line without its link, and a link that
joins no parent and child.
