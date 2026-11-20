# An error names what it read

## The fact
An error about something gltree read names that thing: the snapshot file, the glab config, the
server's answer, an item's description, the repo config. No error borrows another source's name, so
the person reading `gltree: …` knows which file or answer to open.

## Why it is cross-cutting
`tracker::Error` is shared by every adapter and by the core, and the sites that build one do not see
each other: the snapshot file, the glab login, the GitLab adapter and `migrate::plan` each wrap their
own failure. A variant whose `Display` names one source reads as that source at every other site —
a broken glab config printed as "not a tracker snapshot" (measured on the binary at commit c80e895, 2026-11-20).

`Error::Malformed` therefore carries `what` beside `why`, printed `<what> is malformed: <why>`, and
`Error::malformed(what, why)` builds it: no site can build one without naming what it read. One field
rather than a variant per source, because nothing tells the sources apart after construction —
`main` prints every error and exits 2, and the tests match `Malformed { .. }` — and rather than
leaving the whole text to each site, because a name left to each site is how the snapshot's wording
reached the others. `what` is a `String` since `migrate::plan` names the item number (reading,
2026-11-20).

## Territories it holds in
- [keeping-a-snapshot](keeping-a-snapshot.md) — `SnapshotFile::parse`: `the snapshot file is malformed: …`; a file that cannot be read is `Error::Source` naming its path (`Tracker for SnapshotFile`).
- [reading-the-server](reading-the-server.md) — `login`: `the glab config is malformed: …`; `parse_comments`, `all_comments` and `brief`: `the server's answer is malformed: …` (`ANSWER` in `src/gitlab.rs`), the reason naming the item or the field missing.
- [migrating-a-tracker](migrating-a-tracker.md) — `plan`: `#N's description is malformed: …` when `header::with_header` refuses the body under an old head.
- [reading-the-repo-config](reading-the-repo-config.md) — the config has its own `config::Error`, printed `.gltree.toml: <reason>`.

## What a violation looks like
`gltree: not a tracker snapshot: glab config: …` from a command given no snapshot; a malformed server
answer that sends the reader to a snapshot file they never named.

## Discovery history
- Commit c80e895 (#29, 2026-11-20): the repo config got its own error type.
- #29 follow-up (2026-11-20): `Malformed` took `what`, and this note was written as one fact across the sites.

## Where it will recur
A new `Error::malformed` site names what it read in `what`. A new file or answer gltree reads takes a
name of its own, never one already used for another source; a new kind of input outside the tracker
takes its own error type, as `config::Error` does.

## Known holes / open
- Three answer sites in `src/gitlab.rs` build `Error::Server` for an answer they cannot read, so it prints `the server refused: …`: `link_id` (a links answer that is not JSON), `created` (a created issue without `iid`) and `answer` (a body that is not JSON, an answer without `data`). They name the server but call a malformed answer a refusal (measured with a probe test calling each, 2026-11-20). Not changed here.
- `Error::Source` prints `cannot read the tracker: <m>`, the site's message naming the path, URL or tool, including at sites that read something on the way to the tracker (the glab login, a CA certificate, git). `server` and `run` in `src/main.rs` pass `current_dir`'s error bare, which names nothing (reading, 2026-11-20). Not changed here.
