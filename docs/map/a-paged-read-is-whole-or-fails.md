# A paged read is whole or fails

## The fact
Every GraphQL connection gltree asks of the server comes back whole or the read fails: it is either
followed page by page to its end, or asked as one page whose `hasNextPage` fails the read naming the
item. A first page is never taken for the whole. A connection bounded by the numbers it names asks
no `pageInfo`, since it cannot hold more than it asked: `level_query`'s `workItems(iids:…,first:LEVEL_BATCH)`
with at most `LEVEL_BATCH` numbers a request, and the single-item queries' `workItems(iids:[N],first:1)`.

## Why it is cross-cutting
Each connection is asked and parsed at its own site in `src/gitlab.rs`, and nothing ties them
together: a selection left without `pageInfo`, or a parser that does not read it, returns the
server's first page with no error. What reads the result cannot tell a short list from a full one —
a short tree draws as complete, `check` reports links as missing, a short label list shows a decided
root as `missing`.

## Territories it holds in
- [reading-the-server](reading-the-server.md) — `workItems` is followed by cursor and a next page without a cursor is `Malformed` (`GitLab::all_items`, `read_items_with_comments`, `parse_page`, #32); `notes` is followed by cursor and a next page without a cursor is `Malformed` (`parse_comments`, `all_comments`); both take the next cursor from `next_cursor`, the one place that tells a last page from a page that says there is more and gives no cursor; `linkedItems(first:100)` fails on overflow (`parse_level`, `parse_page`); `labels(first:100)` fails on overflow in `brief` (#28).
- [drawing-a-tree](drawing-a-tree.md) — the walk and `serve` trust `level` for every linked item and every label.
- [checking-the-tracker](checking-the-tracker.md) — `unlinked` and `stray-link` need every link of every item.
- [listing-roots](listing-roots.md) — the label filters need every label.
- [serving-the-viewer](serving-the-viewer.md) — the badges need every label.
- [showing-an-item](showing-an-item.md) — every comment is followed; more than 100 labels stops `show`.

## What a violation looks like
A tree with a child missing and nothing said; a root shown as `missing` a badge it carries; a
`roots --without-label` sweep that lists a root already registered.

## Discovery history
- Commit 2bcb3f2 (2026-10-21): `linkedItems` overflow is an error, not a short tree.
- Commit 4cec8b2 (#24, 2026-11-06): comments are followed to their end.
- #28 (2026-11-20): labels were read as one page with no sign of more; they now fail on overflow, and this note was written as one fact across the sites.
- #32 (2026-11-20): a page of work items that says there is more but gives no cursor fails the read, as a page of comments already did.

## Where it will recur
A new connection in any query (a field written `x{nodes{…}}` or `x(first:N){…}`) is subject to this:
ask `pageInfo`, and either follow it or fail naming the item.

## Known holes / open
**None.**
