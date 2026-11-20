# Serving the viewer

## What it is
`gltree serve`: a local web server on 127.0.0.1 that serves one page embedded in the binary and a
small JSON API over the tracker — the families on the left, the selected family's tree on the right,
every node linked to the server, a refresh button, and the badges a repo names in its config.

## Governing decisions
**None.** The shape rests on issue #27, confirmed with the maintainer (2026-11-19), and on the
reasons below.

## Design model
- **Read only.** The server calls only the tracker port's reads: `all_items` through `tree::tree`,
  and `level` through `tree::live_tree` (`App::refresh`, `App::tree_of`). Nothing on the page
  writes (issue #27, out of scope).
- **Two reads.** The full read (`tree::tree` with no root, every family, finished ones included)
  happens once at start, before the address is printed, and again on `POST /api/refresh`; its answer
  is kept and `GET /api/families` serves the kept one. `GET /api/tree/N` reads N's tree fresh: against
  the server it walks from N (`walk`, as `tree N` does), from a snapshot it reads the file again. A
  full read of a tracker of a few thousand items took about 45 s (maintainer, measured, 2026-11-19),
  so the page shows the kept read at once and swaps in the walked tree when it arrives.
- **The tracker is opened per read** through `serve::Open`, a closure `main` builds over `source`,
  the same opener every other command uses: a `SnapshotFile` for `--snapshot`, otherwise `server()`,
  with a failure to open kept as the tracker's own error — so a refresh after a new login picks it up,
  and `App` holds no tracker across threads. The token lives inside `GitLab` and no response carries
  anything but tree JSON and error messages (`App::handle`).
- **No async runtime and no new dependency** (issue #27: smallest footprint, matching the blocking
  HTTP client). `std::net::TcpListener` bound to `Ipv4Addr::LOCALHOST` (`bind`); each connection on
  its own thread (`serve`), so a 45 s refresh or a browser's idle pre-opened socket does not stall the
  others; a 10 s read timeout ends an idle one. The head is read to 16 KiB, a body to 64 KiB and
  discarded; every answer closes the connection (`answer`, `parse_head`, `write`). HEAD answers as
  GET without the body, and a 405 names its `Allow` (RFC 9110). A failed accept waits 100 ms before the
  next, so a lasting error (no file handles left) does not spin a core. A judgement over a small server
  crate (reading, 2026-11-19): the four routes need no more.
- **Refreshes run one at a time** (`App::reading`): a second waits for the first and then reads
  again, so a read that started earlier can never be the one kept. `read_at` is when the kept read
  finished.
- **Who may ask.** A request whose `Host` is not `127.0.0.1` or `localhost` at the port (any case;
  no port means 80, as browsers send it) is refused (403) before anything is read, which stops a
  website from reading the tracker through DNS rebinding; a head with two `Host` lines is refused
  (400). Under `/api/`, a request a browser marks `Sec-Fetch-Site` other than `same-origin` or
  `none` is refused, so another site's page cannot make the server walk the tracker with the user's
  token; a refresh whose `Origin` is another site's is refused too, for a browser that sends no
  `Sec-Fetch-Site`. A request with neither header (curl, a script on the machine) is served
  (`App::ours`, `App::handle`). Every answer sets `no-store`, `nosniff`,
  `no-referrer` and a CSP allowing only the page's inline script and style and `connect-src 'self'`.
- **JSON**: `/api/families` and errors carry `serve::JSON_VERSION` 1 —
  `{version, read_at, badges, tree}` and `{version, error}`; `tree` and `/api/tree/N` are
  `tree::to_json` at its own version ([drawing-a-tree](drawing-a-tree.md)). `read_at` is UTC,
  written by `utc` with no date crate. Status: 404 for an unknown path or number, 405 for a known path
  with another method, 400 / 413 for a head or body it will not read, 403 as above, 502 for a read
  that failed, with the tracker's message.
- **Default port 7420**, a free pick (worker's judgement, 2026-11-19); `--port 0` takes a free one and
  the printed address says which.
- **The page** (`src/serve.html`, `include_str!` into `PAGE`): hand-written HTML, CSS and JS, no
  framework, no build step, and no outside address in it (a unit test refuses `://`). Its layout
  follows the Split prototype the maintainer chose (2026-11-19). It applies the finished-family
  toggle itself with `tree`'s rule — a root closed with `closed == total` — and computes badges,
  filters and counts from `labels`, `extra` and the config's `badges`. The toggle is the "All"
  filter's own test and governs the "All" list only. Every badge filter's values and list, every
  filter button's count and every hero number (families, open items, each badge's `missing`) span
  all families, finished ones included: `missing` only ever marks a closed root, which the toggle
  would mostly hide, and the maintainer chose the same for value filters (issue #27, user story 15;
  maintainer, 2026-11-19) and for the hero, so that no two of its numbers count different sets
  (maintainer, 2026-11-19). While a badge filter is chosen the toggle would change nothing, so it is
  dimmed and cannot be operated (`aria-disabled`, out of the tab order, a title saying badge
  filters list every family); choosing "All" brings it back (maintainer, 2026-11-19). A filter that
  matches no family says so in the list and the detail; "None" and "Nothing left to do" stay for an
  empty "All" list, where they are true (maintainer, 2026-11-19). A chosen filter is held by its
  badge and value, not its place in the list, since the buttons are drawn again on every filter,
  toggle and refresh. Open
  blockers show on every node, the root in the list and the detail included. Counting motion is off
  under reduced motion, as the fades are.
- **How the page moves** (maintainer, 2026-11-20, choosing variant D of four after the earlier motion
  was too strong to look at and a version with almost no fade flickered). Content moves by opacity only:
  nothing slides, rises, scales, overshoots or lifts on hover. Content enters by a plain 0.22 s fade (the
  CSS class `enter`); a list or a tree fades in as one block, with no per-row delay. The detail and the
  list rows (as one block) change by a crossfade (`fade()`, which also gives the new content its
  `enter`): the old node itself, not a copy, becomes the ghost; it is taken out of the flow over the same
  place, held to both side edges of its container so that it keeps the new content's width when a
  scrollbar comes or goes, and fades out in 0.22 s while the new content fades in under it, so the place never goes blank.
  The ghost loses its id and is `inert`, so it takes no pointer events and is hidden from assistive
  technology; it starts at the opacity the old content showed at that moment (a fade-in cut short does
  not pop back to full), and removes itself when its own fade ends or is cancelled, as when reduced
  motion turns on mid-fade. Three things that are not content keep a motion of their own: the
  highlight moves to the selected row in 0.22 s and the ring fills in 0.4 s, both on the page's `--ease`
  curve, which does not overshoot, and the hero counts up in 0.3 s. The switch knob slides between its
  two sides on the same curve, since its side is the switch's state (worker's judgement, 2026-11-20),
  and the refresh button's busy icon spins. Under reduced motion every fade, crossfade, highlight move,
  ring fill, count and the spin is off or instant, except the knob's slide: the CSS rule that turns
  motion off selects `*`, which does not match the knob's `::after` (a gap held for the maintainer).
- **What each act draws** (maintainer, 2026-11-19: selecting a family must not stutter; a detail
  drawn a second time when the walk's tree arrives plays its entrance and ring again). The first read
  and a refresh draw the whole page (`paint`): the hero counts up, and the list and the detail fade in.
  A filter or the toggle draws the filter buttons again and crossfades the list rows when their markup
  changed (`relist`): the hero counts every family, so neither changes it, and the highlight stays the
  same element and moves to the selected row. A selected family that stays in the list keeps its detail
  as it is, a stale note included, and is not read again. A list that drops it selects its first family
  and crossfades the detail as `choose` does; a list left empty whose message changes crossfades the
  detail to the new message. A filter whose rows and detail markup come out the same changes nothing.
  Selecting a family moves the highlight and crossfades the detail, its ring filling once (`choose`).
  The tree `/api/tree/N` brings replaces the detail's contents in place (`draw`), keeping the detail
  element, so a fade-in still running carries on uninterrupted and the new ring starts from the offset
  the old one showed. A tree whose markup is the same as what is shown changes nothing. Clicking the
  selected family again reads `/api/tree/N` again and puts it in place through `draw`, with no fade
  (worker's judgement, 2026-11-19).
- **Every value the page renders is escaped** (`esc`: `& < > " '`) and **a link is made only from an
  `http:` or `https:` URL** (`safeUrl`); any other `url`, `null` included, renders the node
  without a link. `tree --json` passes `url`, labels and `[extra]` through as the tracker holds them
  (#26's review, comment on #27), so the page is where they meet HTML.
- **Badges** come from the repo config ([reading-the-repo-config](reading-the-repo-config.md)) and
  apply to roots only: the first label starting with `prefix` gives the value (the label's rest); a
  closed root with none shows `missing` when the badge sets it. gltree knows no badge by name, which
  keeps [listing-roots](listing-roots.md)' boundary: label meanings are the caller's.

## Code
- `src/serve.rs — JSON_VERSION, DEFAULT_PORT, PAGE, Open, App, App::new, App::refresh, App::tree_of, App::ours, App::handle, Request, Head, Status, Response, utc, bind, serve, answer, parse_head, write`
- `src/config.rs — Badge` (serialised as it is into `/api/families`' `badges`)
- `src/tracker.rs — FakeTracker::fail_all_items` (the unit test that a walk reads no full tracker)
- `src/serve.html`
- `src/main.rs — Command::Serve, source, server`
- `tests/cli.rs — serve_in, serve_that_stops, the serve_* tests`

## Reference behaviour
**None.**

## Cross-cutting invariants
- [One rule for a finished family](one-rule-for-a-finished-family.md) — the page's `finished` is a copy of `tree::with_open_work`.
- [Not recorded is not none](not-recorded-is-not-none.md) — `labels` read as `[]` from an old snapshot gives every closed root `missing`.

## Blast radius
- [drawing-a-tree](drawing-a-tree.md) — the page reads `tree`'s JSON node; a field renamed there breaks the page with no Rust test failing, and a changed finished rule must change `finished` in `src/serve.html` too.
- [reading-the-repo-config](reading-the-repo-config.md) — `badges` is read there and checked against `extra_keys`.
- [the-command-line-contract](the-command-line-contract.md) — a new command, its flags and the API's JSON.
- [reading-the-server](reading-the-server.md) — a refresh costs a full read; selecting a family costs a walk.

## Known holes / open
- The page's behaviour (selection, filters, refresh states, escaping, what each act animates) has no test in the repository; it was checked with throwaway Node harnesses, in headless Edge over CDP, and by reading (2026-11-19; the motion again 2026-11-20). The repository runs no JavaScript.
- `missing` counts every root `tree` draws, a closed item whose parent is absent included, while `roots` does not count that one; and a snapshot saved before labels were kept reads every label list as `[]`, so every closed root shows `missing` ([not-recorded-is-not-none](not-recorded-is-not-none.md)). Open for the maintainer (2026-11-19).
