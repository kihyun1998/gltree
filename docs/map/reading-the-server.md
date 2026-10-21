# Reading the server

How gltree reaches a GitLab tracker: the clone's remote, the user's glab login, TLS, and the
level walk (`src/remote.rs`, `src/login.rs`, `src/gitlab.rs`, `subtree` in `src/tree.rs`).

## Design model

- **The walk.** `tree <N>` asks one `level` per depth: the level's items and every item linked to
  them, bodies included. A linked item joins the tree only when its own first line names the node
  (ADR-0001); a link has no direction, so the parent itself and unrelated links come back too and
  are dropped. A global seen-set stops a cycle. Cost grows with the tree's depth and size, never
  with the tracker's.
- **The query** writes the item numbers into its text (`iids:["3","12"]`), at most 100 per request,
  with `linkedItems(first:100)` each. A page that says `hasNextPage` is an error: a short tree would
  look complete.
- **Project and host** come from `git remote get-url origin`: `https://[user@]host[:port]/path[.git]`,
  `ssh://user@host[:port]/path[.git]`, `user@host:path[.git]`.
- **Login.** glab's config file, found in glab's own search order: `GLAB_CONFIG_DIR/config.yml`
  alone when that is set, else `~/.config/glab-cli/config.yml`, then the platform config directory
  (`XDG_CONFIG_HOME`, else `%LOCALAPPDATA%` on Windows, `~/Library/Application Support` on macOS).
  From `hosts.<host>`: `token`, `ca_cert`, `api_host`, `api_protocol`. `GITLAB_TOKEN` (glab's own
  name) and `GLTREE_CA_CERT` override them.
- **Errors** the server returns — GraphQL `errors` (complexity, rate limit), a non-2xx status,
  an invalid token — are `Error::Server`; every error exits 2.

## Governing decisions

- **A configured CA goes through native-tls, not rustls.** A self-hosted server's certificate can
  be one self-signed sheet marked `CA:TRUE`; rustls/webpki refuse a CA certificate as the server's
  own (`CaUsedAsEndEntity`), while the OS verifiers (SChannel, Security.framework, OpenSSL) accept a
  trusted self-signed sheet. Without a configured CA, rustls with the WebPki roots is used.
  Verification is never switched off: a wrong CA is refused ("untrusted root"), measured on Windows.

## Known holes / open

- A token glab keeps in the OS keyring (`use_keyring`) is not read; the error names `GITLAB_TOKEN`.
- `tree` with no root from the server needs every item, which arrives with `check` (#4).
- native-tls on macOS against a `CA:TRUE` self-signed sheet is not yet measured.
- native-tls on Linux links OpenSSL; the musl release targets need it vendored (#7).

## Traps

- The GraphQL complexity cap on a self-hosted CE is 250; the level query with 100 items fits.
- The issue search API is rate-limited ("requested with the search argument too many times") and
  matches text anywhere in a body; the walk never uses it.
- ureq reads a body with a size limit by default; the adapter lifts it, since a level of 100 items
  with their linked bodies can be large.
