# Reading the repo config

## What it is
`.gltree.toml` at a consuming repo's top level: whether the repo uses gltree at all (the opt-in that
every write needs) and which rules it takes — root label, relation scheme, state policy, extra keys,
its own first-line words, and the badges `serve` shows.

## Governing decisions
- [ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md) — the per-repo choices live in that
  repo's committed config file.

## Design model
- Found at `git rev-parse --show-toplevel` of the working directory; outside a clone, or with no file,
  the defaults with `present = false`. Source: `RepoConfig::of_clone`, `toplevel`. So a `--snapshot`
  read outside a clone judges with no `extra_keys`, `old_head_keys`, state policy or badges:
  `check --snapshot tests/fixtures/tracker.json` there adds a `header` finding for #5's `[extra] marker`
  (proving run for #34, 2026-11-20).
- **`present` is the opt-in.** `new`, `edit`, `move`, `migrate --apply` and `sync` without
  `--dry-run` refuse without it (`write::require_opt_in`), before any login, since `main` calls it
  before `server()`. Reading never needs it.
- Every key is optional: `root_label` (default `SPEC`), `relation_scheme` (`front-matter` only),
  `state_policy` (`roots-only` or `every-parent`), `extra_keys` (bare TOML keys), `old_head_keys`,
  `badges`. Source: `RepoConfig::parse`, `SCHEMES`, `POLICIES`.
- **`[[badges]]`** (#27): each `prefix` (not empty) and `name`, an optional `extra` and an optional
  `missing`; a badge takes no other key (`deny_unknown_fields` on `Badge`). An `extra` not in
  `extra_keys` stops every command, not only `serve`, since the check is in `parse`. The key names
  were picked by the worker (2026-11-19); `serve` is the only reader
  ([serving-the-viewer](serving-the-viewer.md)).
- **Unknown keys and values stop every command**, naming the known ones (`#[serde(deny_unknown_fields)]`
  on `File`). So a config still naming retired settings (`origin_word`, `links`) fails, and a gltree
  older than a key a consumer adds fails on that consumer's config — the reason a consumer writes a
  version floor beside its config (read in a consumer's config, 2026-11-19).

- **A refusal and an unreadable file are a `config::Error`**, printed `gltree: .gltree.toml: <reason>`
  and exiting 2, the file name coming from its `Display` (#29). It is the config's own type, not a
  `tracker::Error` variant: reading the config touches no tracker, and its callers in `main`
  (`run_in_clone`, `serve_clone`) return `Box<dyn Error>`, so no conversion is needed (worker's
  judgement, 2026-11-20).
- **A registered repo's config is read from its clone** (#45): `serve` with repos registered reads
  no config of the folder it runs in, and reads each registered clone's through `registered_config`,
  which names a failure by the file's path, `<clone>/.gltree.toml: <reason>`, as a clone's own read names `.gltree.toml` (an `Error::Refused` carrying `config::Error`'s reason, which does not tell an unreadable file from a refused one), and fails that repo's tab
  only, so one broken config does not stop the others (worker's judgement, 2026-11-24, following the
  issue's rule that a missing clone fails its tab only). It is read again with each full read, so a
  config changed while `serve` runs takes effect on the next refresh (#46; when and what keeps it:
  [serving-the-viewer](serving-the-viewer.md)). `serve add`, `serve list` and `serve remove`
  read no config at all. `toplevel` is shared with `repos::clone_at`, which takes a clone's top
  folder as the path it registers.

## Code
- `src/config.rs — FILE, SCHEMES, POLICIES, Error, RepoConfig, Badge, RepoConfig::parse, RepoConfig::scheme, RepoConfig::policy, RepoConfig::of_clone, bare_key, toplevel`
- `src/write.rs — require_opt_in`
- `src/main.rs — run_in_clone` (reads the config once, before the command), `serve_clone`, `registered_config`
- `src/repos.rs — clone_at` (calls `toplevel`)

## Reference behaviour
**None.**

## Cross-cutting invariants
- [An error names what it read](an-error-names-what-it-read.md) — `config::Error` names `.gltree.toml`.

## Blast radius
- [the-command-line-contract](the-command-line-contract.md) — a key added here is a config a consumer may commit, which older binaries refuse; the version floor moves with it.
- [judging-an-items-state](judging-an-items-state.md) — the policy is built here.
- [holding-references-in-the-header](holding-references-in-the-header.md) — `extra_keys` decides which `[extra]` keys `parse` accepts.
- [checking-the-tracker](checking-the-tracker.md) — `old_head_keys` widens `old-header`.
- [serving-the-viewer](serving-the-viewer.md) — `badges` is shown there; a key renamed here is a page that shows nothing.

## Known holes / open
- `root_label` is parsed and defaulted but nothing reads it (`git grep root_label -- src`: only `src/config.rs`) (commit 6be32e0, 2026-10-22, recorded it; still true at HEAD).
- `RepoConfig::scheme` ignores `relation_scheme`, since `front-matter` is the only name `parse` admits; the write path uses `FrontMatter` directly (`src/write.rs`).
