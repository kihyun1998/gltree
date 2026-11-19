# Reading the server

## What it is
How gltree reaches a GitLab tracker and talks to it: the project from the clone's `origin`, the
token and CA from the user's glab login, TLS that accepts a self-hosted server's self-signed sheet,
and the GitLab adapter behind the tracker port — its GraphQL reads, REST writes and their limits.

## Governing decisions
**None.** The TLS choices below are the maintainer's calls on issues (#9, #10), not decision
records. [ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md) decides
the walk this adapter serves, not the adapter.

## Design model
- **The port** (`Tracker`): `all_items`, `item_with_comments`, `level`, `create_item`, `add_link`,
  `set_desc`, `remove_link`. `GitLab`, `SnapshotFile` (read only) and `FakeTracker` implement it.
- **Project and host** from `git remote get-url origin`: `https://[user@]host[:port]/path[.git]`,
  `ssh://user@host[:port]/path[.git]`, `user@host:path[.git]` (`Remote::parse`).
- **Login** from glab's config, in glab's own search order: `GLAB_CONFIG_DIR/config.yml` alone when set,
  else `~/.config/glab-cli/config.yml`, then the platform directory (`XDG_CONFIG_HOME`, else
  `%LOCALAPPDATA%` on Windows, `~/Library/Application Support` on macOS). From `hosts.<host>`:
  `token`, `ca_cert`, `api_host`, `api_protocol`. `GITLAB_TOKEN` and `GLTREE_CA_CERT` override them.
  Source: `glab_config_paths`, `login`.
- **Reads** are GraphQL. `level` writes the item numbers into the query text (`iids:["3","12"]`), at
  most `LEVEL_BATCH` = 100 per request with `linkedItems(first:100)` each; an overflow of linked items
  is an error, since a short tree would look complete. `all_items` pages 100 at a time with links,
  labels, type, `closedAt` and `webUrl`. Comments have reads of their own
  ([keeping-a-snapshot](keeping-a-snapshot.md)).
- **Writes** are REST under `api/v4/projects/<url-encoded path>`: `POST issues`, `PUT issues/:iid`,
  `POST issues/:a/links` with `relates_to` (409, already linked, is success), and for an unlink the
  link id from `GET issues/:a/links` then `DELETE` (no link is success). A create is judged by its
  status, never by whether the body carries an `iid` (`created`, `linked`, `link_id`).
- **Errors** the server returns — GraphQL `errors` (complexity, rate limit), a non-2xx status — are
  `Error::Server`; every error exits 2. ureq's body size limit is lifted, since a level of 100 items
  with their linked bodies can be large (`call`; commit 2bcb3f2, 2026-10-21).
- **TLS with a configured CA, off macOS: native-tls.** A self-hosted server's certificate can be one
  self-signed sheet marked `CA:TRUE`; rustls/webpki refuse a CA certificate as the server's own
  (`CaUsedAsEndEntity`), while the OS verifiers accept a trusted self-signed sheet. A wrong CA is
  refused ("untrusted root"), measured on Windows (commit 2bcb3f2, 2026-10-21). Without a configured
  CA, ureq's default rustls with the WebPki roots (commit 9e25da8, 2026-10-22).
- **TLS with a configured CA, on macOS: gltree's pinned verifier** (`src/macos_tls.rs`). macOS
  native-tls is Security.framework, which refuses a server certificate whose validity exceeds Apple's
  825-day maximum, and a self-hosted CE's sheet is routinely a decade long — measured (#9). The
  verifier trusts only the configured CA: the presented leaf must be byte-identical to one of its
  certificates, and the host name, the certificate's own validity window and the handshake signatures
  are still checked; only the maximum-span policy is dropped. The maintainer's call on #10,
  reversible, chosen over re-issuing the server certificate with a ≤398-day span (commit 9e25da8,
  2026-10-22). Verified live on macOS against the server native-tls refused (same commit).
- **Linux musl targets** build OpenSSL in (ureq's `vendored` feature), so they reach the same servers
  (commit e5fe873, 2026-10-22).

## Code
- `src/tracker.rs — Tracker, Error, Linked, Created, FakeTracker, level_of`
- `src/remote.rs — Remote::parse, Remote::of_clone`
- `src/login.rs — Login, TOKEN_ENV, CA_ENV, glab_config_paths, login`
- `src/gitlab.rs — GitLab::new, GitLab::graphql, GitLab::rest, GitLab::call, Tracker for GitLab, LEVEL_BATCH, level_query, parse_level, page_query, parse_page, brief, answer, created, linked, link_id, encode_path`
- `src/macos_tls.rs — agent, parse_ca_ders, PinnedVerifier, check_name_and_validity, host_matches, PinnedConnector, PinnedTlsTransport`
- `src/main.rs — server, source`
- `Cargo.toml — ureq features, the macOS-only rustls and x509-parser dependencies`

## Reference behaviour
- glab's config format and `ca_cert` handling: its source tree, gitlab.com/gitlab-org/cli — raw,
  binding (thegraph build, [docs/agents/thegraph.md](../agents/thegraph.md)).
- GitLab CE GraphQL and REST: the GitLab docs — summarized, binding; what rests on them needs
  confirming against a server (same build).
- The issue search API is rate-limited ("requested with the search argument too many times") and
  matches text anywhere in a body; the walk does not use it
  ([ADR-0001](../adr/0001-the-child-records-its-parent-and-links-are-the-way-down.md)).
- The GraphQL complexity cap on a self-hosted CE is 250; the level query with 100 items fits
  (commit 2bcb3f2, 2026-10-21).

## Cross-cutting invariants
- [Not recorded is not none](not-recorded-is-not-none.md) — `parse_page` records links, `brief` and `read_item_with_comments` leave them `None`.

## Blast radius
- [drawing-a-tree](drawing-a-tree.md) — the walk assumes `level` returns every linked item or fails; a silently short page draws a short tree as complete.
- [checking-the-tracker](checking-the-tracker.md) — `unlinked` and `stray-link` assume `all_items` records every link.
- [writing-the-tree](writing-the-tree.md) — the write steps assume 409 means linked and a missing link on unlink is success, so a rerun is safe.
- [keeping-a-snapshot](keeping-a-snapshot.md) — the snapshot is `Item` as this adapter fills it; a field read here and not saved is lost to every `--snapshot` reader.

## Known holes / open
- A token glab keeps in the OS keyring (`use_keyring`) is not read; the error names `GITLAB_TOKEN`.
- The macOS pinned-verifier path builds its own connector chain, so it does not go through an HTTP or SOCKS proxy. Tracked: #11.
