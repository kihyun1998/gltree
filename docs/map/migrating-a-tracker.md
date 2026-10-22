# Migrating a tracker

How `gltree migrate` turns a tracker written in the first-line format into headers
(`src/migrate.rs`), the one reader of the old format left (ADR-0004).

## Design model

- **The old head** (`read_head`): from the top of a body, every line that is a head line, with
  blank lines between them allowed, up to the first line that is not. A head line is `Parent: #N`
  (exactly one number, nothing after), a from word with one number, a related word, `Blocked by` or
  `Depends on` with one or more numbers (`#a, #b`), or a line whose key, lowercased with spaces as
  `_`, is a declared extra key — taken whole as that key's string. A key line that names no number
  (`Blocked by: none — landed`, `Parent: \`../PRD.md\``) ends the head and stays in the body.
- Text after the numbers becomes the key's note, the whole value as written; two lines of one key
  join their lists and their notes with `; `. A `from` equal to the `parent` keeps the parent and
  moves its note there.
- The body is everything after the last head line, its leading blank lines dropped; the header
  adds one blank line back. What follows is byte for byte the old text.
- **The plan** (`plan`): every head turned into a header over a copy of the tracker, then every
  derived list filled from that copy (`derived::expected` / `rebuilt`), then each reference the old
  tracker lacks a link for, each pair once. Only items whose description changes are listed, so a
  migrated tracker plans nothing.
- `--snapshot` plans from a file and `--out` saves the tracker as it would read afterwards
  (`applied`), so `check --snapshot` judges a migration before anything is written. `--apply`
  writes every description, then every link; a failure says how far it got and to run it again,
  which picks up where it stopped since written items plan nothing.
- The from and related words are options, not repo config: once a tracker is migrated nothing
  reads them again. `From` and `Related` are always known.

## Known holes / open

- A link the old tracker kept between related items with no related line in either head is not
  explained by the migration; `check` reports it as `stray-link`, and the consumer decides each
  one (record it with `edit --add-related`, or remove the link).
- A head line in the middle of a body, below a heading, is not read; `check` does not report it
  either, since `old-header` judges only the first line.
