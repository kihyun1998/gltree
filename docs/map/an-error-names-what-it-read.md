# An error names what it read

## The fact
An error about something gltree read, or could not read, names that thing: the snapshot file, the
glab config and the glab login, a CA certificate, git's `origin` remote, the current directory, the
tracker, the server's answer, an item's description, the repo config, the repo list, a registered
clone, the home folder. No error borrows another
source's name, so the person reading `gltree: …` knows which file or answer to open.

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

`Error::Source` has the same shape for the same reason: `{ what, why }`, printed
`cannot read <what>: <why>`, built by `Error::unreadable(what, why)`. It used to print
`cannot read the tracker: …` at every site, so a missing glab token, a CA file that would not open
and a directory outside a clone all read as the tracker; `server` and `run` passed
`current_dir`'s error bare, which named nothing (measured with `login(None, …)` and the CLI outside a
clone, 2026-11-20). Copying `Malformed`'s shape is a derivation from the same argument, not a
separate judgement: a `what` beside the reason rather than a variant per source, because nothing
tells the sources apart after construction. The two split as their names say: a file or answer that
could not be opened or reached is `Source`; one that was read and is not in the shape gltree reads
is `Malformed`, so a CA file with no PEM certificate and an `origin` URL with no project path are
`Malformed`.

A server answer is judged by what it is, not by who sent it: GraphQL `errors` and a non-2xx status are
the server refusing, `Error::Server`, printed `the server refused: …`; a 2xx answer gltree cannot read
(not JSON, no `data` or a null one, links that are not JSON, a created issue without `iid`) is
`Error::malformed(ANSWER, …)`. A non-2xx body that is not JSON stays a refusal, since the status
already says the server refused (`answer`, tested with a 502 HTML body).

## Territories it holds in
- [keeping-a-snapshot](keeping-a-snapshot.md) — `SnapshotFile::parse`: `the snapshot file is malformed: …`; a file that cannot be opened or read is `cannot read the snapshot file <path>: …` (`SnapshotFile::open`, `Tracker for SnapshotFile`, both through `unreadable`), and a directory is `cannot read the snapshot file <path>: it is a directory` (`SnapshotFile::open`).
- [serving-the-viewer](serving-the-viewer.md) — `Repo::finish`: a full read that panics is `cannot read the tracker: the read stopped on a bug in gltree: …`, the panic's message after it. The repo list is `cannot read the repo list <path>: …` or `the repo list <path> is malformed: …` (`RepoList::load`), and `cannot write the repo list <path>: …` when a write fails (`RepoList::save`, an `Error::Refused`, since no variant names a write); a registered clone that is gone is `cannot read the clone <path>: it is not there`, and its config, refused or unreadable, `<path>/.gltree.toml: …`, the file's path before `config::Error`'s reason and no verb of ours, since that error does not tell a read that failed from a file refused (`registered_config`), and a home folder the system does not name `cannot read the home folder: …` (`repo_list`).
- [reading-the-server](reading-the-server.md) — `read_glab_config`: `cannot read the glab config <path>: …` for a config that is there and cannot be read (a directory, no permission); `login`: `the glab config is malformed: …`, bytes that are not UTF-8 included, and `cannot read the glab login: no token …`; `GitLab::new` and `macos_tls`: `cannot read the CA certificate <path>: …` for a file that will not open, `the CA certificate <path> is malformed: …` for one with no PEM certificate (`ca_name` in `src/gitlab.rs` builds the name for both files); `Remote::of_clone`: ``cannot read git's `origin` remote: …`` when git fails, ``git's `origin` remote is malformed: …`` for a URL with no project path; `call`, `parse_level`, `parse_page`, `brief` and `macos_tls::agent`: `cannot read the tracker: …` (`TRACKER` in `src/gitlab.rs`) for the network, a project the token cannot see, a connection longer than one page, and a rustls setup failure; `next_cursor` (for `parse_page` and `parse_comments`), `parse_comments`, `all_comments`, `brief`, `link_id`, `created` and `answer`: `the server's answer is malformed: …` (`ANSWER`), the reason naming the item, the field missing, or a page that says there is more but gives no cursor.
- [the-command-line-contract](the-command-line-contract.md) — `current_dir` in `src/main.rs`: `cannot read the current directory: …`, for `server` and `run`.
- [migrating-a-tracker](migrating-a-tracker.md) — `plan`: `#N's description is malformed: …` when `header::with_header` refuses the body under an old head.
- [reading-the-repo-config](reading-the-repo-config.md) — the config has its own `config::Error`, printed `.gltree.toml: <reason>`.

## What a violation looks like
`gltree: not a tracker snapshot: glab config: …` from a command given no snapshot; a malformed server
answer that sends the reader to a snapshot file they never named; `cannot read the tracker: no token for …`
for a login gltree never reached the tracker with.

## Discovery history
- Commit c80e895 (#29, 2026-11-20): the repo config got its own error type.
- #29 follow-up (2026-11-20): `Malformed` took `what`, and this note was written as one fact across the sites.
- #29 second follow-up (2026-11-20): `Source` took `what` too, `main` names the current directory, and the three unreadable answer sites became `Malformed`.
- #29 third follow-up (2026-11-20): a glab config that is there and cannot be read fails naming it, rather than reading as absent and then as holding no token.

## Where it will recur
A new `Error::malformed` or `Error::unreadable` site names what it read in `what`; an answer is a refusal only when the server says so. A new file or answer gltree reads takes a
name of its own, never one already used for another source; a new kind of input outside the tracker
takes its own error type, as `config::Error` does.

## Known holes / open
- `the glab config is malformed: …` names no path, while `cannot read the glab config <path>: …` does;
  with two candidates (Windows, macOS) the reader cannot tell which file to open (`login` is given
  the bytes, not the path). Not changed here.
- `Remote::of_clone` reports every failing `git remote get-url origin` as "there is none here" and
  drops git's stderr, so a broken `.git/config` on a clone with an origin reads as no origin
  (reproduced, 2026-11-20). Not changed here.
- A write that fails on the network prints `cannot read the tracker: …`: the source is right, the
  verb is not (`call` serves reads and writes). `set_desc` reuses `created`, so a 2xx PUT without
  `iid` says "a created issue". `--body-file` that cannot be read prints `gltree: <path>: …`, named
  by its path but not in the `cannot read …` shape (`src/main.rs`). Not changed here.
- The macOS sites in `src/macos_tls.rs` are not compiled on Windows, and only the release workflow
  builds macOS (`.github/workflows/release.yml`, build only, no tests); they were checked by reading.
