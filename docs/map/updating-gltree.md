# Updating gltree

## What it is
`gltree update`: the one command that talks to GitHub instead of GitLab. It reads gltree's latest
release, compares it with the running binary, and installs it over the running binary, or hands a
binary a package manager installed back to that manager.

## Governing decisions
- A binary Scoop, Homebrew or cargo installed is not replaced by default; `--force` replaces it in
  place and says the manager still lists the old version (maintainer, 2026-11-24, issue #44).
- On Linux the archive is chosen by architecture alone: a build against glibc is replaced by the
  static musl release too, so a `cargo install` on Linux can update with `--force` and the release
  gate (Ubuntu, `cargo test` without `--target`, so a gnu build) runs the Unix replacement path. This
  departs from issue #44's "OS, architecture and environment" wording (maintainer, 2026-11-25).

## Design model
- **Dispatch**: `Command::Update` sits beside `Serve` outside `InClone` in `src/main.rs`, so it reads
  no repo config, clone, glab login or tracker; `run` prints `update::render`'s lines.
- **Where it reads**: `GET https://api.github.com/repos/kihyun1998/gltree/releases/latest`,
  unauthenticated, with `User-Agent: gltree/<version>` (GitHub's REST API refuses a request with no
  User-Agent) and `Accept: application/vnd.github+json`. `GLTREE_GITHUB_API` (`update::API_ENV`)
  replaces `https://api.github.com`; it is the tests' seam, read nowhere else and not in the help.
  The asset addresses come from the answer's `browser_download_url`; on github.com they answer 302 to
  `release-assets.githubusercontent.com`, which ureq 3.4.2 follows by default (`max_redirects: 10`
  in its `Config::default`, read 2026-11-25).
- **Versions** (`Version`): the crate version compiled in against the tag without its `v`, each read
  as exactly three numbers; a latest that is not greater is "up to date", so an older release is
  never installed. `--check` stops after the read and touches nothing on disk.
- **Order**: read the release; `--check` prints; not newer prints up to date; a managed binary
  without `--force` prints the manager's command; only then is the platform checked, the
  `SHA256SUMS` and archive fetched and the binary replaced. So a managed or up-to-date binary on an
  unsupported platform still answers without an error.
- **Platform** (`target`): the compile-time OS and architecture mapped to the six release targets,
  Linux to the musl ones whatever the C library, Windows only for `msvc`; any other build is an
  error naming `ARCH-OS[-env]`. The archive is
  `gltree-v<version>-<target>.tar.gz`, `.zip` on Windows, and holds `gltree-v<version>-<target>/gltree[.exe]`,
  as [shipping-a-release](shipping-a-release.md) packs it.
- **Checking it**: the archive's SHA-256 (`sha2`) must equal its line in the release's
  `SHA256SUMS` (`sha256sum` lines, `<64 hex>  <name>`); a missing line or a mismatch stops before
  anything on disk changes. The checksum guards a corrupted or swapped download; it comes from the
  same release, so it does not guard against the release itself (provenance is out of scope, #44).
- **Unpacking**: `tar -xf <archive> -C <folder>` in `<temp>/gltree-update-<16 hex>`, removed when
  the command ends (`Scratch`). The name comes from std's randomly seeded `RandomState` and the
  folder is made with `create_dir`, which fails on one that exists (a new name is tried, 16 times):
  on Linux `<temp>` is the shared `/tmp`, and a folder another user made in advance could hold a
  link that the archive write follows or a binary swapped in after the checksum. On Windows `Command::new("tar")` finds `System32\tar.exe` (bsdtar, which
  reads zip) before `PATH`, since std searches the application folder and the system folders first
  (`search_paths` in std's `sys/process/windows.rs`, Rust 1.96, read 2026-11-25): Git's GNU tar on
  `PATH` is not reached. Only the binary is taken, and only when it is a regular file.
- **Replacing**: the binary path is `current_exe` canonicalized, because std says `current_exe` may
  return the symbolic link a binary was started through (Homebrew links `bin/gltree` into the
  Cellar), and renaming over a link would replace the link. The new binary is copied from the
  unpacked folder into `<binary>.new-<pid>`, a file that must not exist yet, with the old file's
  permissions; the process id keeps two `update` runs on one binary from writing one file. On Unix
  that file is renamed over the binary. On Windows a running
  exe can be renamed but not overwritten or deleted: the binary is renamed `<binary>.old`, the new one
  moved into place, and when that move fails the old one is renamed back. `<binary>.old` and every
  `<binary>.new-*` are removed at the start of the next `update` that is not `--check`; a run that
  starts while another is writing can remove the other's file, which then fails to move into place and
  leaves the binary as it was.
- **Managed installs** (`manager_of`): a path holding `apps/gltree` is Scoop, `Cellar/gltree`
  Homebrew (both compared in lower case), a `bin` folder whose parent holds `.crates.toml` cargo
  (cargo writes it in every install root; a bare `bin` is not cargo's).
- **Errors** (`update::Error`, exit 2): `cannot reach <address>: …`; a 403 or 429 with
  `x-ratelimit-remaining: 0` (or a 429 with `retry-after`) says the unauthenticated rate limit is used
  up and the minutes until `x-ratelimit-reset` or `retry-after`; any other non-200 is
  `<address> answered HTTP <status>`; an answer without a `tag_name` in the form, or without
  `assets`, is `the release answer from <address> is malformed: …`.
- Measured on Windows 11, 2026-11-25: a 0.8.0 build of this change replaced itself with the real
  v0.9.0 release (zip fetched through the 302, checksum matched, bsdtar unpacked it); the new binary
  printed `gltree 0.9.0` and `gltree.exe.old` stayed beside it.

## Code
- `src/update.rs — API_ENV, Version, target, archive_name, Manager, manager_of, Report, render, update, latest, get, sum_for, Scratch, unpack, replace, swap, clear_leftovers`
- `src/main.rs — Command::Update, run`
- `tests/cli.rs — github_stand_in, release_stand_in, release_archive, binary_copy, update_with, left_beside, refuse_new_files, and the tests named update_* and a_plain_bin_folder_is_not_taken_for_cargo, a_binary_whose_replacement_cannot_be_written_is_left_as_it_was, a_staging_name_an_earlier_run_left_does_not_block_the_update`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [An error names what it read](an-error-names-what-it-read.md) — every error names the address it
  asked, the release's `SHA256SUMS`, the archive or the file it could not write.

## Blast radius
- [shipping-a-release](shipping-a-release.md) — the archive names, the folder inside each archive
  and the `SHA256SUMS` format are read here: a change there breaks `update` for every installed
  version, with no test here failing.
- [the-command-line-contract](the-command-line-contract.md) — the command, its flags, its text and its
  exit status.

## Known holes / open
- The Windows rollback (moving the new binary into place fails after the old one was renamed) is
  checked by reading; the tests reach the failure before anything moves (a folder that refuses new
  files: an `icacls` deny on Windows, mode 555 on Unix, which a root account would not be held to),
  not this one.
- The Unix path (`swap` under `cfg(not(windows))`, the permission copy kept by the rename) is
  compiled and tested only by the release gate on Ubuntu, not by a local run on Windows; a Linux check here stops in the vendored
  OpenSSL build (perl's `Locale::Maketext::Simple` missing, 2026-11-25). Homebrew's link resolution
  on macOS is not measured.
- Two `update` runs at once are reasoned about, not tested: no test orders two processes' writes
  and renames.
- A Scoop install outside a folder named `apps/gltree`, or a cargo root without `.crates.toml`, reads
  as unmanaged and is replaced.
