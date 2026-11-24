# gltree

Issue trees for GitLab trackers where every item is an Issue and each records its references —
its parent above all — in a TOML header at the top of its body, the convention gltree owns
([ADR-0004](docs/adr/0004-references-live-in-a-toml-header.md),
[ADR-0001](docs/adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md),
[ADR-0002](docs/adr/0002-gltree-owns-the-tree-convention.md)). Vocabulary: [GLOSSARY.md](GLOSSARY.md).

## The header

```
+++
parent = 12
from = 8
related = [30]
blocked_by = [40]

[notes]
from = "found while reproducing over SSH"
+++

The body …
```

The block opens the description, between `+++` lines; GitLab shows it as a box above the body. An
item with no references has none. Numbers are integers without `#`; lists are ascending arrays with
no number twice. `parent` says the item is part of #12; `from` says it was found while working #8
and is not part of it ([ADR-0003](docs/adr/0003-where-an-item-was-found-is-not-what-it-is-part-of.md));
`[notes]` holds one reason per reference key; `[extra]` holds the keys the repo config declares.
gltree also writes `children` and `blocks` on the other end of a `parent` or `blocked_by`, and
mirrors `related`. Any other key is refused.

## Install

macOS / Linux (Homebrew):

```sh
brew tap kihyun1998/tap
brew trust kihyun1998/tap      # once; recent Homebrew refuses an untrusted third-party tap
brew install gltree
```

Windows (Scoop):

```sh
scoop bucket add kihyun1998 https://github.com/kihyun1998/scoop-bucket
scoop install gltree
```

Or an archive from the [releases](https://github.com/kihyun1998/gltree/releases) (Linux musl,
macOS, Windows; x64 and arm64), or from source: `cargo install --git https://github.com/kihyun1998/gltree --locked`.

## Use

Inside a clone of the tracker's project:

```sh
gltree tree 12                                  # the tree under #12, read from the server
```

gltree finds the project in `git remote get-url origin` and logs in with your glab login for that
host (its token and `ca_cert`). `GITLAB_TOKEN` and `GLTREE_CA_CERT` override them. It reads only
the tree asked for: one request per depth, along the links between parents and children.

From a saved snapshot instead:

```sh
gltree tree 12 --snapshot tracker.json          # the tree under #12
gltree tree --snapshot tracker.json             # every root's tree that holds an open item
gltree tree --all --snapshot tracker.json       # every root's tree
gltree tree 12 --snapshot tracker.json --json   # the same tree as JSON
```

```
#12 [open] A settings drawer asks every value again  3/6
  #13 [closed] Rows are drawn from one list  0/1
    #19 [open] Trailing space on the line
```

Each node shows its state, `closed/total` of its descendants when it has any, and
`blocked by #B` while a blocker is open. `--json` prints `{"version": 3, "roots": [node]}`, where a
node is `{number, title, state, url, labels, extra, closed, total, blocked_by_open, children}`. `url` is
`null` when the source did not record it (an older snapshot), and `extra` holds the header's `[extra]`
values, as `roots --json` gives them.

Every root's tree (`gltree tree` with no number) reads every item and leaves out a family whose
every item is closed: the root and everything drawn under it, a lone closed root included. A family
with any open item is drawn whole, closed items included, and `closed/total` still counts every
descendant. `--all` draws every family; it takes no number, since `gltree tree N` always draws
N's tree. `--json` leaves out the same roots; the shape is the same.

## Check

```sh
gltree check                       # every way the tracker breaks the convention; exit 1 when any
gltree check --json
```

```
unlinked                   #19  no relates_to link to #13, so tree <root> misses it
1 anomalies
```

Kinds: `header` (a block out of form, with the reason), `old-header` (a body still opening with a
first-line head such as `Blocked by: #4`), `task`, `missing-parent`, `missing-reference`,
`unlinked`, `stray-link` (a link no reference on either end explains), `derived` (a list gltree
keeps that the truth does not give; `gltree sync` rewrites it), `blocker-cycle`, `cycle`,
`closed-parent-open-child`, `open-parent-all-closed`. Every number under `parent`, `from`,
`related` and `blocked_by` needs its `relates_to` link, and every link needs a reference.
`--json` prints `{"version": 1, "findings": [{kind, item, detail}]}`.

## Snapshot and show

```sh
gltree snapshot tracker.json             # save every item for --snapshot
gltree show 12 --snapshot tracker.json   # one item, from the file
gltree show 12                           # one item, from the server
gltree show 12 --json
```

```
#12 [open] A settings drawer asks every value again
labels: SPEC

  The body of the item.

2 comments

mina  2026-01-02T03:04:05Z
  The first comment.

jo  2026-01-03T03:04:05Z  [internal]
  A comment only project members can see.
```

What people wrote is indented under gltree's own lines, with control characters taken out, so a
comment cannot pass for the head of another. `--json` gives the text as written.

A snapshot is a read-only copy of the tracker as of the moment it was taken, so it can be read while
the server is unreachable. It is a JSON list of items: `iid`, `title`, `state`, `desc`, and
optionally `labels`, `links`, `closed_at`, `url` and `comments`. Each comment has `author`,
`created_at`, `body` and `internal`, oldest first. System notes such as label changes are not saved.
Attachments stay links to the server.

**A snapshot can hold internal comments**, as far as the token that took it can read them. Treat the
file like the tracker it was copied from. On Unix a new snapshot is readable by its owner alone, and
a replaced one keeps its mode. On Windows the file takes the access of its folder, every time it is
replaced, so access narrowed on the file itself is lost: keep it in a folder with the access you want.
Save it outside the clone or list it in `.gitignore`, so it is not committed by accident.

`snapshot` and `show` from the server need GitLab 17.11 or later. Below that they stop with the
server's error and the previous snapshot stays; `show --snapshot` needs no server at all.

`gltree snapshot` reads the whole tracker and its comments every time, which takes longer than
`check` or `roots`. Those commands read no comments. The file is replaced only once the new one is
complete: a failed read or write leaves the previous snapshot as it was. `migrate --out` writes the
same way.

`--json` prints `{"version": 1, number, title, state, labels, body, comments}`. `comments` is `null`
when comments were not recorded (a snapshot taken before they were saved, or written by
`migrate --out` from the server, which reads none), and `[]` for an item with none; the text output
tells the two apart as well. `body` is the whole description, its header included. A number the source does not hold exits 2 and names it.

## Roots

```sh
gltree roots --closed --without-label "reg::*" --json      # finished features with no registration label
gltree roots --closed-since 2026-10-19 --closed-until 2026-10-25 --label SPEC
```

Lists items with no `parent`, filtered with AND: `--open` / `--closed`, `--label` (all of
them), `--without-label` (none of them; a trailing `*` matches a prefix), and an inclusive UTC
closing-date range that implies `--closed`. Text: `#N [state] closed:<date|-> labels:<a,b> title`.
`--json` prints `{"version": 2, "roots": [{number, title, state, labels, closed_at, first_line, url, from, extra, blocked_by_open}]}`,
where `first_line` is the body's first line after the header and `blocked_by_open` the blockers
still open.

## New

```sh
gltree new --parent 12 --title "The drawer keeps its width" --body-file body.md --label bug,UI:Dialog
gltree new --from 13 --title "Trailing space on the line" --body-file body.md   # found while working #13
gltree new --parent 12 --related 30 --blocked-by 40 --title "…" --body-file body.md
```

Creates an issue whose body is the header (`parent = 12`), a blank line, then `body.md` (UTF-8),
links it to every item its header names, and syncs the lists on the other ends — `children` on
#12, `blocks` on #40, `related` on #30 — in one step. Without `--parent` it creates a root.
`--from 13` adds `from = 13`; `--related` and `--blocked-by` repeat. Only the labels passed are
applied. It refuses, before writing anything, in a repo without `.gltree.toml`, when a named item
does not exist, when `--from` names the parent, when `body.md` already opens with `+++`, or when
#12 is closed and the state policy would then report it closed over an open item — reopen #12, or
use `--from 12`. If the issue is created but a later step fails, it names the issue and the steps
left, in order, and exits 2. `--json` prints
`{"version": 2, "created": {number, url}, "link": {parent, child} | null, "from": N | null, "related": [N], "blocked_by": [N]}`.

## Edit

```sh
gltree edit 19 --add-related 30 --add-blocked-by 40
gltree edit 19 --remove-related 30 --remove-blocked-by 40
gltree edit 19 --set-extra "marker=K-1 (registered)"   # a declared [extra] key
```

Changes #19's `related` and `blocked_by`, with the links and the other ends: an added related item
lists #19 back and an added blocker lists it under `blocks`; a removed related item is dropped on
both ends, and a removed blocker's `blocks` is synced. A link goes only when no reference on either
end still explains it, and a note goes with the key it explained. It refuses, before writing, a
header out of form, #19 itself, a missing item to add, and a blocker that #19 already blocks at any
depth, and an `[extra]` key the repo config does not declare; an edit that changes nothing writes nothing. `--json` prints
`{"version": 1, "item", "related", "blocked_by", "changed"}`.

## Move

```sh
gltree move 19 --to 12      # #19's parent becomes #12; its old parent is unlinked, #12 linked
gltree move 19 --to-root    # the parent and the old link go; an empty header goes with them
```

Only `parent` changes (and a note on it, when it goes); the rest of the header and the body stay
byte for byte, and `children` is synced on both parents. The old link stays when another reference
still explains it. It refuses a header
out of form, a target that is #19's `from`, #19 or below it, or a closed target the state policy
would then report, and says so and writes nothing when #19 already has that parent. Like `new`, it
writes only in a repo with `.gltree.toml`. `--json` prints `{"version": 1, "item", "from", "to", "changed"}`,
where `from` and `to` are the parent before and after.

## Migrate

```sh
gltree migrate --snapshot tracker.json --out migrated.json   # plan offline, save the result
gltree check --snapshot migrated.json                        # what check would say afterwards
gltree migrate --from-word Origin --apply                    # write it to the server
```

Turns a tracker written in the first-line format into headers. The old head is the key lines at
the top of a body, blank lines between them allowed: `Parent: #N`, `From: #N` (and each
`--from-word`), `Related: #N, #M` (and each `--related-word`), `Blocked by` / `Depends on`, and a
line whose key is a declared extra key (`Marker:` for `extra_keys = ["marker"]`). Text after the numbers
becomes the key's note. The head ends at the first line that is none of these — a key line that
names no issue number, such as `Blocked by: none`, stays in the body — and the body from there is
kept byte for byte. Then every derived list is filled over the whole tracker, and every reference
that lacks its link gets one. An item that already opens with a header is left alone, so a second
run plans nothing. Dry run unless `--apply`; prints each item's old head (`-`) and new block (`+`),
the links, and counts by kind. `--apply` needs `.gltree.toml` and the server.

## Sync

```sh
gltree sync 12 --dry-run    # what #12's children, blocks and related should be, written nowhere
gltree sync 12              # rewrite them
gltree sync                 # every item, from one read of the whole tracker
```

Rewrites the lists gltree keeps from the truth on the other end: `children` from the items whose
`parent` is #12, `blocks` from those whose `blocked_by` names it, and `related` as the union of
both ends. Nothing else in the header or the body changes, and an item already right is not
written, so a second run writes nothing. `sync 12` reads #12 with the items linked to it — every
reference has a link — so a reference without its link is not seen until `check`'s `unlinked` is
fixed. It refuses a header out of form. Writing needs `.gltree.toml`; `--dry-run` does not.
Text: `#N key [before] -> [after]` per change, then `N items would change` (with `--dry-run`) or
`N items changed`; `--json` prints
`{"version": 1, "written", "changes": [{item, key, before, after}]}`.

## Serve

```sh
gltree serve                             # http://127.0.0.1:7420/, read from the server
gltree serve --snapshot tracker.json     # from a file; refresh reads the file again
gltree serve --port 0                    # a free port
gltree serve add ../web --name site      # show another clone's tracker too
gltree serve list                        # the repos serve shows
gltree serve remove site                 # take one off the list
```

Starts a local web server on `127.0.0.1` only, prints its address at once and serves a page until
Ctrl+C. The whole tracker is read in the background; until that first read is done the page says it
is reading, and the terminal says on stderr when it starts and when it ends, with the number of
families and the time it took, or why it failed. A first read that fails leaves the server running:
the page shows why, and the refresh button reads again. What can be checked without the network is
checked before the address is printed, and a failure there stops with exit status 2: the repo
list, the repo config, the port, and for the server git's `origin`, the glab login and the CA certificate, or for
`--snapshot` that the file opens and is not a directory. On the left are the families, as `gltree tree` with no number picks
them (the finished ones behind a toggle), newest (highest number) first, each with its title, a
progress bar, how many items are left and its badges. Every count above the list spans every family,
finished ones included. On the right is the selected family's tree, read fresh when it is selected,
children in ascending order, with every item linked to its page on the server; each item shows at
most three labels and a `+k` chip naming the rest. A mark tells an item to do (a ring), blocked by
an open item (a ring with a bar) and done (a check) apart, in the list, the detail and the tree.
Under each item, children whose whole subtree is closed fold into one `✓ N done — show` row that
unfolds them; the "Show done items" switch unfolds them all. The refresh button reads the
whole tracker again, which takes as long as `gltree tree` does without a number. The page fetches nothing from the internet, follows the system's light or dark
mode, and turns its motion off when the system asks for less. It writes nothing to the tracker, and
the token stays in the server process.

`gltree serve` shows one tracker, the clone it runs in, until repos are registered. `gltree serve add
PATH` registers the clone at PATH in `~/.gltree/repos.toml` (`~` being your home folder,
`%USERPROFILE%` on Windows), under the last part of its GitLab project's path unless `--name` gives
another; a name is 1 to 64 letters, digits, `.`, `_` or `-`. It refuses a path that is not a git
clone, a clone whose `origin` gives no host and project path, and a name already taken, and does not
need a `.gltree.toml`. It does not ask the host whether it is GitLab: a repo whose host is not shows
why in its tab when `serve` reads it. `gltree serve list` prints `NAME  PATH` per repo in the order they were registered,
and `gltree serve remove NAME` takes one off. With at least one repo registered, `gltree serve` shows
every one of them wherever it runs, and reads each clone's `origin`, `.gltree.toml` and glab login
as it would inside it: a row of tabs above the page names each repo with its open items (`app 78
left`), a dot marking one still being read or one whose read failed (its tooltip says why), and the
chosen tab shows that repo's page as above and stays in the address after `#`, so a reload keeps it.
Every registered repo is read at start, all at once; the refresh button reads the repo shown. A
registered clone that is gone, or whose `origin`, login or `.gltree.toml` fails, fails its own tab
and leaves the others running. `--snapshot` shows that file alone and leaves the list out. No other
command reads the list: each keeps taking its tracker from the clone it runs in.

The page reads a small JSON API:

- `GET /api/repos`: `{"version": 3, "repos": [{"name", "state", "error", "read_at", "families",
  "open"}]}`, the registered repos in the order they were registered; `families` and `open` count
  the families and open items of the last full read that succeeded, `null` until one has. With no
  repo registered, or with `--snapshot`, `repos` is empty.
- `GET /api/families`: `{"version": 3, "repo", "state", "error", "read_at", "badges", "tree"}`. `repo`
  is the repo's name, `null` with no repo registered. `state` is
  `reading` while a full read runs, `ready` when the last one succeeded and `failed` when it failed,
  with `error` saying why (`null` otherwise). `tree` is the last full read that succeeded, as
  `gltree tree --all --json` (version 3) gives it, and `read_at` its UTC time; both are `null` until
  one has. `badges` is the repo config's list.
- `POST /api/refresh`: reads the whole tracker again and answers the same shape. While a read runs it
  answers that shape at once, `state` `reading`, and starts nothing.
- `GET /api/tree/N`: N's tree as `gltree tree N --json` gives it, read fresh; 404 for a number the
  tracker does not hold.

With repos registered, `/api/families`, `/api/refresh` and `/api/tree/N` name one with
`?repo=NAME`, compared as sent: without it they answer 400, and 404 for a name not registered. With
none registered they take no `repo` and answer 404 to one.

An error answers `{"version": 3, "error"}`: 404 for an unknown path or number, 405 (with
`Allow`) for a known path asked with another method, 400 and 413 for a request it cannot read or a
body it does not take, 502 when the tracker could not be read. The server answers only requests
addressed to `127.0.0.1` or `localhost` at its port (403 otherwise), and the API only its own page:
a refresh from another site's page, or any API call a browser marks as coming from another site, is
refused before anything is read.

## Repo config

A consuming repo commits `.gltree.toml` at its root; every key is optional:

```toml
root_label = "SPEC"
relation_scheme = "front-matter"  # known: front-matter
state_policy = "roots-only"       # or "every-parent": every parent stays open while a child is open
extra_keys = ["marker"]           # keys the header's [extra] may hold, kept as written
old_head_keys = ["Origin"]        # the repo's own first-line words, so check reports one left in a body

[[badges]]                        # a label prefix gltree serve shows as a badge on roots; repeat for more
prefix = "reg::"                  # a root's label starting with it is shown, the rest as its value
name = "Registration"
extra = "marker"                  # optional: an extra key whose value is shown beside it
missing = "undecided"             # optional: what a closed root without such a label shows
```

gltree knows no badge by name: the repo says which label prefixes to show, and the page shows the
label and, beside it, the `[extra]` value it names, as `tree --json` passes them through. A badge's
`extra` must be one of `extra_keys`; otherwise every command stops at start, as on any other config
error. A gltree older than a key here refuses the config, so state the version that reads it beside
the file.

Exit status: 0 on success, 1 when `check` finds anything, 2 on a usage, input, config or server error.
