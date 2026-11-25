# Shipping a release

## What it is
How a gltree version reaches the machines that run it: a `v*` tag gates, builds six targets and
publishes archives with checksums and provenance on GitHub; Homebrew and Scoop install from those
archives.

## Governing decisions
**None.**

## Design model
- **Trigger**: a `v*` tag runs gate, build and release; a push to `release-check/**` runs gate and build
  and publishes nothing (`.github/workflows/release.yml`).
- **Gate**: on a tag, the tag must equal `v<Cargo.toml version>`; then `cargo fmt --check`,
  `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` on Ubuntu — which is
  where the Unix-only tests (file modes) run.
- **Build**: Linux musl, macOS and Windows on x64 and arm64, each archived as
  `gltree-<version>-<target>` with `README.md` and both licence files; each native binary runs
  `--version` once (not `x86_64-apple-darwin` or `aarch64-pc-windows-msvc`, which the runners cannot
  execute natively). The musl targets need ureq's `vendored` OpenSSL (commit e5fe873, 2026-10-22).
- **Release**: `SHA256SUMS`, build provenance (`actions/attest-build-provenance`), `gh release create
  --generate-notes`. No third-party release tool; every action pinned by commit SHA; `contents: read`
  except the release job (workflow header and permissions).
- **Distribution**: `brew tap kihyun1998/tap` plus `brew trust`, `scoop bucket add kihyun1998`, an
  archive, or `cargo install --git` (README). The tap and bucket pick up a release on their own side,
  on a schedule that can lag a release by hours (0.7.1, 2026-11-24); `gltree update` reads the
  release itself ([updating-gltree](updating-gltree.md)).
- **The archive name carries the tag**: `VERSION` in the build job is `github.ref_name`, so an archive
  is `gltree-v<version>-<target>.tar.gz` / `.zip` with one folder of that name inside, and
  `SHA256SUMS` is `sha256sum *` output. `gltree update` reads all three.
- The crate is MIT OR Apache-2.0 (`Cargo.toml`, `LICENSE-MIT`, `LICENSE-APACHE`).
- Mutation-testing a copy of the crate: a copy keeps file mtimes, so cargo reuses the previous
  mutation's build and the results lie; touch every file after copying (commit 2a78fa5, 2026-10-21).

## Code
- `.github/workflows/release.yml — gate, build, release`
- `Cargo.toml — version, description, license, dependencies`; `Cargo.lock`
- `LICENSE-MIT`, `LICENSE-APACHE`, `README.md` (Install)

## Reference behaviour
- The maintainer's `just-shield` release workflow, `homebrew-tap` and `scoop-bucket` on GitHub: an
  example for the pipeline (thegraph build, [docs/agents/thegraph.md](../agents/thegraph.md)).

## Cross-cutting invariants
**None.**

## Blast radius
- [the-command-line-contract](the-command-line-contract.md) — callers run the version they installed; a new config key or JSON version needs a release before a consumer can raise its floor.
- [reading-the-server](reading-the-server.md) — a TLS change must build on every target; the macOS path compiles only on macOS runners.
- [updating-gltree](updating-gltree.md) — renaming an archive, its folder, the binary inside or `SHA256SUMS`, or adding or dropping a target, breaks `gltree update` in every version already installed.

## Known holes / open
- The tap's and bucket's auto-update live in those repositories; nothing here checks that a release reached them.
- No CI runs on a plain push or pull request: the gate runs only on a tag or a `release-check/**` branch.
