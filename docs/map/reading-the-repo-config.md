# Reading the repo config

## What it is
`.gltree.toml` at a consuming repo's top level: whether the repo uses gltree at all (the opt-in that
every write needs) and which rules it takes — root label, relation scheme, state policy, extra keys,
its own first-line words.

## Governing decisions
- [ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md) — the per-repo choices live in that
  repo's committed config file.

## Design model
- Found at `git rev-parse --show-toplevel` of the working directory; outside a clone, or with no file,
  the defaults with `present = false`. Source: `RepoConfig::of_clone`, `toplevel`.
- **`present` is the opt-in.** `new`, `edit`, `move`, `migrate --apply` and `sync` without
  `--dry-run` refuse without it (`write::require_opt_in`), before any login, since `main` calls it
  before `server()`. Reading never needs it.
- Every key is optional: `root_label` (default `SPEC`), `relation_scheme` (`front-matter` only),
  `state_policy` (`roots-only` or `every-parent`), `extra_keys` (bare TOML keys), `old_head_keys`.
  Source: `RepoConfig::parse`, `SCHEMES`, `POLICIES`.
- **Unknown keys and values stop every command**, naming the known ones (`#[serde(deny_unknown_fields)]`
  on `File`). So a config still naming retired settings (`origin_word`, `links`) fails, and a gltree
  older than a key a consumer adds fails on that consumer's config — the reason a consumer writes a
  version floor beside its config (read in a consumer's config, 2026-11-19).

## Code
- `src/config.rs — FILE, SCHEMES, POLICIES, RepoConfig, RepoConfig::parse, RepoConfig::scheme, RepoConfig::policy, RepoConfig::of_clone, bare_key, toplevel`
- `src/write.rs — require_opt_in`
- `src/main.rs — run` (reads the config once, before the command)

## Reference behaviour
**None.**

## Cross-cutting invariants
**None.**

## Blast radius
- [the-command-line-contract](the-command-line-contract.md) — a key added here is a config a consumer may commit, which older binaries refuse; the version floor moves with it.
- [judging-an-items-state](judging-an-items-state.md) — the policy is built here.
- [holding-references-in-the-header](holding-references-in-the-header.md) — `extra_keys` decides which `[extra]` keys `parse` accepts.
- [checking-the-tracker](checking-the-tracker.md) — `old_head_keys` widens `old-header`.

## Known holes / open
- `root_label` is parsed and defaulted but nothing reads it (`git grep root_label -- src`: only `src/config.rs`) (commit 6be32e0, 2026-10-22, recorded it; still true at HEAD).
- `RepoConfig::scheme` ignores `relation_scheme`, since `front-matter` is the only name `parse` admits; the write path uses `FrontMatter` directly (`src/write.rs`).
