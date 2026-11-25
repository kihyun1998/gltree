# The command-line contract

## What it is
What a caller can rely on when it runs the gltree binary: the commands and flags, the exit status,
the text lines and the versioned JSON shapes, and the repo config a consumer commits. gltree is used
as a binary; its callers are consuming repositories and outside skills that parse its output.

## Governing decisions
**None.** [ADR-0002](../adr/0002-gltree-owns-the-tree-convention.md) makes gltree the convention's one
implementation, which is why its output is what callers read; it does not decide the output.

## Design model
- **Exit status**: 0 on success; 1 when `check` finds anything; 2 on a usage, input, config or server
  error, and on a write left `Unfinished` (`main`, `run`).
- **The source**: `tree`, `roots`, `check`, `show`, `migrate` and `serve` take `--snapshot F` instead of the server (`source`); `sync --dry-run` and `snapshot` always read the server; `tree N`
  without it walks, `tree` without a number reads every item. `serve` takes it too, and its refresh reads the file again.
- **`tree` without a number leaves out families whose every item is closed**; `--all` draws every
  family and is a usage error (exit 2) beside a number. Leaving families out gives fewer `roots`, not
  another shape; `tree N --json`, which outside skills read, draws as before (#25).
- **JSON shapes carry a version**, raised when the shape changes — a field added, removed or renamed; fewer items is not a shape change (#25): `tree` 4 (`related`, `from` and `blocks` on every node, #47; 3 added `url`, `labels` and `extra`, #26), `check` 1,
  `roots` 2, `new` 2, `move` / `edit` 1, `sync` 1, `migrate` 1, `show` 1, `serve`'s API 4, raised with the tree it carries (#47; `/api/item/N` came at 4 with #48, a new route and no changed shape) (each module's `JSON_VERSION`,
  `NEW_JSON_VERSION`). The shapes are in the README and in each owning note.
- **`serve`** listens on 127.0.0.1 only, port 7420 unless `--port` (0 takes a free one), and prints
  `http://127.0.0.1:<port>/` before its first full read, which runs in the background; what stops it
  with exit 2 before the address, and why a failed first read does not, and its six routes and their
  JSON are in [serving-the-viewer](serving-the-viewer.md).
- **`serve` is the one command that can run outside a clone on purpose** (#45, maintainer,
  2026-11-24): with repos registered in `~/.gltree/repos.toml` it shows each of them wherever it
  runs, and `serve add PATH [--name N]`, `serve list` and `serve remove N` keep that list and run
  anywhere. They are subcommands of `serve` (`ServeList`, `args_conflicts_with_subcommands`, so
  `serve list --port 1` is a usage error), and `gltree serve` with no subcommand still runs the
  server, with no `serve run`. Every other command takes its tracker from the clone it runs in, as
  before: `run` dispatches `serve` to `serve_command` and the rest, a flattened `InClone`, to
  `run_in_clone`, which reads the repo config of the folder it runs in once, before the command.
  `serve add` prints `registered NAME: PATH`, `serve list` one `NAME  PATH` line per repo and nothing
  for none, `serve remove` `removed NAME: PATH`; each exits 2 with the reason on a refusal.
- **Item numbers** are accepted as `N` or `#N` (`parse_iid`).
- **The help is the first-time user's manual** (#34): `-h` gives each command's one-line `about` and
  every option's short `help`; `--help` adds the `long_about`, each option's `long_help` where the
  detail does not fit, and an `after_long_help` holding Examples, Output, Requirements and Exit
  status. The Requirements and Exit status paragraphs are `macro_rules!` literals in `main.rs`
  (`requires_clone`, `reads_a_snapshot`, `reads_the_whole_tracker`, `writes_need_the_config`,
  `reads_the_config`, `exit_status`, `exit_status_of_a_write`), since `concat!` takes literals
  only; `check` and `serve` state their own exit status, and a few command-specific lines sit beside
  the macros. `exit_status_of_a_write` goes on the commands that return `Error::Unfinished`
  (`new`, `edit`, `move`, `migrate`), not on `sync`, whose failed write is a plain error.
  clap here has no `wrap_help` feature, so every rendered help line but an example is kept within
  80 columns by hand. The help says "item", as GLOSSARY.md does, and glosses header, family, state
  policy and `[extra]` where a command's help uses them. `bin_name = "gltree"` keeps the usage line
  from naming `gltree.exe` on Windows (clap takes argv[0] only when `bin_name` is unset).
- **`--snapshot` outside a clone runs with the default repo config**, as [reading-the-repo-config](reading-the-repo-config.md) says; the help tells people to run it in the clone all the same.
- **Tests that hold the help**: every command has an about, a long description, the four sections
  and at least two examples that parse with clap (one for a command that takes no argument, such as
  `serve list`), and every argument a help line
  (`every_command_explains_itself_with_examples_that_parse`); every line but an example fits 80
  columns (`every_help_line_but_an_example_fits_80_columns`); both tests walk `serve`'s subcommands
  too; `tests/cli.rs` reads `--help` and `-h`
  for every command.
- **What callers are known to read** (read in the callers, 2026-11-19; not named here, maintainer,
  2026-11-19): consuming repositories commit `.gltree.toml` with `state_policy` and `extra_keys` and
  state a minimum gltree version beside it; their docs tell people to create and change items only
  through `new`, `edit`, `move` and `sync`. Outside skills run `roots --json` with
  `--closed-since` / `--closed-until` / `--without-label` and read `extra` for a registration marker,
  run `tree N --json` for every descendant's number, title and state, and write the marker with
  `edit --set-extra`.

## Code
- `src/main.rs — Cli, Command, InClone, ServeList, parse_iid, parse_extra, unreadable_dir, current_dir, server, server_in, source, main, run, run_in_clone, serve_command`
- `src/main.rs — tests::every_command_explains_itself_with_examples_that_parse, tests::every_help_line_but_an_example_fits_80_columns, tests::the_usage_line_names_gltree_on_every_platform`
- `tests/cli.rs`
- `README.md` (the user-facing shapes)

## Reference behaviour
**None.**

## Cross-cutting invariants
- [An error names what it read](an-error-names-what-it-read.md) — `current_dir` names the current directory when it cannot be read.

## Blast radius
- [listing-roots](listing-roots.md) — `extra`, the label filters and the date filters are read outside; renaming one breaks a caller with no test here failing.
- [drawing-a-tree](drawing-a-tree.md) — `tree --json` fields are read outside.
- [writing-the-tree](writing-the-tree.md) — `edit --set-extra` is how outside skills write their marker.
- [reading-the-repo-config](reading-the-repo-config.md) — a config key added means consumers raise their minimum version.
- [serving-the-viewer](serving-the-viewer.md) — the API's JSON is part of the contract: `/api/families` embeds `tree --all --json`.
- [shipping-a-release](shipping-a-release.md) — a changed shape reaches callers only through a release, and the version they install.

## Known holes / open
- Shapes are tested (`show_as_json_carries_the_same_fields` in `tests/cli.rs`; `json_carries_from_and_extra_and_the_first_line_after_the_header` in `src/roots.rs`), but a test is edited along with a rename: nothing here marks which fields are read outside, besides this note.
