# thegraph build — gltree

What a person already knows and no file in this repo answers.

## What this project is

A Rust CLI that reads and writes the issue trees of trackers on a self-hosted GitLab CE, and the
single owner of their tree convention (ADR-0002). Its consumers are internal repositories: their
names, paths and server stay out of this public repository, in the untracked
`docs/agents/consumers.local.md`.

## References

| Source | Informs | Reached by | Binding |
|---|---|---|---|
| each consumer's Python tree tool and its tests | how it works: `tree`/`check` output, line for line | the clone paths in `consumers.local.md` — raw | binding |
| each consumer's `docs/agents/issue-tracker.md` § Structure | how it works: the convention, until gltree holds it | the same clones — raw | binding |
| GitLab CE 17.3 GraphQL and REST | how it works | the GitLab docs site — **summarized**; confirm against a consumer's server | binding |
| glab: config format, `ca_cert` handling | how it works | its source tree, gitlab.com/gitlab-org/cli — raw | binding |
| `gh` · `glab` | how it works: command and flag layout | their source trees — raw | example |
| the maintainer's `just-shield` release workflow and `homebrew-tap` / `scoop-bucket` auto-update | where files go: the release and distribution pipeline | github.com/kihyun1998 — raw | example |

**Parity is per consumer.** gltree matches each consumer's own copy, since the copies drift. Each
consumer keeps its own state policy (`roots-only` or `every-parent`); finding text is English in
gltree, so a consumer whose tool writes another language matches on kind and item only.

**Nothing internal is written here** — no server name, consumer name, issue number or count from
a consumer's tracker, in code, tests, docs, commits or GitHub issues. Tests use invented fixtures.
