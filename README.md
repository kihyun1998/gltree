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

Each node shows its state, and `closed/total` of its descendants when it has any.

A snapshot is a JSON list of items (`iid`, `title`, `state`, `desc`, and optionally `labels` and
`links`). Reading the server directly is not built yet.

`--json` prints `{"version": 1, "roots": [node]}`, where a node is
`{number, title, state, closed, total, children}`.

Exit status: 0 on success, 2 on a usage, input or server error.
