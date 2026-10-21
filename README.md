# gltree

Issue trees for GitLab trackers where every item is an Issue and each names its parent on its
body's first line, `Parent: #N` — the convention gltree owns ([ADR-0001](docs/adr/0001-the-parent-line-is-the-truth-and-links-are-the-way-down.md),
[ADR-0002](docs/adr/0002-gltree-owns-the-tree-convention.md)). Vocabulary: [GLOSSARY.md](GLOSSARY.md).

## Install

From source, until releases exist:

```sh
cargo install --git https://github.com/kihyun1998/gltree
```

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

Kinds: `parent-line`, `task`, `missing-parent`, `unlinked`, `stray-link`, `cycle`,
`closed-parent-open-child`, `open-parent-all-closed`. `--json` prints
`{"version": 1, "findings": [{kind, item, detail}]}`.

## New

```sh
gltree new --parent 12 --title "The drawer keeps its width" --body-file body.md --label bug,UI:Dialog
```

Creates an issue whose body is `Parent: #12`, a blank line, then `body.md` (UTF-8), and links it
to #12, in one step. Without `--parent` it creates a root. Only the labels passed are applied.
It refuses, before writing anything, in a repo without `.gltree.toml`, when #12 does not exist,
or when `body.md` already opens with a `Parent:` line. If the issue is created but the link fails,
it names the issue and the link left to add, and exits 2. `--json` prints
`{"version": 1, "created": {number, url}, "link": {parent, child} | null}`.

## Repo config

A consuming repo commits `.gltree.toml` at its root; every key is optional:

```toml
root_label = "SPEC"
relation_scheme = "parent-line"   # known: parent-line
state_policy = "roots-only"       # known: roots-only
links = "tree-only"               # or "shared": links also join related issues, no stray-link
```

Exit status: 0 on success, 1 when `check` finds anything, 2 on a usage, input, config or server error.
