# gltree

The tree a GitLab tracker forms when every item is an Issue and each names its parent in its own body.

## Language

### The tree

**Item**:
One GitLab Issue in a tracker that gltree reads.
_Avoid_: ticket, task, work item

**Parent line**:
An item's first body line `Parent: #N`; the only record of who its parent is.
_Avoid_: parent field, hierarchy parent

**Parent**:
The item a Parent line names.

**Child**:
An item whose Parent line names a given item. gltree draws no line between kinds of child.
_Avoid_: slice, follow-up, sub-issue, Task

**Root**:
An item with no Parent line.

**Descendant**:
A child, or a descendant of a child.

**Link**:
A `relates_to` link between two items. It has no direction; it joins every parent and child, and, in a repo whose config shares links, related items too.
_Avoid_: relation, related issue

### Rules a repo chooses

**Relation scheme**:
Where a tracker records the parent and how a tree is walked down.

**State policy**:
The rule for when an item may be closed or open against its descendants.

**Repo config**:
The file a consuming repo commits to say it uses gltree and which root label, relation scheme and state policy it takes.
