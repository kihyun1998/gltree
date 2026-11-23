## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues on `kihyun1998/gltree` (via `gh`). See `docs/agents/issue-tracker.md`.

### Triage labels

Default canonical labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one root `GLOSSARY.md` plus `docs/adr/`. See `docs/agents/domain.md`.

## Comments

A comment is one of: a license header; behaviour forced by an outside dependency, platform, vendor or protocol we cannot reshape (Rust's `SAFETY:` on `unsafe` is one); a tool directive; a doc comment on a public API; an issue or RFC link for a constraint the code cannot express. Nothing else: a surprise in our own code is fixed, not explained. A measured value, an outside constraint and the reason behind a design judgement go to the territory note under `docs/map/`. Where none seems to hold it, search the folder and the code symbol first; only then start a new note, named for what the system does there, not for the file. A workaround's reason goes nowhere: the workaround is fixed. History goes to the commit message. Comments written before this rule may still carry what the map keeps: never delete one whose content the map does not yet hold — move it there first.


## Public artifacts

This repository is public. Commits, issues, comments, docs, code, tests and fixtures name no consumer repository, no internal server host and no internal identifier (an issue number or label scheme from a private tracker). Use a generic stand-in instead: `a self-hosted GitLab`, `example.com`, a made-up label. Before a push, search the outgoing commits for the names a session has seen.

## Map

The territory map is [docs/map/README.md](docs/map/README.md). Before designing a change, open the territory that holds the code and follow its blast radius and invariants. After the change lands, bring back in line the notes it made false. When a fix reveals a fact, ask whether it holds at every site that shares the same assumption; if it does, the fix lands with an invariant note.
