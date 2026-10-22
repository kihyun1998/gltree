# gltree

Issue trees for GitLab trackers where every item is an Issue and each names its parent on its
body's first line, `Parent: #N` — the convention gltree owns ([ADR-0001](docs/adr/0001-the-parent-line-is-the-truth-and-links-are-the-way-down.md),
[ADR-0002](docs/adr/0002-gltree-owns-the-tree-convention.md)). Vocabulary: [GLOSSARY.md](GLOSSARY.md).

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

Kinds: `parent-line`, `origin-line`, `task`, `missing-parent`, `missing-origin`, `unlinked`,
`stray-link`, `cycle`, `closed-parent-open-child`, `open-parent-all-closed`. `--json` prints
`{"version": 1, "findings": [{kind, item, detail}]}`.

## Roots

```sh
gltree roots --closed --without-label "nit::*" --json      # finished features with no registration label
gltree roots --closed-since 2026-10-19 --closed-until 2026-10-25 --label SPEC
```

Lists items with no Parent line, filtered with AND: `--open` / `--closed`, `--label` (all of
them), `--without-label` (none of them; a trailing `*` matches a prefix), and an inclusive UTC
closing-date range that implies `--closed`. Text: `#N [state] closed:<date|-> labels:<a,b> title`.
`--json` prints `{"version": 1, "roots": [{number, title, state, labels, closed_at, first_line, url, origin}]}`.

## New

```sh
gltree new --parent 12 --title "The drawer keeps its width" --body-file body.md --label bug,UI:Dialog
gltree new --from 13 --title "Trailing space on the line" --body-file body.md   # found while working #13
```

Creates an issue whose body is `Parent: #12`, a blank line, then `body.md` (UTF-8), and links it
to #12, in one step. Without `--parent` it creates a root. `--from 13` adds the origin line
`<origin_word>: #13` (under the Parent line, if any) and links #13: the issue was found while
working #13 and is not part of it ([ADR-0003](docs/adr/0003-where-an-item-was-found-is-not-what-it-is-part-of.md)).
Only the labels passed are applied.
It refuses, before writing anything, in a repo without `.gltree.toml`, when #12 or #13 does not
exist, when `body.md` already opens with a `Parent:` or origin line, or when #12 is closed and the
state policy would then report it closed over an open item — reopen #12, or use `--from 12`. If
the issue is created but a link fails, it names the issue and the links left to add, and exits 2.
`--json` prints `{"version": 1, "created": {number, url}, "link": {parent, child} | null, "origin": N | null}`.

## Move

```sh
gltree move 19 --to 12      # #19's Parent line becomes `Parent: #12`; its old parent is unlinked, #12 linked
gltree move 19 --to-root    # the Parent line (and the blank line under it) and the old link go
```

Every other line of the body stays byte for byte, and an origin line stays directly under the
Parent line. It refuses a target that is #19 or below it, or a closed target the state policy
would then report, and says so and writes nothing when #19 already has that parent. Like `new`, it writes only in a
repo with `.gltree.toml`. `--json` prints `{"version": 1, "item", "from", "to", "changed"}`.

## Repo config

A consuming repo commits `.gltree.toml` at its root; every key but `origin_word` is optional:

```toml
origin_word = "From"              # required: the origin line is `From: #N`; one word, not `Parent`
root_label = "SPEC"
relation_scheme = "parent-line"   # known: parent-line
state_policy = "roots-only"       # or "every-parent": every parent stays open while a child is open
links = "tree-only"               # or "shared": links also join related issues, no stray-link
```

Exit status: 0 on success, 1 when `check` finds anything, 2 on a usage, input, config or server error.
