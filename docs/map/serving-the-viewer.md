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
  and `level` through `tree::live_tree` (`App::read_all`, `App::tree_of`). Nothing on the page
  writes (issue #27, out of scope).
- **Two reads.** The full read (`tree::tree` with no root, every family, finished ones included)
  starts once the address is printed, on a thread of its own (`App::start`), and again on
  `POST /api/refresh`; its answer is kept and `GET /api/families` serves the kept one. `GET /api/tree/N` reads N's tree fresh: against
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
- **The address comes first, the first read after it** (issue #35, confirmed with the maintainer,
  2026-11-23; it reverses #27's "the first full read happens at start, before the address is printed",
  maintainer, 2026-11-20). On a tracker of 2,803 items on a self-hosted CE the old order showed nothing
  in the terminal for about a minute (maintainer, measured, 2026-11-20). Before the address, `main`
  calls `source` once: for the server that reads git's `origin`, the glab login and the CA certificate
  with no network; for `--snapshot` it opens the file and refuses a directory (`SnapshotFile::open`; on Linux and
  macOS opening a directory succeeds, so the open alone would let one through). Either failing exits
  2 with no address, as a port in use or a config error does. The tracker `source` opened is the one
  the first read reads (`App::start` takes it), so nothing is opened twice; later reads open their
  own through `Open`. The terminal says on stderr when the first
  read starts and when it ends, with the number of families and the time, or why it failed; stdout
  carries the address only, since callers read its first line.
- **One read at a time; a refresh during one starts nothing** (issue #35). `App::begin` marks a read
  as running, and `App::start` marks it before returning, so a refresh that arrives before the
  thread runs cannot start a second one. `POST /api/refresh` while a read runs answers the
  `/api/families` shape at once with `state` `reading`; otherwise it reads on the request's thread
  and answers when the read ends (worker's judgement, 2026-11-23: the brief changed only the refresh
  that meets a running read). A read that panics is caught (`App::finish`) and kept as a failed read
  naming the panic, so `reading` is always released and the next refresh reads again.
  `read_at` is when the kept read finished.
- **`state`** (`App::families_of`, worker's judgement, 2026-11-23, derived from what the page must
  show): `reading` while any read runs, the first or a refresh; `failed` when the last read failed,
  with `error` its message; `ready` when it succeeded. `tree` and `read_at` are the last read that
  succeeded, `null` until one has, and stay through a later refresh and a failed one: a page opened
  during a refresh shows the kept families at once, and one opened after a failed refresh shows them
  with the failure. A failed refresh answers 502 to the request that asked.
- **Who may ask.** A request whose `Host` is not `127.0.0.1` or `localhost` at the port (any case;
  no port means 80, as browsers send it) is refused (403) before anything is read, which stops a
  website from reading the tracker through DNS rebinding; a head with two `Host` lines is refused
  (400). Under `/api/`, a request a browser marks `Sec-Fetch-Site` other than `same-origin` or
  `none` is refused, so another site's page cannot make the server walk the tracker with the user's
  token; a refresh whose `Origin` is another site's is refused too, for a browser that sends no
  `Sec-Fetch-Site`. A request with neither header (curl, a script on the machine) is served
  (`App::ours`, `App::handle`). Every answer sets `no-store`, `nosniff`,
  `no-referrer` and a CSP allowing only the page's inline script and style and `connect-src 'self'`.
- **JSON**: `/api/families` and errors carry `serve::JSON_VERSION` 2 — `{version, state, error, read_at, badges, tree}`
  and `{version, error}`; `tree` and `/api/tree/N` are
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
  badge and value, not its place in the list, since a refresh draws the buttons again from the new
  read, where the list can differ. While `/api/families` answers `reading` the page asks again every
  second (`take`, `load`; worker's judgement, 2026-11-23) and shows the refresh button's busy state; the
  list area keeps "Reading…" until there is a kept read. When the read ends, `ready` paints the page
  and flashes "Up to date", as a refresh does; `failed` shows its `error` in the banner through
  `fail`, "The read failed" with no kept read and "The refresh failed" with one, and the button can
  read again. A refresh answered `reading` takes the same path. Open
  blockers show on every node, the root in the list and the detail included. Counting motion is off
  under reduced motion, as the fades are.
- **The layout fills the window and the page does not scroll** (maintainer, 2026-11-23, choosing variant
  A2 of four in a prototype run against 150 synthetic families and an 86-item family; issue #38). The top
  bar and the hero stay; the family list and the detail share the rest of the window and each scroll on
  their own, so a family chosen far down the list shows the top of its detail, and scrolling a long detail
  leaves the list where it was. The body is a column that is the window's height and does not scroll;
  `main`, `#m` and the split each take what is left (`flex:1` with `min-height:0`, without which a
  flex item grows to its content); the split's one row is `minmax(0,1fr)`, which the list's column and
  the detail fill by the grid's stretch, the column with `min-height:0` so that `.list` (`flex:1`) takes
  what the filters leave; `.list` and `.detail` are the two scroll containers. The banner, "Reading…", an empty list and a filter
  that matches nothing lay out inside the same column. A new selection gives the detail a new element
  (`fade`), so it starts at its top; a walk's tree drawn in place (`draw`) keeps the detail's scroll,
  and a refresh that leaves the same family selected gives the new detail the scroll the old one had
  (`paint`; the maintainer, 2026-11-23, at #38's review: a refresh used to keep the page's scroll). A
  refresh that selects another family starts it at its top.
  The selected row is brought into view inside the list, nearest edge and with no motion (`into`, from
  `moveHl`), whenever the list is drawn or the selection moves: a refresh draws the list again at its
  top, and a filter or the toggle can leave the selected row outside the part shown. A resize moves the
  highlight only, so it does not take the list away from where the reader scrolled it. A row reached by
  Tab is brought into view by the browser's own focus scrolling. Only desktop full-screen windows were
  judged; narrow windows are out of scope (maintainer, 2026-11-23). The page declares
  `color-scheme: light dark`, so under a dark system theme the two scrollbars and other native controls
  are dark too. The highlight and the list's crossfade ghost are positioned against `.list`, their
  containing block, so they scroll with the rows. The detail's ghost is positioned against the split,
  which does not scroll; it is the old `.detail` itself, a scroll container of its own, and keeps the
  scroll it had while it fades.
- **How the page moves** (maintainer, 2026-11-20, choosing variant D of four after the earlier motion
  was too strong to look at and a version with almost no fade flickered). Content moves by opacity only:
  nothing slides, rises, scales, overshoots or lifts on hover. Content enters by a plain 0.22 s fade (the
  CSS class `enter`); a list or a tree fades in as one block, with no per-row delay. The detail and the
  list rows (as one block) change by a crossfade (`fade()`, which also gives the new content its
  `enter`): the old node itself, not a copy, becomes the ghost; it is taken out of the flow over the same
  place, held to both side edges of its container so that it keeps the new content's width when a
  scrollbar comes or goes, and to the height it had, since the detail stretches to its grid row and a
  ghost out of the flow would otherwise shrink to its content (#38), and fades out in 0.22 s while the new content fades in under it, so the place never goes blank.
  The ghost loses its id and is `inert`, so it takes no pointer events and is hidden from assistive
  technology; it starts at the opacity the old content showed at that moment (a fade-in cut short does
  not pop back to full), and removes itself when its own fade ends or is cancelled, as when reduced
  motion turns on mid-fade. Three things that are not content keep a motion of their own: the
  highlight moves to the selected row in 0.22 s and the ring fills in 0.4 s, both on the page's `--ease`
  curve, which does not overshoot, and the hero counts up in 0.3 s. The switch knob slides between its
  two sides on the same curve, since its side is the switch's state (worker's judgement, 2026-11-20),
  and the refresh button's busy icon spins. Nothing else moves: a filter button changes to its chosen
  look at once, with no transition, since the buttons stay in place and a transition on them would be
  a motion outside this list (worker's judgement, 2026-11-23). Under reduced motion every fade,
  crossfade, highlight move, ring fill, count, the spin and the knob's slide is off or instant: the
  CSS rule that turns motion off selects `*`, `::before` and `::after`, since `*` alone does not
  match pseudo-elements and the knob is the switch's `::after` (checked in headless Edge with reduced
  motion emulated, 2026-11-23: no element or pseudo-element keeps a transition or an animation, and
  toggling the switch starts none).
- **What each act draws** (maintainer, 2026-11-19: selecting a family must not stutter; a detail
  drawn a second time when the walk's tree arrives plays its entrance and ring again). The first read
  and a refresh draw the whole page (`paint`): the hero counts up, and the list and the detail fade in.
  A filter or the toggle (`relist`) marks the chosen filter button in place (`sync`), keeping every
  button element, so a filter chosen by keyboard keeps focus on its button and the next Tab moves on
  from there. Marking in place holds because `relist` works on the same read as the `paint` that
  drew the bar, so the filters come out the same and in the same order; only `paint` follows a new
  read, and it draws the bar whole. `relist` crossfades the list rows when their markup changed; the
  hero counts every family, so neither act changes it, and the highlight stays the same element and
  moves to the selected row. A selected family that stays in the list keeps its detail as it is, a
  stale note included, and is not read again. A list that drops it selects its first family and
  crossfades the detail as `choose` does; a list left empty whose message changes crossfades the
  detail to the new message. A filter whose rows and detail markup come out the same changes nothing.
  Selecting a family moves the highlight and crossfades the detail, its ring filling once (`choose`).
  The tree `/api/tree/N` brings replaces the detail's contents in place (`draw`), keeping the detail
  element, so a fade-in still running carries on uninterrupted and the new ring starts from the offset
  the old one showed. A tree whose markup is the same as what is shown changes nothing; on a live
  self-hosted tracker the walk and the full read gave the same JSON for families of 1, 2, 5 and 54
  items, so a selection there takes this path (read-only run, 2026-11-20). Clicking the
  selected family again reads `/api/tree/N` again and puts it in place through `draw`, with no fade
  (worker's judgement, 2026-11-19).
- **Only the latest tree read is drawn** (issue #30, 2026-11-23). Each `/api/tree/N` read takes a number from
  `want` (`fresh`), and `choose` takes one too, so a change of selection, a selection left empty, and
  the repaint after a full read each make every read still out of date. An answer, a success or a
  failure, whose number is not the latest is dropped: it neither draws nor is remembered. So of two
  quick clicks on the selected family only the second answer is drawn, whatever order they arrive
  in, and a walk sent before a refresh that answers after it leaves the refreshed page as it is. A walk
  that answers while a refresh still runs is drawn, since the page has not changed yet.
- **A failed re-read shows the newer of the last full read and the family's last walk** (maintainer,
  2026-11-23, at #30's confirm stop; the issue left the choice open). The page keeps each family's
  last successful walk and its time in `walks`, and empties it when a full read (the first read or a
  refresh) succeeds, so a kept walk is always newer than the kept full read. A failure draws the kept
  walk with "Showing the read of this family at HH:MM:SS" and the error, or the full read with
  "Showing the last full read" and the error. Selecting a family still shows the full read first, as
  before; the kept walk is used only after a failure.
- **The refresh button's states.** Every change of the button, the "Up to date" flash's `ok` class
  included, goes through `button`, which cancels the flash's 1.4 s timer (`flash`) first, so a refresh
  started within those 1.4 s keeps its busy state (issue #30, 2026-11-23). Once the button is busy it
  refuses a click, and only `take` with `reading` schedules the next `load`, so from then on one
  `/api/families` chain runs. Before the page's first `/api/families` answers, the button is not yet
  busy, so a refresh clicked then starts a second chain (see Known holes).
- **Every value the page renders is escaped** (`esc`: `& < > " '`) and **a link is made only from an
  `http:` or `https:` URL** (`safeUrl`); any other `url`, `null` included, renders the node
  without a link. `tree --json` passes `url`, labels and `[extra]` through as the tracker holds them
  (#26's review, comment on #27), so the page is where they meet HTML.
- **Badges** come from the repo config ([reading-the-repo-config](reading-the-repo-config.md)) and
  apply to roots only: the first label starting with `prefix` gives the value (the label's rest); a
  closed root with none shows `missing` when the badge sets it. gltree knows no badge by name, which
  keeps [listing-roots](listing-roots.md)' boundary: label meanings are the caller's.

## Code
- `src/serve.rs — JSON_VERSION, DEFAULT_PORT, PAGE, Open, App, Kept, App::new, App::start, App::refresh, App::begin, App::finish, App::read_all, App::families_of, App::tree_of, App::ours, App::handle, Request, Head, Status, Response, utc, bind, serve, answer, parse_head, write`
- `src/snapshot.rs — SnapshotFile::open` (the check before the address, a directory refused)
- `src/config.rs — Badge` (serialised as it is into `/api/families`' `badges`)
- `src/tracker.rs — FakeTracker::fail_all_items` (the unit test that a walk reads no full tracker)
- `src/serve.html`
- `src/main.rs — Command::Serve, source, server`
- `tests/cli.rs — serve_in, read_families, serve_that_stops, the serve_* tests`; `serve_prints_its_address_while_the_first_read_still_waits_on_the_server` holds the first read behind a local listener that never answers, since a snapshot's read ends too fast to show the order

## Reference behaviour
**None.**

## Cross-cutting invariants
- [A paged read is whole or fails](a-paged-read-is-whole-or-fails.md) — the badges read every label, or the read fails.
- [One rule for a finished family](one-rule-for-a-finished-family.md) — the page's `finished` is a copy of `tree::with_open_work`.
- [Not recorded is not none](not-recorded-is-not-none.md) — `labels` read as `[]` from an old snapshot gives every closed root `missing`.

## Blast radius
- [drawing-a-tree](drawing-a-tree.md) — the page reads `tree`'s JSON node; a field renamed there breaks the page with no Rust test failing, and a changed finished rule must change `finished` in `src/serve.html` too.
- [reading-the-repo-config](reading-the-repo-config.md) — `badges` is read there and checked against `extra_keys`.
- [the-command-line-contract](the-command-line-contract.md) — a new command, its flags and the API's JSON.
- [reading-the-server](reading-the-server.md) — a refresh costs a full read; selecting a family costs a walk.

## Known holes / open
- Two `/api/families` chains can run at once: a refresh clicked before the page's first `/api/families` answers is accepted, and when the server is still reading both answers are `reading` and each schedules its own `load`. The second `ready` turns "Up to date" back to "Refresh" and paints again, which reads the selected family's tree once more. Shown in a harness that holds answers (#30's review, 2026-11-23); the window is the first request's round trip, a few ms on localhost. Present since the page landed; closing it (the first `load` marking the button busy) was left open by the maintainer (2026-11-23).
- That `App::start` marks the read before its thread runs is not tested: the race it closes needs
  the thread scheduled late, which a test cannot arrange without a seam (2026-11-23).
- The page's behaviour (selection, filters, refresh states, escaping, what each act animates) has no test in the repository; it was checked with throwaway Node harnesses, in headless Edge over CDP, and by reading (2026-11-19; the motion again 2026-11-20). The ordering of tree reads, the fallback and the flash timer (#30) were checked in headless Edge with a harness that replaces `fetch`, holds every answer and releases them in a chosen order (2026-11-23). The layout (#38) was checked in headless Edge over CDP at 1440×900 with a stubbed `fetch` serving 150 families and an 86-item family: the page does not scroll in the ready, reading, failed, failed-with-a-kept-read and empty states; the 100th family shows the top of its detail; scrolling the long detail to its end leaves the list in place; a row reached by Tab and one left selected by a refresh or the toggle is inside the list; a resize leaves the list's scroll; `color-scheme` is declared; the detail's ghost keeps the old height; a refresh keeps the detail's scroll for the same family and starts another at its top; #31's focus and the reduced-motion rule hold (2026-11-23). The repository runs no JavaScript.
- `missing` counts every root `tree` draws, a closed item whose parent is absent included, while `roots` does not count that one; and a snapshot saved before labels were kept reads every label list as `[]`, so every closed root shows `missing` ([not-recorded-is-not-none](not-recorded-is-not-none.md)). Open for the maintainer (2026-11-19).
