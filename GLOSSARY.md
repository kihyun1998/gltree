# gltree

The tree a GitLab tracker forms when every item is an Issue and each records its references in its own body.

## Language

### The tree

**Item**:
One GitLab Issue in a tracker that gltree reads.
_Avoid_: ticket, task, work item

**Header**:
The TOML block between `+++` lines that opens an item's description; the only record of the item's references (ADR-0004). An item with no references has none.
_Avoid_: front matter (the general term), metadata, Parent line

**Parent**:
The item a header's `parent` names: the item this one is part of.
_Avoid_: hierarchy parent

**Child**:
An item whose header names a given item as its `parent`. gltree draws no line between kinds of child.
_Avoid_: slice, follow-up, sub-issue, Task

**Root**:
An item with no `parent`.

**Origin**:
The item a header's `from` names: the item this one was found while working. An item is not part of its origin, and an origin's state does not depend on it (ADR-0003).
_Avoid_: source

**Find**:
An item with a `from`. With a `parent` it is part of that parent; without one it is a root.
_Avoid_: follow-up (consumers use it for both), spin-off

**Related item**:
An item a header lists under `related`: see also, with no bearing on state. gltree lists it on both ends.

**Blocker**:
An item a header lists under `blocked_by`: this one cannot start until it closes.

**Derived list**:
A header key gltree writes from the truth on the other end — `children` from `parent`, `blocks` from `blocked_by`, and the `related` mirror. A person never edits one.

**Descendant**:
A child, or a descendant of a child.

**Link**:
A `relates_to` link between two items. It has no direction; every reference in a header has one, and every link is explained by a reference.
_Avoid_: relation

### Rules a repo chooses

**Relation scheme**:
Where a tracker records the parent and how a tree is walked down.

**State policy**:
The rule for when an item may be closed or open against its descendants.

**Extra key**:
A key the repo config declares for the header's `[extra]` table; gltree keeps it as written and never reads it.

**Repo config**:
The file a consuming repo commits to say it uses gltree and which root label, relation scheme, state policy and extra keys it takes.
