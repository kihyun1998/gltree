use clap::{Parser, Subcommand};
use gltree::check;
use gltree::config::{self, RepoConfig};
use gltree::derived;
use gltree::gitlab::GitLab;
use gltree::item::State;
use gltree::login;
use gltree::migrate;
use gltree::remote::Remote;
use gltree::repos::{self, RepoList};
use gltree::roots;
use gltree::serve;
use gltree::show;
use gltree::snapshot::SnapshotFile;
use gltree::tracker::{Error as TrackerError, Tracker};
use gltree::tree;
use gltree::write;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

macro_rules! requires_clone {
    () => {
        "Requirements:
  Run gltree inside a clone of the GitLab project. It takes the project from
  `git remote get-url origin` and logs in with glab's saved login for that
  host (run `glab auth login` once); GITLAB_TOKEN overrides the token and
  GLTREE_CA_CERT the CA certificate.
"
    };
}

macro_rules! reads_a_snapshot {
    () => {
        "  With --snapshot FILE it reads that file instead and needs no login or
  server. Run it in the clone all the same: outside one, the repo config does
  not apply.
"
    };
}

macro_rules! reads_the_whole_tracker {
    () => {
        "  A read of the whole tracker fetches 100 items at a time, one page after
  another: on a tracker of a few thousand items it takes most of a minute.
"
    };
}

macro_rules! writes_need_the_config {
    () => {
        "  Writing needs the repo config, a `.gltree.toml` at the root of the clone;
  without one gltree refuses before writing anything.
"
    };
}

macro_rules! reads_the_config {
    () => {
        "  Inside a git clone, every command first reads the clone's `.gltree.toml`
  when there is one, and stops when it cannot read it.
"
    };
}

macro_rules! exit_status {
    () => {
        "
Exit status:
  0 on success; 2 on a usage, input, config or server error, with the reason
  on stderr."
    };
}

macro_rules! exit_status_of_a_write {
    () => {
        "
Exit status:
  0 on success; 2 on a usage, input, config or server error, with the reason
  on stderr, and when a write stopped part way, naming what was written and
  what is left to run."
    };
}

#[derive(Parser)]
#[command(
    name = "gltree",
    bin_name = "gltree",
    version,
    about = "Draw and keep the trees of a GitLab project's issues",
    long_about = "gltree draws and keeps the trees of a GitLab project's tracker, where each
item (one GitLab issue) records its references in a header: a TOML block
between `+++` lines at the top of the item's description that names its
parent (the item it is part of), the item it was found while working on,
related items and blockers. An item with no parent is a root; a root and
everything under it is a family.

It reads the trees (tree, roots, show, check, serve), saves the tracker to a
file to read offline (snapshot), and changes it with each header, the links
between items and the lists on the other ends kept in step (new, edit, move,
sync, migrate). Item numbers are written N or #N; quote #N in a shell, where #
starts a comment.",
    after_help = "Run `gltree <command> --help` for a command's examples, requirements and output.",
    after_long_help = concat!(
        requires_clone!(),
        "  `gltree serve` with repos registered by `gltree serve add` runs anywhere.
",
        "  tree, roots, check, show, migrate and serve take --snapshot FILE, a copy
  saved by `gltree snapshot`, and then need no login or server.
",
        writes_need_the_config!(),
        "
Examples:
  gltree tree 12      # the tree under #12
  gltree tree         # every family that still has an open item
  gltree check        # every way the tracker breaks the convention

Exit status:
  0 on success, 1 when `check` finds anything, 2 on an error.

Run `gltree <command> --help` for a command's options, examples and output."
    )
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(flatten)]
    InClone(InClone),
    #[command(
        about = "Serve a local page of the tracker's families and trees on 127.0.0.1",
        args_conflicts_with_subcommands = true,
        long_about = "Starts a local web server on 127.0.0.1 only, out of reach of other machines,
prints the page's address at once and serves the page until Ctrl+C. The
whole tracker is read in the background; until that first read is done, the
page says it is reading. On the left are the families (a root and everything
under it), as `gltree tree` picks them, the finished ones behind a toggle,
each with its progress and badges; on the right is the selected family's
tree, read fresh, every item linked to its page on the server. The refresh
button reads the whole tracker again. It writes nothing to the tracker.

With repos registered by `gltree serve add`, it shows every one of them,
wherever it runs: a tab above the page for each, in the order they were
registered, with how many items it has open. Every registered repo is read
at start, all at once, and the refresh button reads the repo shown. With
none registered, it shows the clone it runs in. --snapshot shows that file
alone and leaves the list out. The list is ~/.gltree/repos.toml (~ is your
home folder, %USERPROFILE% on Windows); `serve add`, `serve list` and
`serve remove` keep it, and no other command reads it.

Badges come from `[[badges]]` in the repo config: each names a label prefix,
and a root's label that starts with it is shown as a badge.",
        after_long_help = concat!(
            "Examples:
  gltree serve                            # http://127.0.0.1:7420/
  gltree serve --port 0                   # a free port
  gltree serve --snapshot tracker.json    # from a file; refresh reads it again
  gltree serve add ../web                 # show ../web's tracker too
  gltree serve list                       # the repos it shows

Output:
  Text: the address, `http://127.0.0.1:PORT/`, before the first read starts.
  On stderr, a line when the first read starts and one when it ends, with
  the number of families and the time it took, or with why it failed; with
  repos registered, one such line for each repo, its name first.

Requirements:
  With no repo registered, run it inside a clone of the GitLab project. It
  takes the project from `git remote get-url origin` and logs in with glab's
  saved login for that host (run `glab auth login` once); GITLAB_TOKEN
  overrides the token and GLTREE_CA_CERT the CA certificate. With repos
  registered, it does the same in each registered clone, wherever it runs.
",
            reads_a_snapshot!(),
            reads_the_whole_tracker!(),
            "  With no repo registered, it first reads the clone's `.gltree.toml` when
  there is one, and stops when it cannot read it. With repos registered, it
  reads each registered clone's instead, again on each refresh of its tab,
  and one that fails fails that repo's tab only, which keeps its last read.
",
            "
Exit status:
  2 when it cannot start (a port in use, a config that fails, git's origin,
  the glab login or the CA certificate it cannot read, a snapshot file that
  will not open or is a directory, a repo list it cannot read), with the
  reason on stderr and no address printed. With repos registered, a clone
  that is not there or whose origin, login or config fails does not stop it:
  that repo's tab shows why. A first read that fails does not stop it: the
  page shows why, and the refresh button reads again. Ctrl+C stops it."
        )
    )]
    Serve {
        #[command(subcommand)]
        list: Option<ServeList>,
        /// The port; 0 takes a free one
        #[arg(long, default_value_t = serve::DEFAULT_PORT)]
        port: u16,
        /// Read the tracker from FILE; refresh reads it again
        #[arg(
            long,
            value_name = "FILE",
            long_help = "Read the tracker from FILE, a copy saved by `gltree snapshot`, instead
of the server, leaving the repo list out; the refresh button reads
the file again"
        )]
        snapshot: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ServeList {
    #[command(
        about = "Register a clone whose tracker gltree serve shows",
        long_about = "Adds the clone at PATH to the list gltree serve shows, ~/.gltree/repos.toml,
under NAME: the last part of the GitLab project's path, unless --name gives
another. Its tab shows the project the clone's `origin` names, with the
clone's `.gltree.toml`, both read when serve starts and again on each
refresh of its tab. Every other command keeps reading the clone it runs in.",
        after_long_help = concat!(
            "Examples:
  gltree serve add .                      # the clone you are in
  gltree serve add ../web --name site     # another clone, under another name

Output:
  Text: `registered NAME: PATH`, PATH being the clone's top folder.

Requirements:
  PATH is inside a git clone whose `origin` names a GitLab project
  (`git remote get-url origin`); a `.gltree.toml` is not required. It needs
  no login or server: glab's saved login is read when serve starts.
",
            "
Exit status:
  0 on success; 2 when PATH is not a git clone, its origin gives no host and
  project path (whether the host is GitLab shows when serve reads it), NAME
  is taken or is not 1 to 64 letters, digits, `.`, `_` or `-`, or the list
  cannot be read or written, with the reason on stderr."
        )
    )]
    Add {
        /// The clone to register
        #[arg(value_name = "PATH")]
        path: PathBuf,
        /// The name its tab shows; the project's name by default
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },
    #[command(
        about = "List the repos gltree serve shows",
        long_about = "Prints the repos registered in ~/.gltree/repos.toml, in the order they were
registered, which is the order of serve's tabs.",
        after_long_help = "Examples:
  gltree serve list

Output:
  Text: one line per repo, `NAME  PATH`; nothing when none is registered.

Requirements:
  None: it reads ~/.gltree/repos.toml only, and runs anywhere.

Exit status:
  0 on success; 2 when the list cannot be read, with the reason on stderr."
    )]
    List,
    #[command(
        about = "Take a repo off the list gltree serve shows",
        long_about = "Removes NAME from ~/.gltree/repos.toml. The clone and its tracker are left
as they are.",
        after_long_help = "Examples:
  gltree serve remove web
  gltree serve remove site                # the name given with --name

Output:
  Text: `removed NAME: PATH`.

Requirements:
  None: it changes ~/.gltree/repos.toml only, and runs anywhere.

Exit status:
  0 on success; 2 when no repo is registered as NAME, or the list cannot be
  read or written, with the reason on stderr."
    )]
    Remove {
        /// The registered repo to remove
        #[arg(value_name = "NAME")]
        name: String,
    },
}

#[derive(Subcommand)]
enum InClone {
    #[command(
        about = "Draw the tree under an item, or every family with open work",
        long_about = "Draws an item and everything under it as an indented tree. Each line shows
the number, state and title, `closed/total` over the item's descendants (every
item under it, at any depth) when it has any, and `blocked by #B` while a
blocker is still open.

With a number it draws that item's tree, read from the server one level at a
time along the links between parents and children. Without one it reads the
whole tracker and draws every family (a root and everything under it) that
holds an open item, closed items in it included; a family whose every item
is closed is left out unless --all.",
        after_long_help = concat!(
            "Examples:
  gltree tree 12                              # the tree under #12
  gltree tree                                 # every family with an open item
  gltree tree --all --snapshot tracker.json   # every family, from a saved file
  gltree tree 12 --json                       # the tree under #12 as JSON

Output:
  Text, one item per line, each indented under its parent. --json prints
  {\"version\": 3, \"roots\": [node]}, where a node is {number, title, state,
  url, labels, extra, closed, total, blocked_by_open, children}.

",
            requires_clone!(),
            reads_a_snapshot!(),
            "  Without a number, it reads the whole tracker.\n",
            reads_the_whole_tracker!(),
            reads_the_config!(),
            exit_status!()
        )
    )]
    Tree {
        /// The item to draw from, as N or #N; leave it out for every family
        #[arg(value_parser = parse_iid)]
        root: Option<u64>,
        /// Draw every family, finished ones too; no number
        #[arg(long, conflicts_with = "root")]
        all: bool,
        /// Read the tracker from FILE instead of the server
        #[arg(
            long,
            value_name = "FILE",
            long_help = "Read the tracker from FILE, a copy saved by `gltree snapshot`, instead
of the server"
        )]
        snapshot: Option<PathBuf>,
        /// Print JSON (version 3) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Report every way the tracker breaks the convention; exit 1 if any",
        long_about = "Reads every item and reports each way the tracker breaks the convention
gltree keeps: every item's references in its header (the TOML block between
`+++` lines that opens its description), and a `relates_to` link between
the two items for every reference. One finding per line: a header
out of form (header), a first-line head left from before headers
(old-header), a GitLab Task rather than an issue (task), a parent
or reference to an item that does not exist (missing-parent,
missing-reference), a reference without its link (unlinked), a link no
reference explains (stray-link), a list gltree keeps that is out of step
(derived), a loop of blockers or parents (blocker-cycle, cycle), and a state
the repo's state policy does not allow (closed-parent-open-child,
open-parent-all-closed); the state policy is the repo config's rule for when
an item may be closed or open against its descendants.

Most findings are fixed with a command: `derived` names the `gltree sync N`
that rewrites the list; a wrong parent is changed with `gltree move`, a
wrong related or blocked_by with `gltree edit`. An old first-line head is
moved by `gltree migrate` in an item with no header yet (pass the repo's own
words with --from-word or --related-word); in an item that has a header,
move the line into the header by hand. A header finding names what is wrong
in the block.",
        after_long_help = concat!(
            "Examples:
  gltree check                              # read from the server
  gltree check --json
  gltree check --snapshot migrated.json     # a tracker saved by migrate --out

Output:
  Text: `kind  #N  detail` per finding, then `N anomalies`. --json prints
  {\"version\": 1, \"findings\": [{kind, item, detail}]}.

",
            requires_clone!(),
            reads_a_snapshot!(),
            reads_the_whole_tracker!(),
            reads_the_config!(),
            "
Exit status:
  0 when it finds nothing, 1 when it finds anything, 2 on a usage, input,
  config or server error, with the reason on stderr."
        )
    )]
    Check {
        /// Read the tracker from FILE instead of the server
        #[arg(
            long,
            value_name = "FILE",
            long_help = "Read the tracker from FILE, a copy saved by `gltree snapshot`, instead
of the server"
        )]
        snapshot: Option<PathBuf>,
        /// Print JSON (version 1) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Create an item with its header, links and other ends in one step",
        long_about = "Creates an item whose description is a header naming its references (the
TOML block between `+++` lines that opens a description), a blank line,
then the body file. It links the new item to every item the
header names and updates the lists gltree keeps on the other ends:
`children` on the parent, `blocks` on each blocker, `related` on each related
item. Without --parent it creates a root.

It refuses before writing anything when a named item does not exist, when
--from names the parent, when the body already opens with `+++`, or when the
parent is closed and the repo's state policy (its rule for when an item may
be closed or open against its descendants) would then report it (reopen the
parent, or use --from). If the item is created but a later step fails,
it names the item and the steps left.",
        after_long_help = concat!(
            "Examples:
  gltree new --parent 12 --title \"Rows keep their order\" --body-file body.md
  gltree new --from 13 --title \"Trailing space on the line\" --body-file body.md --label bug
  gltree new --parent 12 --related 30 --blocked-by 40 --title \"Keep the width\" --body-file body.md

Output:
  Text: `#N under #P: URL`, or `#N (a root): URL`, with `, from #F` when
  --from is given. --json prints {\"version\": 2, \"created\": {number, url},
  \"link\": {parent, child} | null, \"from\": N | null, \"related\": [N],
  \"blocked_by\": [N]}.

",
            requires_clone!(),
            writes_need_the_config!(),
            reads_the_config!(),
            exit_status_of_a_write!()
        )
    )]
    New {
        /// The item it is part of; leave it out for a root
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "The item it is part of, as N or #N; leave it out to create a root"
        )]
        parent: Option<u64>,
        /// The item it was found while working on
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "The item it was found while working on, as N or #N; written as
`from` in the header. The new item is not part of it, and that item's
state does not depend on this one."
        )]
        from: Option<u64>,
        /// A related item, listed on both ends; repeatable
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "A related item, as N or #N, listed under `related` on both ends;
repeat for several"
        )]
        related: Vec<u64>,
        /// An item that has to close first; repeatable
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "An item that has to close before this one can start, as N or #N;
repeat for several. It lists the new item under `blocks`."
        )]
        blocked_by: Vec<u64>,
        /// The title of the new item
        #[arg(long)]
        title: String,
        /// A UTF-8 file with the description
        #[arg(
            long,
            value_name = "FILE",
            long_help = "A UTF-8 file holding the description; gltree writes the header above
it, then a blank line"
        )]
        body_file: PathBuf,
        /// A label; repeat it, or separate several with commas
        #[arg(
            long = "label",
            value_name = "LABEL",
            long_help = "A label to apply; repeat it, or separate several with commas. Only the
labels given are applied."
        )]
        labels: Vec<String>,
        /// Print JSON (version 2) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Change an item's related items, blockers or extra values",
        long_about = "Changes one item's `related` and `blocked_by` lists in its header (the
TOML block between `+++` lines that opens its description), together with
the links and the other ends: an added related item lists this one back, an
added blocker lists it under `blocks`, a removed related item is dropped
on both ends, and a removed blocker's `blocks` is brought
back in step. A link is removed only when no reference on either end still
needs it. --set-extra writes a value in the header's `[extra]` table, which
holds the keys the repo config declares in `extra_keys`.

It refuses before writing a header out of form, the item itself, an item
to add that does not exist, a blocker this item already blocks at any
depth, and an `[extra]` key the repo config does not declare. An edit that
changes nothing writes nothing. It needs at least one --add-*, --remove-* or
--set-extra.",
        after_long_help = concat!(
            "Examples:
  gltree edit 19 --add-related 30 --add-blocked-by 40
  gltree edit 19 --remove-related 30 --remove-blocked-by 40
  gltree edit 19 --set-extra \"marker=K-1\"   # a key the repo config declares

Output:
  Text: `#N now related [..] blocked_by [..]`, or `already` when nothing
  changed. --json prints {\"version\": 1, \"item\", \"related\", \"blocked_by\",
  \"changed\"}.

",
            requires_clone!(),
            writes_need_the_config!(),
            reads_the_config!(),
            exit_status_of_a_write!()
        )
    )]
    Edit {
        /// The item to change, as N or #N
        #[arg(value_parser = parse_iid)]
        item: u64,
        /// List N under `related`, on both ends
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "List N under `related` on both ends; repeat for several"
        )]
        add_related: Vec<u64>,
        /// Drop N from `related`, on both ends
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "Drop N from `related` on both ends; repeat for several"
        )]
        remove_related: Vec<u64>,
        /// List N under `blocked_by`: this item waits for N
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "List N under `blocked_by`: this item cannot start until N closes. N
lists this item under `blocks`. Repeat for several."
        )]
        add_blocked_by: Vec<u64>,
        /// Drop N from `blocked_by`
        #[arg(
            long,
            value_parser = parse_iid,
            value_name = "N",
            long_help = "Drop N from `blocked_by`, and this item from N's `blocks`; repeat
for several"
        )]
        remove_blocked_by: Vec<u64>,
        /// Set an `[extra]` value, as key=value
        #[arg(
            long,
            value_parser = parse_extra,
            value_name = "KEY=VALUE",
            long_help = "Set a value in the header's `[extra]` table, as key=value; the key
has to be one the repo config declares in `extra_keys`. Repeat for
several."
        )]
        set_extra: Vec<(String, String)>,
        /// Print JSON (version 1) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Give an item a new parent, or make it a root",
        long_about = "Changes the `parent` in one item's header (the TOML block between `+++`
lines that opens its description) and keeps the links and both parents'
`children` lists in step. The rest of the header and the body stay
as they were, and the link to the old parent stays when another reference
still needs it.

It refuses a header out of form, a new parent that is the item itself, an
item under it or its `from`, or a closed parent the repo's state policy
(its rule for when an item may be closed or open against its descendants)
would then report. When the item already has that parent, it says so and
writes nothing.",
        after_long_help = concat!(
            "Examples:
  gltree move 19 --to 12      # #19 becomes part of #12
  gltree move 19 --to-root    # #19 becomes a root

Output:
  Text: `#N moved from <old> to <new>`, or
  `#N is already <...>; nothing written`. --json prints {\"version\": 1,
  \"item\", \"from\", \"to\", \"changed\"}, where `from` and `to` are the parent
  before and after.

",
            requires_clone!(),
            writes_need_the_config!(),
            reads_the_config!(),
            exit_status_of_a_write!()
        )
    )]
    Move {
        /// The item to move, as N or #N
        #[arg(value_parser = parse_iid)]
        item: u64,
        /// The new parent, as N or #N
        #[arg(long, value_parser = parse_iid, value_name = "N", conflicts_with = "to_root", required_unless_present = "to_root")]
        to: Option<u64>,
        /// Make the item a root: remove its parent
        #[arg(long)]
        to_root: bool,
        /// Print JSON (version 1) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "List the roots, filtered by state, labels and closing date",
        long_about = "Lists the tracker's roots (items with no parent), one per line. Every filter
given has to hold, so they combine with AND: --open or --closed; every
--label has to be on the root; no --without-label may be on it, where a
trailing `*` matches every label that starts with the text before it; and
--closed-since and --closed-until keep the roots closed in that range of UTC
dates, both ends included, which implies --closed.",
        after_long_help = concat!(
            "Examples:
  gltree roots --open
  gltree roots --closed --without-label \"reg::*\" --json   # closed, no reg:: label
  gltree roots --closed-since 2026-10-19 --closed-until 2026-10-25 --label SPEC

Output:
  Text: `#N [state] closed:<date|-> labels:<a,b> title`. --json prints
  {\"version\": 2, \"roots\": [{number, title, state, labels, closed_at,
  first_line, url, from, extra, blocked_by_open}]}, where first_line is the
  body's first line after the header.

",
            requires_clone!(),
            reads_a_snapshot!(),
            reads_the_whole_tracker!(),
            reads_the_config!(),
            exit_status!()
        )
    )]
    Roots {
        /// Only open roots
        #[arg(long, conflicts_with = "closed")]
        open: bool,
        /// Only closed roots
        #[arg(long)]
        closed: bool,
        /// A label the root has to carry; repeat for several
        #[arg(long = "label", value_name = "LABEL")]
        labels: Vec<String>,
        /// A label the root must not carry; repeatable
        #[arg(
            long = "without-label",
            value_name = "LABEL",
            long_help = "A label the root must not carry; repeat for several. A trailing `*`
matches every label that starts with the text before it, as in
`reg::*`."
        )]
        without: Vec<String>,
        /// Closed on or after DATE (UTC, YYYY-MM-DD)
        #[arg(long, conflicts_with = "open", value_name = "DATE")]
        closed_since: Option<String>,
        /// Closed on or before DATE (UTC, YYYY-MM-DD)
        #[arg(long, conflicts_with = "open", value_name = "DATE")]
        closed_until: Option<String>,
        /// Read the tracker from FILE instead of the server
        #[arg(
            long,
            value_name = "FILE",
            long_help = "Read the tracker from FILE, a copy saved by `gltree snapshot`, instead
of the server"
        )]
        snapshot: Option<PathBuf>,
        /// Print JSON (version 2) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Turn first-line references into headers; a dry run unless --apply",
        long_about = "Before headers (the TOML block between `+++` lines that now opens a
description), a tracker recorded references in the first lines of each
description, the first-line format: `Parent: #4`, `From: #8`,
`Related: #30, #31`, `Blocked by: #40` or `Depends on: #40`, and a line
for each extra key the repo config declares. migrate turns that head into a
header and keeps the rest of the body byte for byte, brings every list
gltree keeps in step across the whole tracker, and adds every link a
reference lacks. In an item that already opens with a header, only the lists
gltree keeps are brought in step. A second run plans nothing.

It is a dry run unless --apply: it prints each item's old head (-) and new
header (+), the links it would add, and counts by kind. To see what `check`
would report afterwards without writing, save the result with --out and run
`gltree check --snapshot` on that file.",
        after_long_help = concat!(
            "Examples:
  gltree migrate --snapshot tracker.json --out migrated.json   # plan offline
  gltree check --snapshot migrated.json                        # judge the result
  gltree migrate --from-word Origin --apply                    # write it

Output:
  Text: the plan, item by item, then counts. --json prints the plan as
  version 1.

",
            requires_clone!(),
            reads_a_snapshot!(),
            "  Only --apply writes, and it cannot be combined with --snapshot.
",
            writes_need_the_config!(),
            reads_the_whole_tracker!(),
            reads_the_config!(),
            exit_status_of_a_write!()
        )
    )]
    Migrate {
        /// Another word for `From` in the old head
        #[arg(
            long,
            value_name = "WORD",
            long_help = "Another word the old head used for `From`; repeat for several"
        )]
        from_word: Vec<String>,
        /// Another word for `Related` in the old head
        #[arg(
            long,
            value_name = "WORD",
            long_help = "Another word the old head used for `Related`; repeat for several"
        )]
        related_word: Vec<String>,
        /// Plan from FILE instead of the server
        #[arg(
            long,
            value_name = "FILE",
            conflicts_with = "apply",
            long_help = "Plan from FILE, a copy saved by `gltree snapshot`, instead of the
server; a dry run only"
        )]
        snapshot: Option<PathBuf>,
        /// Save the migrated tracker to FILE
        #[arg(
            long,
            value_name = "FILE",
            long_help = "Save the tracker as it would read after the migration to FILE, a
snapshot `gltree check --snapshot FILE` reads"
        )]
        out: Option<PathBuf>,
        /// Write the plan to the server
        #[arg(long)]
        apply: bool,
        /// Print JSON (version 1) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Rewrite the lists gltree keeps: children, blocks, related",
        long_about = "Some lists in a header (the TOML block between `+++` lines that opens a
description) are kept by gltree, not written by people: `children` (the
items whose `parent` names this one), `blocks` (those whose `blocked_by`
names it) and the `related` mirror. sync works each out from the other
ends and rewrites the ones that differ. `related` becomes the union of
what the item lists and what lists it, so sync adds to it and never takes
away. Nothing else in the header or the body changes, and an item already
right is not written, so a second run writes nothing.

With a number it reads that item and the items linked to it, so a
reference without its link is not seen until `check`'s `unlinked` finding is
fixed. Without a number it reads the whole tracker once.",
        after_long_help = concat!(
            "Examples:
  gltree sync 12 --dry-run    # what would change on #12, written nowhere
  gltree sync 12              # rewrite #12's lists
  gltree sync --dry-run       # every item, written nowhere

Output:
  Text: `#N key [before] -> [after]` per change, then `N items would change`
  (or `changed`). --json prints {\"version\": 1, \"written\", \"changes\":
  [{item, key, before, after}]}.

",
            requires_clone!(),
            writes_need_the_config!(),
            "  --dry-run writes nothing and needs no repo config.
  Without a number, it reads the whole tracker.
",
            reads_the_whole_tracker!(),
            reads_the_config!(),
            exit_status!()
        )
    )]
    Sync {
        /// The item to sync, as N or #N; leave it out for every item
        #[arg(value_parser = parse_iid)]
        item: Option<u64>,
        /// Print what would change and write nothing
        #[arg(long)]
        dry_run: bool,
        /// Print JSON (version 1) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Print one item with its labels, body and comments",
        long_about = "Prints one item: its number, state and title, its labels, its whole
description with the header, and its comments oldest first, marking the
internal ones (visible to project members only). What people wrote is
indented under gltree's own lines. Read from the server, it needs GitLab
17.11 or later.",
        after_long_help = concat!(
            "Examples:
  gltree show 12
  gltree show 12 --snapshot tracker.json
  gltree show 12 --json

Output:
  Text, as above. --json prints {\"version\": 1, number, title, state,
  labels, body, comments}; comments is null when the source did not record
  them and [] when there are none.

",
            requires_clone!(),
            reads_a_snapshot!(),
            reads_the_config!(),
            exit_status!()
        )
    )]
    Show {
        /// The item, as N or #N
        #[arg(value_parser = parse_iid)]
        item: u64,
        /// Read the item from FILE instead of the server
        #[arg(
            long,
            value_name = "FILE",
            long_help = "Read the item from FILE, a copy saved by `gltree snapshot`, instead of
the server"
        )]
        snapshot: Option<PathBuf>,
        /// Print JSON (version 1) instead of text
        #[arg(long)]
        json: bool,
    },
    #[command(
        about = "Save every item, with links and comments, to read offline",
        long_about = "Reads every item of the tracker with its labels, links and comments and
saves them to FILE as JSON. tree, roots, check, show, migrate and serve
take --snapshot FILE to read that copy instead of the server, so they work
while the server is out of reach. The file is replaced only once the new
one is complete.

A snapshot can hold internal comments, as far as your token can read them:
save it outside the clone or list it in .gitignore. It needs GitLab 17.11 or
later, and reads the comments too, so it takes longer than `check`.",
        after_long_help = concat!(
            "Examples:
  gltree snapshot tracker.json
  gltree tree --snapshot tracker.json     # read the copy
  gltree check --snapshot tracker.json

Output:
  Text: `N items -> FILE`.

",
            requires_clone!(),
            reads_the_whole_tracker!(),
            reads_the_config!(),
            exit_status!()
        )
    )]
    Snapshot {
        /// The file to write
        #[arg(value_name = "FILE")]
        out: PathBuf,
    },
}

/// `key=value` for `--set-extra`.
fn parse_extra(s: &str) -> Result<(String, String), String> {
    s.split_once('=')
        .map(|(k, v)| (k.trim().to_string(), v.to_string()))
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| format!("not key=value: {s}"))
}

fn parse_iid(s: &str) -> Result<u64, String> {
    s.trim_start_matches('#')
        .parse()
        .map_err(|_| format!("not an item number: {s}"))
}

fn unreadable_dir(e: std::io::Error) -> TrackerError {
    TrackerError::unreadable("the current directory", e.to_string())
}

fn current_dir() -> Result<PathBuf, TrackerError> {
    std::env::current_dir().map_err(unreadable_dir)
}

/// The tracker of the clone gltree runs in, through the user's glab login.
fn server() -> Result<GitLab, TrackerError> {
    server_in(&current_dir()?)
}

fn server_in(dir: &Path) -> Result<GitLab, TrackerError> {
    let remote = Remote::of_clone(dir)?;
    let env = |k: &str| std::env::var(k).ok();
    let yaml = login::read_glab_config(&login::glab_config_paths(&env, std::env::home_dir()))?;
    let login = login::login(yaml.as_deref(), &remote.host, &env)?;
    GitLab::new(login, &remote.path)
}

/// The snapshot when one is named, else the server.
fn source(snapshot: Option<PathBuf>) -> Result<Box<dyn Tracker + Send>, TrackerError> {
    Ok(match snapshot {
        Some(path) => Box::new(SnapshotFile::open(path)?),
        None => Box::new(server()?),
    })
}

fn repo_list() -> Result<RepoList, TrackerError> {
    let home = std::env::home_dir()
        .ok_or_else(|| TrackerError::unreadable("the home folder", "the system names none"))?;
    RepoList::load(RepoList::file_in(&home))
}

fn registered_clone(path: &Path) -> Result<(), TrackerError> {
    if path.is_dir() {
        return Ok(());
    }
    Err(TrackerError::unreadable(
        format!("the clone {}", path.display()),
        "it is not there",
    ))
}

fn registered_config(path: &Path) -> Result<RepoConfig, TrackerError> {
    registered_clone(path)?;
    RepoConfig::of_clone(path).map_err(|e| {
        TrackerError::Refused(format!("{}: {}", path.join(config::FILE).display(), e.0))
    })
}

fn registered_tracker(path: &Path) -> Result<Box<dyn Tracker + Send>, TrackerError> {
    registered_clone(path)?;
    Ok(Box::new(server_in(path)?))
}

fn serve_command(
    list: Option<ServeList>,
    port: u16,
    snapshot: Option<PathBuf>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut out = std::io::stdout().lock();
    match list {
        Some(ServeList::Add { path, name }) => {
            let mut list = repo_list()?;
            let (top, remote) = repos::clone_at(&path)?;
            let name = name.unwrap_or_else(|| repos::default_name(&remote).to_string());
            list.add(&name, top.clone())?;
            list.save()?;
            writeln!(out, "registered {name}: {}", top.display())?;
        }
        Some(ServeList::List) => {
            for repo in repo_list()?.repos {
                writeln!(out, "{}  {}", repo.name, repo.path.display())?;
            }
        }
        Some(ServeList::Remove { name }) => {
            let mut list = repo_list()?;
            let gone = list.remove(&name)?;
            list.save()?;
            writeln!(out, "removed {}: {}", gone.name, gone.path.display())?;
        }
        None => {
            let registered = match snapshot {
                Some(_) => vec![],
                None => repo_list()?.repos,
            };
            if registered.is_empty() {
                serve_clone(port, snapshot)?;
            } else {
                serve_registered(port, registered)?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn listen(port: u16) -> Result<(std::net::TcpListener, u16), Box<dyn std::error::Error>> {
    let listener = serve::bind(port).map_err(|e| format!("127.0.0.1:{port}: {e}"))?;
    let port = listener.local_addr()?.port();
    Ok((listener, port))
}

fn print_address(port: u16) -> std::io::Result<()> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "http://127.0.0.1:{port}/")?;
    out.flush()
}

fn serve_clone(port: u16, snapshot: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let config = RepoConfig::of_clone(&current_dir()?)?;
    let (listener, port) = listen(port)?;
    let walk = snapshot.is_none();
    let first = source(snapshot.clone())?;
    let open: serve::Open = Box::new(move || source(snapshot.clone()));
    let app = Arc::new(serve::App::new(open, walk, config, port));
    print_address(port)?;
    eprintln!("gltree: reading the whole tracker");
    let began = std::time::Instant::now();
    app.start(0, Ok(first), move |read| match read {
        Ok(n) => eprintln!(
            "gltree: read {n} families in {:.1} s",
            began.elapsed().as_secs_f64()
        ),
        Err(e) => eprintln!("gltree: the first read failed: {e}"),
    });
    serve::serve(listener, app)?;
    Ok(())
}

fn serve_registered(
    port: u16,
    registered: Vec<repos::Repo>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (listener, port) = listen(port)?;
    let mut shown = Vec::with_capacity(registered.len());
    let mut firsts = Vec::with_capacity(registered.len());
    for repo in registered {
        let first = registered_tracker(&repo.path);
        let (opened, configured) = (repo.path.clone(), repo.path.clone());
        let open: serve::Open = Box::new(move || registered_tracker(&opened));
        let configure: serve::Configure = Box::new(move || registered_config(&configured));
        shown.push(serve::Repo::named(repo.name.clone(), open, configure, true));
        firsts.push((repo.name, first));
    }
    let app = Arc::new(serve::App::registered(shown, port));
    print_address(port)?;
    eprintln!("gltree: reading {} registered repos", firsts.len());
    let began = std::time::Instant::now();
    for (i, (name, first)) in firsts.into_iter().enumerate() {
        app.start(i, first, move |read| match read {
            Ok(n) => eprintln!(
                "gltree: {name}: read {n} families in {:.1} s",
                began.elapsed().as_secs_f64()
            ),
            Err(e) => eprintln!("gltree: {name}: the first read failed: {e}"),
        });
    }
    serve::serve(listener, app)?;
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("gltree: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Serve {
            list,
            port,
            snapshot,
        } => serve_command(list, port, snapshot),
        Command::InClone(command) => run_in_clone(command),
    }
}

fn run_in_clone(command: InClone) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let config = RepoConfig::of_clone(&current_dir()?)?;
    let scheme = config.scheme();
    let policy = config.policy();
    let rules = write::Rules {
        opted_in: config.present,
        policy: policy.as_ref(),
        extra_keys: &config.extra_keys,
    };
    let mut out = std::io::stdout().lock();
    match command {
        InClone::Tree {
            root,
            all,
            snapshot,
            json,
        } => {
            let tracker = source(snapshot.clone())?;
            let nodes = match (snapshot, root) {
                (None, Some(r)) => tree::live_tree(tracker.as_ref(), scheme.as_ref(), r)?,
                (_, Some(_)) => tree::tree(tracker.as_ref(), scheme.as_ref(), root)?,
                (_, None) => {
                    let nodes = tree::tree(tracker.as_ref(), scheme.as_ref(), None)?;
                    if all {
                        nodes
                    } else {
                        tree::with_open_work(nodes)
                    }
                }
            };
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&tree::to_json(&nodes))?
                )?;
            } else {
                for line in tree::render(&nodes) {
                    writeln!(out, "{line}")?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Check { snapshot, json } => {
            let items = source(snapshot)?.all_items()?;
            let found = check::check_with(
                &items,
                scheme.as_ref(),
                policy.as_ref(),
                &config.extra_keys,
                &config.old_head_keys,
            );
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&check::to_json(&found))?
                )?;
            } else {
                for line in check::render(&found) {
                    writeln!(out, "{line}")?;
                }
            }
            Ok(if found.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            })
        }
        InClone::New {
            parent,
            from,
            related,
            blocked_by,
            title,
            body_file,
            labels,
            json,
        } => {
            let bytes =
                std::fs::read(&body_file).map_err(|e| format!("{}: {e}", body_file.display()))?;
            let text = String::from_utf8(bytes)
                .map_err(|_| format!("{} is not UTF-8", body_file.display()))?;
            let labels = write::split_labels(&labels);
            write::require_opt_in(config.present)?;
            let refs = write::Refs {
                parent,
                from,
                related,
                blocked_by,
            };
            let made = write::new_item(&server()?, &rules, &refs, &title, &text, &labels)?;
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&write::new_json(&made))?
                )?;
            } else {
                let place = made
                    .link
                    .map_or("(a root)".to_string(), |(p, _)| format!("under #{p}"));
                let found = made.from.map_or(String::new(), |f| format!(", from #{f}"));
                writeln!(
                    out,
                    "#{} {place}{found}: {}",
                    made.created.iid, made.created.url
                )?;
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Edit {
            item,
            add_related,
            remove_related,
            add_blocked_by,
            remove_blocked_by,
            set_extra,
            json,
        } => {
            let edits = write::Edits {
                add_related,
                remove_related,
                add_blocked_by,
                remove_blocked_by,
                set_extra,
            };
            if edits == write::Edits::default() {
                return Err(
                    "edit needs at least one --add-*, --remove-* or --set-extra flag".into(),
                );
            }
            write::require_opt_in(config.present)?;
            let e = write::edit_item(&server()?, &rules, item, &edits)?;
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&write::edited_json(&e))?
                )?;
            } else {
                let list = |ns: &[u64]| {
                    let ns: Vec<String> = ns.iter().map(|n| format!("#{n}")).collect();
                    format!("[{}]", ns.join(", "))
                };
                let verb = if e.changed { "now" } else { "already" };
                writeln!(
                    out,
                    "#{} {verb} related {} blocked_by {}",
                    e.iid,
                    list(&e.related),
                    list(&e.blocked_by)
                )?;
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Move {
            item,
            to,
            to_root: _,
            json,
        } => {
            write::require_opt_in(config.present)?;
            let m = write::move_item(&server()?, &rules, item, to)?;
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&write::moved_json(&m))?
                )?;
            } else {
                let name =
                    |p: Option<u64>| p.map_or("a root".to_string(), |p| format!("under #{p}"));
                if m.changed {
                    writeln!(
                        out,
                        "#{} moved from {} to {}",
                        m.iid,
                        name(m.from),
                        name(m.to)
                    )?;
                } else {
                    writeln!(out, "#{} is already {}; nothing written", m.iid, name(m.to))?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Roots {
            open,
            closed,
            labels,
            without,
            closed_since,
            closed_until,
            snapshot,
            json,
        } => {
            let filter = roots::Filter {
                state: if open {
                    Some(State::Open)
                } else if closed {
                    Some(State::Closed)
                } else {
                    None
                },
                labels,
                without,
                closed_since,
                closed_until,
            };
            let items = source(snapshot)?.all_items()?;
            let found = roots::roots(&items, scheme.as_ref(), &filter)?;
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&roots::to_json(&found, &tree::states(&items)))?
                )?;
            } else {
                for line in roots::render(&found) {
                    writeln!(out, "{line}")?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Migrate {
            from_word,
            related_word,
            snapshot,
            out: save,
            apply,
            json,
        } => {
            if apply {
                write::require_opt_in(config.present)?;
            }
            let mut words = migrate::Words::default();
            words.from.extend(from_word);
            words.related.extend(related_word);
            let tracker = source(snapshot)?;
            let items = tracker.all_items()?;
            let plan = migrate::plan(&items, &words, &config.extra_keys)?;
            if let Some(path) = save {
                let after = migrate::applied(&items, &plan);
                SnapshotFile::save(&path, &after)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
            }
            if apply {
                migrate::apply(tracker.as_ref(), &plan)?;
            }
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&migrate::to_json(&plan, apply))?
                )?;
            } else {
                for line in migrate::render(&plan, apply) {
                    writeln!(out, "{line}")?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Sync {
            item,
            dry_run,
            json,
        } => {
            if !dry_run {
                write::require_opt_in(config.present)?;
            }
            let tracker = server()?;
            let changes = match item {
                Some(n) => derived::sync_one(&tracker, &config.extra_keys, n, dry_run)?,
                None => derived::sync_all(&tracker, &config.extra_keys, dry_run)?,
            };
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&derived::to_json(&changes, dry_run))?
                )?;
            } else {
                for line in derived::render(&changes, dry_run) {
                    writeln!(out, "{line}")?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Show {
            item,
            snapshot,
            json,
        } => {
            let item = source(snapshot)?.item_with_comments(item)?;
            if json {
                writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&show::to_json(&item))?
                )?;
            } else {
                for line in show::render(&item) {
                    writeln!(out, "{line}")?;
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        InClone::Snapshot { out: path } => {
            let items = server()?.items_with_comments()?;
            SnapshotFile::save(&path, &items).map_err(|e| format!("{}: {e}", path.display()))?;
            writeln!(out, "{} items -> {}", items.len(), path.display())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    fn examples(cmd: &clap::Command) -> Vec<String> {
        let after = cmd
            .get_after_long_help()
            .map(|s| s.to_string())
            .unwrap_or_default();
        after
            .split_once("Examples:\n")
            .map(|(_, rest)| {
                rest.lines()
                    .take_while(|l| l.is_empty() || l.starts_with(' '))
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.split("  # ").next().unwrap().trim().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn words(line: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        for c in line.chars() {
            match c {
                '"' => quoted = !quoted,
                ' ' if !quoted => {
                    if !word.is_empty() {
                        out.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        if !word.is_empty() {
            out.push(word);
        }
        out
    }

    #[test]
    fn every_command_explains_itself_with_examples_that_parse() {
        let mut top = Cli::command();
        top.build();
        let mut commands = vec![top.clone()];
        commands.extend(
            top.get_subcommands()
                .filter(|c| c.get_name() != "help")
                .flat_map(|c| std::iter::once(c).chain(c.get_subcommands()))
                .filter(|c| c.get_name() != "help")
                .cloned(),
        );
        assert!(commands.len() > 10);
        for nested in ["add", "list", "remove"] {
            assert!(
                commands.iter().any(|c| c.get_name() == nested),
                "serve {nested} is not checked"
            );
        }
        for cmd in &commands {
            let name = cmd.get_name();
            assert!(cmd.get_about().is_some(), "{name}: no about");
            assert!(cmd.get_long_about().is_some(), "{name}: no long about");
            let long = cmd
                .get_after_long_help()
                .map(|s| s.to_string())
                .unwrap_or_default();
            for heading in ["Requirements:", "Examples:", "Exit status:"] {
                assert!(long.contains(heading), "{name}: no {heading}");
            }
            if name != "gltree" {
                assert!(long.contains("Output:"), "{name}: no Output:");
            }
            let lines = examples(cmd);
            let takes_any = cmd
                .get_arguments()
                .any(|a| !matches!(a.get_id().as_str(), "help" | "version"));
            assert!(
                lines.len() >= if takes_any { 2 } else { 1 },
                "{name}: too few examples"
            );
            for line in lines {
                let words = words(&line);
                assert_eq!(words[0], "gltree", "{name}: {line}");
                if cmd.get_name() != "gltree" {
                    assert!(words.len() > 1, "{name}: {line}");
                }
                if let Err(e) = Cli::command().try_get_matches_from(&words) {
                    panic!("{name}: {line}\n{e}");
                }
            }
            for arg in cmd.get_arguments() {
                assert!(
                    arg.get_help().is_some(),
                    "{name}: {} has no help line",
                    arg.get_id()
                );
            }
        }
    }

    #[test]
    fn every_help_line_but_an_example_fits_80_columns() {
        let mut top = Cli::command();
        top.build();
        let mut commands = vec![top.clone()];
        commands.extend(
            top.get_subcommands()
                .flat_map(|c| std::iter::once(c).chain(c.get_subcommands()))
                .cloned(),
        );
        for mut cmd in commands {
            let name = cmd.get_name().to_string();
            for text in [cmd.render_help(), cmd.render_long_help()] {
                for line in text.to_string().lines() {
                    if line.starts_with("  gltree ") {
                        continue;
                    }
                    assert!(line.chars().count() <= 80, "{name}: {line}");
                }
            }
        }
    }

    #[test]
    fn the_usage_line_names_gltree_on_every_platform() {
        let mut cmd = Cli::command();
        let usage = cmd.render_usage().to_string();
        assert!(usage.contains("gltree <COMMAND>"), "{usage}");
        let err = Cli::command()
            .try_get_matches_from(["gltree.exe", "new"])
            .unwrap_err()
            .to_string();
        assert!(err.contains("gltree new"), "{err}");
        assert!(!err.contains("gltree.exe"), "{err}");
    }

    #[test]
    fn a_registered_config_that_fails_is_named_by_its_path_with_no_verb_of_ours() {
        let dir =
            std::env::temp_dir().join(format!("gltree-{}-registered-config", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&dir)
                .status()
                .unwrap()
                .success()
        );
        let file = dir.join(".gltree.toml");
        let named = format!("{}: ", file.display());
        std::fs::create_dir_all(&file).unwrap();
        let unread = super::registered_config(&dir).unwrap_err().to_string();
        std::fs::remove_dir_all(&file).unwrap();
        std::fs::write(&file, "state_policy = \"nope\"\n").unwrap();
        let refused = super::registered_config(&dir).unwrap_err().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        for m in [&unread, &refused] {
            assert!(m.starts_with(&named), "{m}");
            assert!(
                !m.contains("malformed") && !m.contains("cannot read"),
                "{m}"
            );
        }
        assert!(refused.contains("unknown state_policy"), "{refused}");
    }

    #[test]
    fn a_current_directory_that_cannot_be_read_is_named() {
        let m = super::unreadable_dir(std::io::Error::other("gone")).to_string();
        assert_eq!(m, "cannot read the current directory: gone");
    }
}
