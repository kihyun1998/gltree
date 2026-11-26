# Serving the viewer

## What it is
`gltree serve`: a local web server on 127.0.0.1 that serves one page embedded in the binary and a
small JSON API over the tracker — the families on the left, each followed by its sub-families, the
selected family's tree on the right, zoomed into one of its items on request,
a refresh button, the badges a repo names in its config, a search over every item, and an issue view
that reads one item's body and comments. It
shows the tracker of the clone it runs in, or, once repos are registered in `~/.gltree/repos.toml`,
every registered repo behind a row of tabs.

## Governing decisions
**None.** The shape rests on issue #27, confirmed with the maintainer (2026-11-19), and on the
reasons below.

## Design model
- **Read only.** The server calls only the tracker port's reads: `all_items` through `tree::tree`,
  `level` through `tree::live_tree` (`Repo::read_all`, `Repo::tree_of`), and `item_rendered` for
  `/api/item/N` (`Repo::item_of`). Nothing on the page writes (issue #27, out of scope).
- **The repo list** (issue #45, maintainer, 2026-11-24). `~/.gltree/repos.toml` holds `[[repo]]`
  entries, each a `name` and the absolute `path` of a clone, in the order they were registered
  (`repos::RepoList`); the home folder is `std::env::home_dir`, which reads `USERPROFILE` first on
  Windows and `HOME` on Unix (std 1.96, `library/std/src/sys/paths/windows.rs`), as the glab config
  lookup already does. Only `serve` reads it; every other command keeps taking its tracker from the
  clone it runs in, so the skills and scripts that call gltree inside clones are unchanged. `serve add
  PATH [--name N]`, `serve list` and `serve remove N` keep it (`main::serve_command`): `add` stores
  the clone's top folder (`repos::clone_at`, through `config::toplevel`), named by the last part of
  its project path (`repos::default_name`), and refuses a path outside a git clone, an `origin` that
  gives no host and project path, and a name taken; it needs no `.gltree.toml`, since `serve` runs
  without one. It reads the remote only and asks no server, so a host that is not GitLab (a GitHub
  `origin`) registers and its tab fails at the first read, naming why (#45's lens review; seen by the
  conductor, 2026-11-24: no glab token for github.com). A
  name is 1 to 64 ASCII letters, digits, `.`, `_` or `-` (`repos::name_error`; worker's judgement,
  2026-11-24): the characters a GitLab project path's last part takes, which `encodeURIComponent`
  leaves as they are, so `?repo=` and the address after `#` carry a name unchanged and the server
  compares the query's value as sent, with no decoding. The file is read whole and refused as
  malformed when it breaks a rule `add` holds (a taken or bad name, a relative path) or names a key
  it does not take; it is written through `snapshot::replace_file`, so a failed write leaves the old
  list. `serve add`, `serve list` and `serve remove` read no repo config, so they run anywhere.
- **What `serve` shows** (#45). With no `--snapshot` and at least one repo registered, every
  registered repo, wherever it runs (`main::serve_registered`); with none, the clone it runs in, as
  before (`main::serve_clone`); with `--snapshot`, that file alone, the list not even read, so a
  broken list does not stop it. A list that cannot be read stops `serve` with exit 2 before the
  address. Each registered repo is a `serve::Repo`: its name, its `Open` over `main::registered_tracker`,
  its `Configure` over `main::registered_config` and its kept read. Before the address, each clone's
  `origin` and glab login are read with no network (`server_in`, the same as `server` with the
  clone's folder); a failure there (a clone that is gone, an `origin` or login that fails) becomes that
  repo's first read, so its tab fails with the reason and the others run. Every
  repo's first full read starts at once, each on its own thread (`App::start` with the repo's index),
  since one read of a real tracker takes about 30–45 s (maintainer, 2026-11-24); stderr says when they
  start and, per repo with its name first, when each ends. A refresh reads the repo it names only.
- **A registered repo's config is read with each full read** (#46). `Repo::read_all` opens the
  tracker, then reads the config through `Configure`, then reads the tree with it; a read that
  succeeds keeps that config in `Kept` beside the tree, and `/api/families`' `badges` and every
  later walk of `/api/tree/N` use it until the next full read succeeds. A config refused or unreadable
  fails that read like any other failure: the first read's tab fails naming the file
  (`<clone>/.gltree.toml: …`), a refresh keeps the last good tree and its config and answers 502
  with the same reason, and the other repos are untouched; the refresh after the fix takes the new
  config. Until a first read succeeds the repo has the default config, which shows no badges and no
  tree. A walk reads no config: it takes the kept one, so the walked tree and the list keep one config
  between refreshes (#46, worker's judgement, 2026-11-24, following the issue's plan). With no repo
  registered, `serve_clone` reads the clone's config once before the address and a broken one stops
  `serve` with exit 2, as before; that `Repo` has no `Configure` and keeps the config it was given.
  `RepoConfig::scheme` knows one scheme, so today only `badges` shows which config a read used.
- **Two reads.** The full read (`tree::tree` with no root, every family, finished ones included)
  starts once the address is printed, on a thread of its own (`App::start`), and again on
  `POST /api/refresh`; its answer is kept and `GET /api/families` serves the kept one. `GET /api/tree/N` reads N's tree fresh: against
  the server it walks from N (`walk`, as `tree N` does), from a snapshot it reads the file again. A
  full read of a tracker of a few thousand items took about 45 s (maintainer, measured, 2026-11-19),
  so the page shows the kept read at once and swaps in the walked tree when it arrives.
- **The tracker is opened per read** through `serve::Open`, a closure `main` builds over `source`,
  the same opener every other command uses (for a registered repo, over `registered_tracker`, which
  checks the clone is there before `server_in` and reads no config): a `SnapshotFile` for `--snapshot`, otherwise `server()`,
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
  crate (reading, 2026-11-19): the six routes need no more. `/api/tree/N` and `/api/item/N` share
  one branch of `App::handle` (`ReadOne`), so a number that does not parse, a method other than GET
  or HEAD, `?repo=` and the 404 / 502 split are the same for both.
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
- **One read at a time; a refresh during one starts nothing** (issue #35), per repo. `Repo::begin` marks a read
  as running, and `App::start` marks it before returning, so a refresh that arrives before the
  thread runs cannot start a second one. `POST /api/refresh` while a read runs answers the
  `/api/families` shape at once with `state` `reading`; otherwise it reads on the request's thread
  and answers when the read ends (worker's judgement, 2026-11-23: the brief changed only the refresh
  that meets a running read). A read that panics is caught (`Repo::finish`) and kept as a failed read
  naming the panic, so `reading` is always released and the next refresh reads again.
  `read_at` is when the kept read finished.
- **`state`** (`Repo::state`, worker's judgement, 2026-11-23, derived from what the page must
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
  `/api/repos` is under `/api/`, so the same checks hold for it, and a repo name is never a path:
  the server looks it up among the registered names and opens only the clone path the list holds.
- **JSON**: `/api/repos`, `/api/families` and errors carry `serve::JSON_VERSION` 4 (3 before #47, 2 before #45) —
  `{version, repos: [{name, state, error, read_at, families, open}]}`, `{version, repo, state, error,
  read_at, badges, tree}`, `/api/item/N`'s `{version, number, title, state, labels, url, body, body_html,
  comments: [{author, created_at, internal, body, body_html}]}` and `{version, error}`; `/api/item/N`
  came with #48 at version 4, since adding an endpoint changes no existing shape, and its `comments`
  is `null` where the source did not record them ([not-recorded-is-not-none](not-recorded-is-not-none.md)); `repos` is the registered repos in the order they
  were registered, empty with none registered or with `--snapshot`, and `families` and `open` count
  the kept read's families and open items, counted once when the read ends (`open_items`), `null`
  until one has; `repo` is the repo's name, `null` for the clone it runs in. With repos registered,
  `/api/families`, `/api/refresh`, `/api/tree/N` and `/api/item/N` name one with `?repo=NAME` (`App::repo`): none or
  `repo` given twice is 400, a name not registered 404; with none registered, a `repo` is 404, so a
  caller asking for a repo never gets the clone's tracker in its place. The query is kept apart from
  the path by `parse_head` and read as `&`-separated pairs, no percent-decoding; `tree` and `/api/tree/N` are
  `tree::to_json` at its own version ([drawing-a-tree](drawing-a-tree.md)). `read_at` is UTC,
  written by `utc` with no date crate. Status: 404 for an unknown path or number, 405 for a known path
  with another method, 400 / 413 for a head or body it will not read, 403 as above, 502 for a read
  that failed, with the tracker's message.
- **Default port 7420**, a free pick (worker's judgement, 2026-11-19); `--port 0` takes a free one and
  the printed address says which.
- **The page** (`src/serve.html`, `include_str!` into `PAGE`): hand-written HTML, CSS and JS, no
  framework, no build step, and no outside address in it (a unit test refuses `://`). Its layout
  follows the Split prototype the maintainer chose (2026-11-19). It applies the finished-family
  toggle itself with `tree`'s rule — a root closed with `closed == total` — and computes badges
  and counts from `labels`, `extra` and the config's `badges`. Every hero number (families, open
  items, each badge's `missing`) spans all families, finished ones included: `missing` only ever
  marks a closed root, which the toggle would mostly hide, and no two of its numbers count different
  sets (maintainer, 2026-11-19). "None" and "Nothing left to do" are what an empty list and its detail
  say.
- **What the page shows** (issue #41: the maintainer picked variant A of three, run on a real
  tracker of about 630 families and 2,800 items, 2026-11-23). A mark tells three states apart, the
  same in the list, the detail head and the tree (`mark`): to do, an open item with no open blocker,
  is a hollow ring in `--accent`; blocked, open with an open blocker, a ring in `--warn` with a
  bar; done a filled check in `--done`, its title in `--sub`. The family list runs by number, highest first, since the newest family is the
  one usually wanted; children inside a family keep `tree`'s ascending order, the order the work was
  split in, which follows `blocked by` (maintainer, #41). A list row is the mark, the title clamped to
  two lines, the number, a progress bar of `pct`, the blockers, and `N left` in `--accent`, or
  `done` when nothing is left, beside the badge chips. `left` is the open descendants, or the family
  itself, 1 or 0, when it has no children. A tree row is a grid of mark, number and title, the title
  taking the rest of the width and wrapping anywhere; the blockers and at most 3 labels, then a `+k`
  chip whose `title` names the rest, sit on a second line under the title (`node`); since #50,
  `blocks #N, …` in `--accent` follows `blocked by #N` there when the item names items it blocks
  (`bks`, the item's `blocks` as the tree JSON carries it). On that tracker
  labels per item ran to 12 and titles to 149 characters, and the one-line row let labels take the
  title's width and run past the card. The detail head shows the family's own labels in full.
- **Done items fold** (#41). Under each parent, the root included, the children whose whole subtree
  is closed (`finished`, the page's copy of the one rule) fold into one `✓ N done — show` row, N counting those children, placed where
  the first of them stood (`branch`). Clicking it unfolds that parent's done children with their
  whole subtrees: a revealed done subtree folds nothing inside it, since it would only show another
  fold row (worker's judgement, 2026-11-23). A lone done child folds too, so the rule has no
  exception. The top bar's "Show done items" switch (`showDone`) unfolds every fold; it is a page
  setting and stays through every act. Which parents the reader unfolded is `unfolded`, a set of
  numbers that belongs to the selected family: `choose` empties it when the selection changes to
  another family (`foldsOf`), so a walk drawn in place and a refresh that keeps the family selected
  keep the folds as the reader left them, and a new selection starts folded (worker's judgement,
  2026-11-23, following how the detail's scroll is kept). An unfold and the switch redraw the detail
  in place through `draw` with the tree and note last drawn (`last`), so the detail keeps its
  scroll and plays no fade; an unfold made while its fold row has focus moves focus to the first row
  it revealed, since the row it was on is gone. That is an unfold from the keyboard, and a click too
  in a browser that focuses a clicked button, as Chromium does (#41's lens review, CDP mouse input:
  focus moved, `:focus-visible` false, so no ring shows). The move is the redraw's focus rule below:
  the fold row has no counterpart, and the first revealed row stands where it stood (#37).
- **The relations box** (#47: the maintainer picked variant 2 of three, run on a real tracker of
  about 2,800 items, 2026-11-24; there 96 of 106 `related` links and 114 of 153 `from` pointed into
  another family). Under the detail's head, `rels` gathers the `blocked_by_open`, `blocks`, `from`
  and `related` of every node of the drawn item's subtree (the family, or the zoomed item, below),
  that item's first and then each item's in the tree's order, into four groups, Blocked by, Blocks,
  From, Related (`KINDS`; #50, maintainer, 2026-11-26: Blocked by added first, the order shared with
  the issue view's box, and "Found while working on" renamed From after the header key), each left
  out when empty, and the whole box left out when all four are. A relation whose two ends are both in
  the drawn subtree shows in both of its groups, `#A → #B` under Blocked by and `#B → #A` under
  Blocks: each group stays the whole list of its own direction (maintainer, 2026-11-26). Blocked by
  lists open blockers only, since the tree JSON carries `blocked_by_open` alone. A row is the target's mark, number and title, `#S →` before it when
  the relation is item S's rather than the root's, and `in #R` after it when the target's family is
  not the drawn one and the target is not R itself, since `#R … in #R` would name it twice (maintainer,
  2026-11-24, after #47 shipped it on every relation to a one-item family). A target in the drawn family is read from it; one in another family from the
  kept full read (`where`, an index of `data.tree` built once per full read), which holds every
  family, finished ones included. A target in neither (deleted, or newer than the full read) shows
  its number alone and is not a button (worker's judgement, 2026-11-24: there is nothing to go to).
  Each group shows its first 5 rows and a `N more` button that shows the rest (`relMore`, emptied
  with `unfolded` when the family changes); it moves focus to the first row it revealed, as an
  unfold does. Choosing a row goes to the target (`jump`): another family is selected, with "Finished
  families" turned on first when that family is finished and hidden, and its folds reset as any new
  selection's are (`reset`). A target outside the zoomed item's subtree zooms out first (`land`,
  `inZoom`; worker's judgement, 2026-11-26: the zoomed tree does not hold it, so the go-to would
  lead nowhere). The target stays the page's aim (`aim`) until a family row is clicked,
  another jump is made or the tab changes: every detail drawn for its family, the full read's first
  and the walk's after it, first unfolds the parent of the first finished item on the path from the
  drawn item (the root, or the zoomed item) to the target (`reveal`, `trail`, `zoomed`), the one fold that hides it, so a walk that finds the
  target newly closed does not fold it away again (#47's bug review). The target's tree row, or the
  detail's title when the target is the drawn item (the family's root, or the zoomed item), is focused once, scrolled into view and
  marked with `hit` for 1.6 s (`aimAt`), the mark put back on each redraw within that time. The
  title is a counterpart of its own in `hold`, so a redraw keeps focus on it (#47's spec review).
- **Search** (#48: the maintainer picked variant A of two, run on a real self-hosted tracker,
  2026-11-25). A box in the top bar, "Search # or title", searches the kept full read of the repo
  shown (`index`, the one index of every item under every root that `where` reads too, finished families included, built once
  per full read, each item with its root and the numbers above it; an item reached from two roots is
  listed under the first). A query that is a number, with or without `#`, lists
  that item and the items whose number starts with it, lowest first, at most 10; since every number
  starting with N is at least N, the exact item comes first (`search`; the issue says "a few", 10 and
  the order are the worker's judgement, 2026-11-25). Any other query lists the titles holding every
  word, lowercased (`toLowerCase`) on both sides, open before closed and then highest number first,
  at most 30, each word marked in the title with `mark` when lowercasing keeps the title's length,
  and nothing marked otherwise (`lit`). A result row is the item's mark, number, title and its place:
  `family` for a root, `in #R › #P` for an item under one (`hitRow`). Choosing one, by Enter, which
  takes the highlighted row (the first until an arrow moves it), or by a click, goes there as a
  relations row does (`jump`, which closes the view first, then `land`); the arrows move through the
  rows (`aria-activedescendant` on the box, a `combobox` with a `listbox`), and Escape closes them
  and keeps the text, so an arrow opens them again (worker's judgement, 2026-11-25; a `search`
  input clears itself on Escape unless the key is taken). Leaving the box closes them. With no kept
  read the placeholder and the one row say "Waiting for the full read…"; a tab switch puts it back
  until that repo is read, and a full read that lands while the rows are open runs the query again.
- **The issue view** (#48: the maintainer picked variant 1 of three, 2026-11-25). "View issue" in the
  detail's head, or a tree row, opens an item in the detail pane, replacing its contents in place
  (`openItem`, `putView`): "← Back to the tree", "Open in GitLab", the mark, number, state and,
  for an item under the root, the path to it, `#R › #M ›` (#50; it replaced `in #R`), the title,
  the item's own relations, the labels, then the body and the comments. The relations box (`own`;
  #50, the maintainer picked variant 1 of three, 2026-11-26) holds the opened item's
  `blocked_by_open`, `blocks`, `from` and `related` alone, in `KINDS`' groups, each headed with
  its count and left out when empty, the box left out when all are; it is drawn from the tree node
  the view was opened with, so it shows while the item is still read. A target in the drawn family is
  read from it; one elsewhere from `where`, with `in #R` as the family box shows it; one in neither
  is its number alone and no button. Choosing a target in the drawn family opens its issue view, which
  keeps the tree's scroll from the first open for "← Back"; one in another family goes to it as a
  relations row does (`jump`). Finds (the items whose `from` names this one) are not listed: the
  API carries no reverse list (maintainer, 2026-11-26). The head
  is drawn at once from the drawn tree, with "Reading #N from GitLab…" under it, and redrawn from
  `GET /api/item/N` when it answers; a failure shows "Cannot read #N: " and the server's message, with
  "Open in GitLab" still in the head. The item is read on every open and nothing is kept
  (`itemWant` drops an answer that arrives after the view closed or another opened). A tree row is
  entered as variant 3 of the prototype showed: every row ends in a `›`, `--faint`, `--accent` while
  the row is hovered, with no transition; a plain click opens the view, and a click with Ctrl, Cmd,
  Shift or Alt, or the middle button (`auxclick`), keeps the row's own link to GitLab in a new tab.
  A row whose item has no `http:` or `https:` URL is a button, so it opens the view too. While the view
  is open, `draw` only records the tree it was given (`last`): a walk, the "Show done items" switch
  and a refresh leave the view, and "← Back" draws `last`, so it shows the newest tree with the folds
  and the switch as they now are, at the scroll the tree had when the view opened, with no ring fill
  (`restore`). A refresh that keeps the family selected keeps the view (`choose`); selecting a family,
  the selected one included, which shows its tree at once, choosing a search result, and a tab
  switch close it (`closeView`). Opening moves focus to "← Back"; Back puts it on the row that opened
  it, or on "View issue", and on the detail when that row is gone. A redraw keeps focus on both by
  the keys *Focus survives a redraw* lists.
- **Sub-families and the zoom** (#50: the maintainer picked variant A of three, run on a real
  self-hosted tracker, 2026-11-25). A sub-family is an item that is not a root and has at least one
  child. Each root's row in the list is followed by its sub-families in tree order, depth first
  (`subs`, `sub`): a button indented 18 px per level below the root, its mark, `↳` and the title,
  `#X · N children`, its progress bar and `N left` or `done`. A finished sub-family follows the
  finished-family toggle ([one rule for a finished family](one-rule-for-a-finished-family.md)): hidden
  while it is off, and every sub-family of a hidden family goes with it. The list stays one flat
  list with no folding of its own (issue #50, story 23); the prototype saw at most 4 sub-families in
  a repo and the issue estimates 30, unmeasured. Zoom is one piece of page state, `zoom`, the root
  and the item zoomed into or `null`, and zooming to the root itself clears it (`zoomTo`).
  `detail` takes the family's root as before and draws the zoomed item through the same `card` a
  root gets, with a path in front (`crumb`, a `nav` of one button per ancestor, `#N` and the
  title, then `#X`), so the ring, the relations box and the tree are the item's with no code of
  their own; `last`, the walk, the folds and the issue view keep the family's root. A sub row
  zooms into its item, selecting its family first; a tree row with children carries a `▸ N` button
  after the row's link, outside it, labelled "Zoom into #X" (`zrow`, `zchip`); an ancestor on the
  path zooms out to it, and the root's button returns to the whole family; the chip and the path
  move focus to the detail's title. The list's highlight and `aria-current` sit on the zoomed
  item's sub row while it is listed, and otherwise on the root's row (`moveHl`). What clears the
  zoom: the root's row (the same family clicked again) and the root on the path, another family
  (`choose`), a tab switch (`show`, since two repos can hold the same numbers), and a search result
  (`choice`); clearing redraws the detail and moves the highlight with it. A refresh and a walk keep
  it while the item is still under that root in what they drew, and drop it otherwise
  (`keepZoom`, in both `choose` and `draw`, so the kept read's first frame already shows the whole
  family when the item moved).
- **Cleaning GitLab's HTML** (#48). GitLab's GraphQL gives the rendered HTML of a description
  (`WorkItemWidgetDescription.descriptionHtml`) and of a comment (`Note.bodyHtml`), both measured
  present on a self-hosted GitLab by the conductor, a body of about 22,000 characters rendered with its
  table (2026-11-25), so the page needs no Markdown renderer (maintainer, 2026-11-25, over a
  hand-written renderer and a vendored library). The page never inserts it as it came: `clean` parses
  it with `DOMParser`, which runs no script and loads nothing, and rebuilds it element by element
  (`put`): an allow-listed tag (`KEEP`) is made anew with no attribute but `colspan` and `rowspan` up
  to three digits, a `text-align` of left, center or right on a cell, `start` on a list and `open` on
  `details`; a link keeps only an `http:` or `https:` `href`, resolved against the item's URL (`safeUrl` with a base),
  and opens in a new tab with `noopener noreferrer`, a link inside a link is left as its text, and a
  link with no text is dropped: GitLab puts an empty `a.anchor` in every heading (8 in one body on
  the self-hosted server, conductor, 2026-11-25), which would otherwise be an unnamed link; a
  checkbox becomes a disabled one; an image becomes a link to its `data-src` or `src` named "Image: "
  and its alt, or that text alone inside a link, since GitLab's uploads need a sign-in and the CSP
  allows only `data:` images; `DROP` (`script`, `style`, `iframe`, `svg`, `math`, form controls and
  the like) goes with its contents; any other tag is dropped and its children kept. No `id`, `class`,
  `style` or event attribute survives, so a body cannot pose as the page's own `#vb` or a `data-n` row.
  GitLab renders the TOML header as a code block marked `data-lang-params="frontmatter"`, the first
  element of the description, in a `div.markdown-code-block` (conductor, live self-hosted GitLab,
  2026-11-25); `clean` drops that block when it is the first element, and leaves a later one, which a
  person wrote. A snapshot keeps no HTML (`Item::desc_html` and `Comment::body_html` are never
  written or read, `#[serde(skip)]`), so `body_html` is `null` and the page shows the Markdown as
  escaped text with its line breaks, the header cut as `header::parse` cuts it (`unfence`); comments
  not recorded say "Comments not recorded; take a fresh snapshot", as `gltree show` does.
- **Repo tabs** (#45: the maintainer picked variant A of three, run on a real tracker of about 630
  families plus two synthetic repos, 2026-11-24). With repos registered, a row of tabs above the hero
  (`#tabs`, a `nav` of buttons) names each repo in the order `/api/repos` gives, with its open items
  (`N left`) once it has a kept read, a dot in `--accent` while it is read and one in `--warn` when its
  read failed, its `title` naming why. The chosen tab has `aria-current="page"` and shows today's page
  for that repo unchanged: every API call carries `?repo=` (`q`). With none registered (`repos`
  empty) the row stays hidden and no call carries `repo`. The page asks `/api/repos` first (`boot`),
  again every 1.5 s while any repo is read (`tabs`, one timer), and again when the shown repo's read
  ends (`take`), or a refresh of it fails with an error status (`ask`), so its tab shows the failure; the buttons are made once and updated in place (`tab`), compared by name as strings
  and never put into a selector, so a tab with focus keeps it through an update. The chosen tab is
  the address after `#`, the name through `encodeURIComponent`: a click sets `location.hash` and the
  `hashchange` that follows switches (`show`), so the back button goes to the tab before and a reload
  or reopened page shows the same repo; a hash naming no registered repo changes nothing, and with no
  hash the first repo is chosen and written with `history.replaceState`. A switch resets the page to
  that repo's first paint (no data, no selection, no kept walks or folds, "Reading…", the button idle)
  and loads its families (worker's judgement, 2026-11-24: the issue keeps the chosen repo, not the
  family chosen in each). `gen` counts switches: an `/api/families` or `/api/refresh` answer, a
  failure included, that arrives after a switch is dropped, and so is the next `load` a `reading`
  answer scheduled, so an old repo's refresh never paints over the new one; a walk is dropped by `want`
  as before, which a switch also raises. The tabs add no motion: the dots do not pulse.
- **No filter bar** (#41, taking over #40). The page once had a filter per badge value and per
  badge's `missing`; the maintainer does not use them (2026-11-23), and with no `[[badges]]` the bar
  held one "All" button that changed nothing. The buttons, their state and the dimming of
  "Finished families" while one was chosen went with them. `[[badges]]` and `/api/families` are
  unchanged and the badge chips still show where configured.
- **Reading state.** While `/api/families` answers `reading` the page asks again every
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
  it; `.list` and `.detail` are the two scroll containers. The banner, "Reading…" and an empty list
  lay out inside the same column. A new selection gives the detail a new element
  (`fade`), so it starts at its top; a walk's tree drawn in place (`draw`) keeps the detail's scroll,
  and a refresh that leaves the same family selected gives the new detail the scroll the old one had
  (`paint`; the maintainer, 2026-11-23, at #38's review: a refresh used to keep the page's scroll). A
  refresh that selects another family starts it at its top.
  The selected row is brought into view inside the list, nearest edge and with no motion (`into`, from
  `moveHl`), whenever the list is drawn or the selection moves: a refresh draws the list again at its
  top, and the toggle can leave the selected row outside the part shown; a row that keeps focus
  through a refresh is brought into view after it (*Focus survives a redraw*, below). A resize moves the
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
  curve, which does not overshoot, and the hero counts up in 0.3 s. The knob of each switch slides between its
  two sides on the same curve, since its side is the switch's state (worker's judgement, 2026-11-20),
  and the refresh button's busy icon spins. Nothing else moves: the state marks and the fold rows
  carry no transition, and an unfold draws in place with no fade (#41). Under reduced motion every fade,
  crossfade, highlight move, ring fill, count, the spin and the knob's slide is off or instant: the
  CSS rule that turns motion off selects `*`, `::before` and `::after`, since `*` alone does not
  match pseudo-elements and the knob is the switch's `::after` (checked in headless Edge with reduced
  motion emulated, 2026-11-23: no element or pseudo-element keeps a transition or an animation, and
  toggling the switch starts none).
- **What each act draws** (maintainer, 2026-11-19: selecting a family must not stutter; a detail
  drawn a second time when the walk's tree arrives plays its entrance and ring again). The first read
  and a refresh draw the whole page (`paint`): the hero counts up, and the list and the detail fade in.
  The finished-family toggle (`relist`) sets both switches' state in place (`sync`); the switches
  sit in the top bar, which no act draws again, so a switch operated by keyboard keeps focus.
  `relist` crossfades the list rows when their markup changed; the
  hero counts every family, so the toggle does not change it, and the highlight stays the same element and
  moves to the selected row. A selected family that stays in the list keeps its detail as it is, a
  stale note included, and is not read again. A list that drops it selects its first family and
  crossfades the detail as `choose` does; a list left empty crossfades the detail to
  "Nothing left to do". A toggle whose rows come out the same changes nothing.
  Selecting a family moves the highlight and crossfades the detail, its ring filling once (`choose`).
  The tree `/api/tree/N` brings replaces the detail's contents in place (`draw`), keeping the detail
  element, so a fade-in still running carries on uninterrupted and the new ring starts from the offset
  the old one showed. A tree whose markup is the same as what is shown changes nothing; on a live
  self-hosted tracker the walk and the full read gave the same JSON for families of 1, 2, 5 and 54
  items, so a selection there takes this path (read-only run, 2026-11-20). Clicking the
  selected family again reads `/api/tree/N` again and puts it in place through `draw`, with no fade
  (worker's judgement, 2026-11-19).
- **Focus survives a redraw** (issue #37, 2026-11-24). The two acts that rebuild what may hold
  focus, `paint` (the first read and a refresh) and `draw` (a walked tree, an unfold, the "Show done
  items" switch), note before they redraw whether focus is inside what they replace (`hold`), and
  afterwards focus its counterpart in what was drawn (`go`), by a key that survives the redraw: a
  family row by its number (`data-s`), a sub-family row by its item number (`data-x`), a tree row by
  its item number (`data-n`), a fold row by its parent's number (`data-u`), a zoom chip by its item
  number (`data-z`), a path button by its item number (`data-p`), a relations-box row by its group,
  source and target (`data-j`), an issue-view relations row by its group and target (`data-ik`), a
  group's `N more` by its group (`data-g`), "Open in GitLab" and "View issue" by their class, the issue view's "← Back" (`#vb`) and the detail itself by their id. Keys
  are compared as strings against `dataset`, never put into a selector, so a number the tracker
  sends cannot change what is looked up; the search covers only `#rows` and `#det`, which a fading
  ghost no longer carries as its id. Where the counterpart is gone, focus goes to a fallback (worker's
  judgement, 2026-11-24, choosing the nearest thing that still means the same place): a family row
  that left the list falls to the selected family's row, or the list when it is empty; something in
  the detail of a family still selected falls to the tree row now at the position it had, the last
  row when the tree got shorter, the row's link where that position holds a row with a zoom chip
  (`rowOf`). Everything else falls to the detail: the family gone, the tree
  empty, or "Open in GitLab" or the detail itself with nothing in its place.
  A target that cannot take focus (the detail) gets `tabIndex` -1 for as
  long as it holds focus, and loses it on blur: `draw` keeps the detail element, and a detail left
  at -1 would lose the Tab stop Chromium gives a scroll container once it grows long enough to scroll.
  Focus is moved with `preventScroll`, so the detail keeps the scroll `paint` gives it back; a
  family row focused this way is brought into the list's view as the highlight's row is (`into`),
  since a rebuilt list starts at the selected row and the focused one may be elsewhere. Focus outside
  what is redrawn (the top bar, or nothing) is left alone, so a redraw never takes focus. Focusing
  adds no motion. Choosing a family, which crossfades the detail (`choose`), and the finished-family
  toggle, which crossfades the rows (`relist`), keep no focus of their own: each is started from the
  list or the top bar, outside what it replaces, as long as the activation puts focus on what was
  activated, as a click does in Chromium and the keyboard does everywhere. An activation that leaves
  focus where it was (a screen reader's browse mode, a click in Safari) can leave it on a detail or
  a row these acts replace, and then it is lost; present since the page landed (#37's lens review,
  2026-11-24), left as it is by the maintainer, since the page is judged on desktop Edge.
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
  as a button with no link. The one exception is the HTML GitLab renders for a body or a comment,
  which the page rebuilds through an allow-list instead (*Cleaning GitLab's HTML*, below). `tree --json` passes `url`, labels and `[extra]` through as the tracker holds them
  (#26's review, comment on #27), so the page is where they meet HTML.
- **Badges** come from the repo config ([reading-the-repo-config](reading-the-repo-config.md)) and
  apply to roots only: the first label starting with `prefix` gives the value (the label's rest); a
  closed root with none shows `missing` when the badge sets it. gltree knows no badge by name, which
  keeps [listing-roots](listing-roots.md)' boundary: label meanings are the caller's.

## Code
- `src/serve.rs — JSON_VERSION, DEFAULT_PORT, PAGE, Open, Configure, Repo, Repo::named, App, Kept, Kept::new, Done, App::new, App::registered, App::start, App::repos_json, App::repo, Repo::refresh, Repo::begin, Repo::finish, Repo::read_all, Repo::families, Repo::summary, Repo::tree_of, Repo::item_of, ReadOne, open_items, App::ours, App::handle, Request, Head, Status, Response, utc, bind, serve, answer, parse_head, write`
- `src/item.rs — Item::desc_html, Comment::body_html` (never written to a snapshot or `show --json`)
- `src/tracker.rs — Tracker::item_rendered` (a source that renders nothing answers `item_with_comments`)
- `src/repos.rs — Repo, RepoList, RepoList::file_in, RepoList::load, RepoList::add, RepoList::remove, RepoList::save, name_error, clone_at, default_name`
- `src/snapshot.rs — SnapshotFile::open` (the check before the address, a directory refused)
- `src/config.rs — Badge` (serialised as it is into `/api/families`' `badges`)
- `src/tracker.rs — FakeTracker::fail_all_items` (the unit test that a walk reads no full tracker)
- `src/serve.html`
- `src/main.rs — Command::Serve, ServeList, serve_command, repo_list, registered_clone, registered_config, registered_tracker, serve_clone, serve_registered, listen, print_address, source, server, server_in`
- `tests/cli.rs — serve_in, read_families, serve_that_stops, the serve_* tests, at_home, in_home, stand_in, read_repos`; `gltree` and `gltree_cmd` give every command they start a temporary home (`HOME` and `USERPROFILE`), and every test that runs `serve` or a `serve` subcommand starts it through them; the few tests that start the binary by hand run `tree`, `check`, `new`, `move` and `sync`, which never read `~/.gltree`; `serve_with_repos_registered_shows_each_wherever_it_runs_and_refreshes_the_one_named` serves three registered clones, two backed by local stand-ins that answer every GraphQL request with one page of work items and count the requests, and one whose folder is deleted, then rewrites one clone's `.gltree.toml` and breaks it, refreshing after each; `serve_prints_its_address_while_the_first_read_still_waits_on_the_server` holds the first read behind a local listener that never answers, since a snapshot's read ends too fast to show the order

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
- #45's page was checked in headless Edge over CDP at 1440×900 against the real binary, three
  registered clones and two local stand-in GraphQL servers, one holding its first answer 3 s
  (2026-11-24): the tabs in order with `app` chosen and written after `#`; the reading dot, then
  `2 left` and `1 left`; the failed dot with its reason and the failed repo's banner; a real mouse
  click switching tabs and the address; a reload and the back button keeping the tab; an unknown name
  after `#` changing nothing; a refresh reading the shown repo only; focus on a tab kept through a
  refresh's tab update; an `app` refresh that answers after a switch to `site` leaving `site` as it
  is; the tabs above the hero and the page not scrolling; and no tabs and no hash with `--snapshot`.
  Three mutations were each seen failing their check: no `gen` guard in `ask`, no `replaceState`, and
  the tab row rebuilt on every update. The stand-in answered a walk with the page shape, so a walked
  tree drew the stale note; the walk itself is #30's, unchanged here.
- #50's sub-families, zoom and relations were checked in headless Edge over CDP at 1440×900 with a
  stubbed `fetch` serving two repos of made-up items (2026-11-26): one with a family holding
  sub-families two levels deep, a finished sub-family, an in-family blocked-by pair, relations to
  another family and to an item not in the read, an HTML title, a family with no sub-family and a
  finished family with one, and a family whose sub-family holds a relation to itself beside a finished
  sub-family whose children relate; the other with the same numbers as the first. 30 checks: the sub rows,
  their order, indent and text, none for a flat family, the toggle hiding and showing them; a sub row
  zooming (path, ring, tree, relations box, highlight and `aria-current`), the path zooming out and
  back to the family, the root row clearing it, the tree's chip (its own button, not inside the link,
  none on a leaf); another family, a tab switch to a repo holding the same numbers, a search result
  inside the zoomed subtree and a go-to outside it clearing the zoom; a go-to inside the zoom marking
  and focusing the title when the target is the zoomed item, and unfolding the zoomed item's done
  children when it is finished (check-it's lens and bug reviews, 2026-11-26); a refresh keeping it and one
  that moved the item dropping it, also in the kept read's first frame with the walk held; the issue
  view's path; Enter on a sub row and a chip reached by Tab, Space on a path button, focus on the
  title after a chip; focus kept on a sub row, a chip and a path button through a refresh, on an
  issue-view relations row when the item answers, and the fallback landing on a row's link; the
  issue view's groups, counts and order shown while the item still loads, empty ones left out, no box
  for an item with none, an unknown target as plain text, `in #R` only for another family, a target
  in the same family opening its view and one elsewhere going to its tree; the family box's four
  groups, From, the pair in both groups and a Blocked by row's go-to; `blocks #N` on a tree row. 50
  mutations of the page each failed the check named for them, among them the two landing fixes
  turned off one at a time and the shared relation row, bar, `left` and title helpers each broken;
  one that dropped `in #R`'s test against the drawn item survived, since it differs only for a
  target missing from the walk but under the same root in the full read, which no fixture holds (the test predates #50), and one that compared the target with any zoomed item, not only the drawn family's, is equivalent, since item numbers are unique within a repo; two runs that lost the browser's port
  were run again. The page was opened from a file, so no CSP stood behind it; against the real
  binary serving `tests/fixtures/tracker.json` with its CSP, the two sub rows listed, one zoomed with
  its path, and a child's issue view showed `#1 › #2 ›`, with no console message. The harness is not committed: the repository runs no JavaScript (maintainer, #50).
- #48's search and issue view were checked in headless Edge 158 over CDP at 1440×900, with a stubbed
  `fetch` that can hold any answer, against a tree of five families (one of 44 items with a fold, a
  finished one, numbers 12, 120, 123 and 1200, an XSS title) and item answers that include a body
  holding a frontmatter block, a table, task lists, a `javascript:` link, relative links, GitLab's
  lazy image markup, `script`, `iframe`, `style`, `svg`, a form, `onerror`/`onclick` and an `id="vb"`
  (2026-11-25); the page was opened from a file, so no CSP stood behind the cleaner. 44 checks: the
  number and word orders, the marks and the path, no match, the arrows, Enter and Escape, a child, a
  folded item and a finished family reached, a root's title marked, the waiting box and its return on
  a tab switch, the search kept to the tab shown and rebuilt after a switch; every row's `›`, its
  hover colour, a row with no link as a button, "View issue" beside "Open in GitLab", Ctrl, Shift and
  middle clicks keeping the link; the loading state and focus on Back, the rendered body, the header
  block gone and a later one kept, a heading anchor leaving no empty link, nothing hostile left (no script ran, no element or attribute
  outside the list), links and images, Back with the scroll, folds and focus, "View issue" and focus,
  the failure, a snapshot's plain text, unrecorded comments, an answer after Back dropped, a refresh
  and a walk while the view is open, the "Show done items" switch under it, focus on "View issue"
  through a refresh, the relations box's go-to after it was split into `land`, and the view closed by
  another family, the same family, a search result and a tab switch, `?repo=` on the item call.
  48 mutations of the page each failed the check named for them (42, then 6 more after the review's
  fixes); four first survived, each a check
  whose state never occurred (the switch with every fold already open, a walk equal to the drawn tree,
  a tab switch between families of different numbers, Escape cleared by the input itself), and the
  checks were changed until each failed. Against the real binary serving the fixture snapshot with
  its CSP, a row opened the view and the search listed results with no console message. GitLab's
  markup for an uploaded image (`data-src` on a lazy `img` inside a link) was written from memory and
  not read from a real answer. The rendered query fits the real server's complexity cap
  ([reading-the-server](reading-the-server.md)).
- #47's box was checked in headless Edge over CDP at 1440×900 with a stubbed `fetch` serving ten
  families, one with relations from its root and two items, an XSS title among the targets, a
  finished family and folded done items (2026-11-24): the three groups and their labels, 5 rows and
  `4 more` of 9, `#S →` and `in #R`, the box under the head, the title rendered as text, no box on a
  family without relations, `N more` showing all and moving focus to the sixth row, focus on a box
  row kept through a redraw that changes the markup, a jump into a fold of the same family, a jump
  in the same family, the mark gone after 1.6 s, a jump into another family's fold with the list's
  highlight moved, a jump to a finished family's root turning "Finished families" on, and no mark
  left after selecting another family. Seven mutations were each seen failing their check: no switch
  turned on, no unfold, `data-j` and `data-g` dropped from `hold`, no fold at 5, no `#S →`, no focus
  on the target, and the mark kept after a family row is clicked. After the review, with a walk held
  400 ms: the target shown in the full read's first frame and still shown when the walk returns it
  closed, and focus kept on the title when the walk renames the root; each seen failing with
  `reveal` left out of `draw`, then of `choose`, and with the title dropped from `hold`. The reduced-motion rule was read,
  not run: the box adds no transition or animation.
- Two `/api/families` chains can run at once: a refresh clicked before the page's first `/api/families` answers is accepted, and when the server is still reading both answers are `reading` and each schedules its own `load`. The second `ready` turns "Up to date" back to "Refresh" and paints again, which reads the selected family's tree once more. Shown in a harness that holds answers (#30's review, 2026-11-23); the window is the first request's round trip, a few ms on localhost. Present since the page landed; closing it (the first `load` marking the button busy) was left open by the maintainer (2026-11-23).
- That `App::start` marks the read before its thread runs is not tested: the race it closes needs
  the thread scheduled late, which a test cannot arrange without a seam (2026-11-23).
- The page's behaviour (selection, folds, refresh states, escaping, what each act animates) has no test in the repository; it was checked with throwaway Node harnesses, in headless Edge over CDP, and by reading (2026-11-19; the motion again 2026-11-20). The ordering of tree reads, the fallback and the flash timer (#30) were checked in headless Edge with a harness that replaces `fetch`, holds every answer and releases them in a chosen order (2026-11-23). The layout (#38) was checked in headless Edge over CDP at 1440×900 with a stubbed `fetch` serving 150 families and an 86-item family: the page does not scroll in the ready, reading, failed, failed-with-a-kept-read and empty states; the 100th family shows the top of its detail; scrolling the long detail to its end leaves the list in place; a row reached by Tab and one left selected by a refresh or the toggle is inside the list; a resize leaves the list's scroll; `color-scheme` is declared; the detail's ghost keeps the old height; a refresh keeps the detail's scroll for the same family and starts another at its top; #31's focus and the reduced-motion rule hold (2026-11-23). #41 was checked in headless Edge at 1440×900 with a stubbed `fetch` serving families that include an item with 12 labels and a 149-character title and an XSS title, label and `javascript:` URL: the order, the marks, `N left`, the clamp, the `+k` chip, no overflow in the list or the detail, the folds and where they sit, unfolding in place with focus moved, the folds kept through a walk drawn in place and a refresh and reset by a new selection, both switches, and no filter bar with and without `[[badges]]`; each of those checks was seen failing, on the page before #41 or under a mutation of the code it covers (2026-11-23). The reduced-motion rule was read, not run: the new rows add no transition. #37 was checked in headless Edge at 1440×900 with a stubbed `fetch` that holds every answer: a family row, a tree link, a fold row and "Open in GitLab" keep focus through a refresh, a tree link and "Open in GitLab" through a walked tree drawn in place, the three fallbacks, the top bar's focus and no focus left alone, the unfold rule, Tab after a refresh going on from the kept row, the detail's scroll kept, and a row far down the list shown in it; each was seen failing on the page before #37 or under one of eleven mutations of the new code (2026-11-24). A detail that took focus as a fallback and then scrolls is a Tab stop again (#37's review): Tab from the last family row reaches it, and goes to the body when `tabIndex` -1 is left on it. The repository runs no JavaScript.
- `missing` counts every root `tree` draws, a closed item whose parent is absent included, while `roots` does not count that one; and a snapshot saved before labels were kept reads every label list as `[]`, so every closed root shows `missing` ([not-recorded-is-not-none](not-recorded-is-not-none.md)). Open for the maintainer (2026-11-19).
