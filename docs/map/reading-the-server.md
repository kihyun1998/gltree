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

- **A configured CA goes through native-tls, not rustls — except on macOS.** A self-hosted server's
  certificate can be one self-signed sheet marked `CA:TRUE`; rustls/webpki refuse a CA certificate as
  the server's own (`CaUsedAsEndEntity`), while the OS verifiers (SChannel, Security.framework,
  OpenSSL) accept a trusted self-signed sheet. Without a configured CA, rustls with the WebPki roots
  is used. Verification is never switched off: a wrong CA is refused ("untrusted root"), measured on
  Windows.
- **On macOS a configured CA is verified by gltree's own pinned rustls verifier** (`src/macos_tls.rs`),
  not native-tls. macOS native-tls is Security.framework, which refuses any server certificate whose
  validity span exceeds Apple's maximum (825 days); a self-hosted CE's self-signed sheet is routinely
  a decade long, so native-tls cannot reach it at all — measured (#9), and native-tls offers no way
  past it. The verifier trusts only the configured CA: the presented leaf must be byte-identical to
  it, which is how a `CA:TRUE` sheet is accepted as the server's own without the `CaUsedAsEndEntity`
  refusal. It still checks the host name, the certificate's own notBefore/notAfter and the handshake
  signatures; the single check it drops is Apple's maximum-span policy, so verification is moved to a
  verifier gltree owns, not switched off. **This is the maintainer's call (#10), reversible** — chosen
  over re-issuing the server certificate with a ≤398-day span, which would also work but is
  server-side and recurring. It did not touch Windows or Linux, which keep native-tls.

## Known holes / open

- A token glab keeps in the OS keyring (`use_keyring`) is not read; the error names `GITLAB_TOKEN`.
- The macOS pinned-verifier path drops ureq's default connector chain for `TcpConnector` → the
  pinned TLS connector, so it does not go through an HTTP/SOCKS proxy; untested against one (#10).
- native-tls on Linux links OpenSSL; the musl release targets need it vendored (#7).

## Traps

- The GraphQL complexity cap on a self-hosted CE is 250; the level query with 100 items fits.
- The issue search API is rate-limited ("requested with the search argument too many times") and
  matches text anywhere in a body; the walk never uses it.
- ureq reads a body with a size limit by default; the adapter lifts it, since a level of 100 items
  with their linked bodies can be large.
