use clap::{Parser, Subcommand};
use gltree::check;
use gltree::config::RepoConfig;
use gltree::derived;
use gltree::gitlab::GitLab;
use gltree::item::State;
use gltree::login;
use gltree::migrate;
use gltree::remote::Remote;
use gltree::roots;
use gltree::snapshot::SnapshotFile;
use gltree::tracker::Tracker;
use gltree::tree;
use gltree::write;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "gltree",
    version,
    about = "Issue trees for GitLab trackers whose references sit in a TOML header at the top of each body"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The tree under an item with closed/all descendants per node; every root's tree without one
    Tree {
        /// The root item, as `N` or `#N`
        #[arg(value_parser = parse_iid)]
        root: Option<u64>,
        /// Read the tracker from a snapshot file instead of the server
        #[arg(long)]
        snapshot: Option<PathBuf>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Every way the tracker breaks the tree convention; exits 1 when there is any
    Check {
        /// Read the tracker from a snapshot file instead of the server
        #[arg(long)]
        snapshot: Option<PathBuf>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Create an item whose header names its parent and where it was found, linked to each, in one step
    New {
        /// The parent, as `N` or `#N`; without it the item is a root
        #[arg(long, value_parser = parse_iid)]
        parent: Option<u64>,
        /// The item it was found while working, as `N` or `#N`: the header's `from`, not a parent
        #[arg(long, value_parser = parse_iid)]
        from: Option<u64>,
        /// An item to list under `related`; repeat for several
        #[arg(long, value_parser = parse_iid)]
        related: Vec<u64>,
        /// An item this one cannot start before; repeat for several
        #[arg(long, value_parser = parse_iid)]
        blocked_by: Vec<u64>,
        #[arg(long)]
        title: String,
        /// The body, a UTF-8 file; gltree writes the header above it
        #[arg(long)]
        body_file: PathBuf,
        /// A label to apply; repeat it, or separate labels with commas
        #[arg(long = "label")]
        labels: Vec<String>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Add to or remove from an item's `related` and `blocked_by`, with the links and the other ends
    Edit {
        /// The item to edit, as `N` or `#N`
        #[arg(value_parser = parse_iid)]
        item: u64,
        #[arg(long, value_parser = parse_iid)]
        add_related: Vec<u64>,
        #[arg(long, value_parser = parse_iid)]
        remove_related: Vec<u64>,
        #[arg(long, value_parser = parse_iid)]
        add_blocked_by: Vec<u64>,
        #[arg(long, value_parser = parse_iid)]
        remove_blocked_by: Vec<u64>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Give an item a new parent, or make it a root, keeping its header and links in step
    Move {
        /// The item to move, as `N` or `#N`
        #[arg(value_parser = parse_iid)]
        item: u64,
        /// The new parent, as `N` or `#N`
        #[arg(long, value_parser = parse_iid, conflicts_with = "to_root", required_unless_present = "to_root")]
        to: Option<u64>,
        /// Make the item a root
        #[arg(long)]
        to_root: bool,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// The tracker's roots, filtered by state, labels and closing date
    Roots {
        /// Only open roots
        #[arg(long, conflicts_with = "closed")]
        open: bool,
        /// Only closed roots
        #[arg(long)]
        closed: bool,
        /// A label the root must carry; repeat for several
        #[arg(long = "label")]
        labels: Vec<String>,
        /// A label the root must not carry; a trailing `*` matches a prefix, as in `reg::*`
        #[arg(long = "without-label")]
        without: Vec<String>,
        /// Closed on or after this UTC date (YYYY-MM-DD); implies --closed
        #[arg(long, conflicts_with = "open")]
        closed_since: Option<String>,
        /// Closed on or before this UTC date (YYYY-MM-DD); implies --closed
        #[arg(long, conflicts_with = "open")]
        closed_until: Option<String>,
        /// Read the tracker from a snapshot file instead of the server
        #[arg(long)]
        snapshot: Option<PathBuf>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Turn a tracker written in the first-line format into headers; a dry run unless --apply
    Migrate {
        /// A word the old head used for `from`, beside `From`; repeat for several
        #[arg(long)]
        from_word: Vec<String>,
        /// A word the old head used for `related`, beside `Related`; repeat for several
        #[arg(long)]
        related_word: Vec<String>,
        /// Plan from a snapshot file instead of the server (a dry run only)
        #[arg(long, conflicts_with = "apply")]
        snapshot: Option<PathBuf>,
        /// Save the tracker as it would read after the migration, as a snapshot, for `check`
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write the plan to the server
        #[arg(long)]
        apply: bool,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Rewrite the lists gltree keeps — children, blocks, the related mirror — from the truth
    Sync {
        /// One item, as `N` or `#N`, read with the items linked to it; every item without it
        #[arg(value_parser = parse_iid)]
        item: Option<u64>,
        /// Report what would change and write nothing
        #[arg(long)]
        dry_run: bool,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Save every item of the tracker, with its links, to a file
    Snapshot {
        /// The file to write
        out: PathBuf,
    },
}

fn parse_iid(s: &str) -> Result<u64, String> {
    s.trim_start_matches('#')
        .parse()
        .map_err(|_| format!("not an item number: {s}"))
}

/// The tracker of the clone gltree runs in, through the user's glab login.
fn server() -> Result<GitLab, Box<dyn std::error::Error>> {
    let remote = Remote::of_clone(&std::env::current_dir()?)?;
    let env = |k: &str| std::env::var(k).ok();
    let yaml = login::glab_config_paths(&env, std::env::home_dir())
        .into_iter()
        .find_map(|p| std::fs::read_to_string(p).ok());
    let login = login::login(yaml.as_deref(), &remote.host, &env)?;
    Ok(GitLab::new(login, &remote.path)?)
}

/// The snapshot when one is named, else the server.
fn source(snapshot: Option<PathBuf>) -> Result<Box<dyn Tracker>, Box<dyn std::error::Error>> {
    Ok(match snapshot {
        Some(path) => Box::new(SnapshotFile { path }),
        None => Box::new(server()?),
    })
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
    let config = RepoConfig::of_clone(&std::env::current_dir()?)?;
    let scheme = config.scheme();
    let policy = config.policy();
    let rules = write::Rules {
        opted_in: config.present,
        policy: policy.as_ref(),
        extra_keys: &config.extra_keys,
    };
    let mut out = std::io::stdout().lock();
    match cli.command {
        Command::Tree {
            root,
            snapshot,
            json,
        } => {
            let tracker = source(snapshot.clone())?;
            let nodes = match (snapshot, root) {
                (None, Some(r)) => tree::live_tree(tracker.as_ref(), scheme.as_ref(), r)?,
                _ => tree::tree(tracker.as_ref(), scheme.as_ref(), root)?,
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
        Command::Check { snapshot, json } => {
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
        Command::New {
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
        Command::Edit {
            item,
            add_related,
            remove_related,
            add_blocked_by,
            remove_blocked_by,
            json,
        } => {
            let edits = write::Edits {
                add_related,
                remove_related,
                add_blocked_by,
                remove_blocked_by,
            };
            if edits == write::Edits::default() {
                return Err("edit needs at least one --add-* or --remove-* flag".into());
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
        Command::Move {
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
        Command::Roots {
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
        Command::Migrate {
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
                std::fs::write(&path, SnapshotFile::to_bytes(&after))
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
        Command::Sync {
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
        Command::Snapshot { out: path } => {
            let items = server()?.all_items()?;
            std::fs::write(&path, SnapshotFile::to_bytes(&items))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            writeln!(out, "{} items -> {}", items.len(), path.display())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
