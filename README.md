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
gltree tree --snapshot tracker.json             # every root's tree
gltree tree 12 --snapshot tracker.json --json   # the same tree as JSON
```

```
#12 [open] A settings drawer asks every value again  3/6
  #13 [closed] Rows are drawn from one list  0/1
    #19 [open] Trailing space on the line
```

Each node shows its state, and `closed/total` of its descendants when it has any. `--json` prints
`{"version": 1, "roots": [node]}`, where a node is `{number, title, state, closed, total, children}`.

A snapshot is a JSON list of items (`iid`, `title`, `state`, `desc`, and optionally `labels` and
`links`). Every root's tree (`gltree tree` with no number) reads every item.

## Check

```sh
gltree check                       # every way the tracker breaks the convention; exit 1 when any
gltree check --json
gltree snapshot tracker.json       # save every item, with its links, for --snapshot
```

```
unlinked                   #19  no relates_to link to #13, so tree <root> misses it
1 anomalies
```

Kinds: `header` (a block out of form, with the reason), `task`, `missing-parent`, `unlinked`,
`cycle`, `closed-parent-open-child`, `open-parent-all-closed`. `--json` prints
`{"version": 1, "findings": [{kind, item, detail}]}`.

## Roots

```sh
gltree roots --closed --without-label "reg::*" --json      # finished features with no registration label
gltree roots --closed-since 2026-10-19 --closed-until 2026-10-25 --label SPEC
```

Lists items with no `parent`, filtered with AND: `--open` / `--closed`, `--label` (all of
them), `--without-label` (none of them; a trailing `*` matches a prefix), and an inclusive UTC
closing-date range that implies `--closed`. Text: `#N [state] closed:<date|-> labels:<a,b> title`.
`--json` prints `{"version": 2, "roots": [{number, title, state, labels, closed_at, first_line, url, from, extra}]}`,
where `first_line` is the body's first line after the header.

## New

```sh
gltree new --parent 12 --title "The drawer keeps its width" --body-file body.md --label bug,UI:Dialog
gltree new --from 13 --title "Trailing space on the line" --body-file body.md   # found while working #13
```

Creates an issue whose body is the header (`parent = 12`), a blank line, then `body.md` (UTF-8),
and links it to #12, in one step. Without `--parent` it creates a root. `--from 13` adds
`from = 13` and links #13. Only the labels passed are applied. It refuses, before writing anything,
in a repo without `.gltree.toml`, when #12 or #13 does not exist, when `--from` names the parent,
when `body.md` already opens with `+++`, or when #12 is closed and the state policy would then
report it closed over an open item — reopen #12, or use `--from 12`. If the issue is created but a
link fails, it names the issue and the links left to add, and exits 2. `--json` prints
`{"version": 2, "created": {number, url}, "link": {parent, child} | null, "from": N | null}`.

## Move

```sh
gltree move 19 --to 12      # #19's parent becomes #12; its old parent is unlinked, #12 linked
gltree move 19 --to-root    # the parent and the old link go; an empty header goes with them
```

Only `parent` changes; the rest of the header and the body stay byte for byte. It refuses a header
out of form, a target that is #19's `from`, #19 or below it, or a closed target the state policy
would then report, and says so and writes nothing when #19 already has that parent. Like `new`, it
writes only in a repo with `.gltree.toml`. `--json` prints `{"version": 1, "item", "from", "to", "changed"}`,
where `from` and `to` are the parent before and after.

## Repo config

A consuming repo commits `.gltree.toml` at its root; every key is optional:

```toml
root_label = "SPEC"
relation_scheme = "front-matter"  # known: front-matter
state_policy = "roots-only"       # or "every-parent": every parent stays open while a child is open
extra_keys = ["marker"]           # keys the header's [extra] may hold, kept as written
```

Exit status: 0 on success, 1 when `check` finds anything, 2 on a usage, input, config or server error.
