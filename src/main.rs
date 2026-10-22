use clap::{Parser, Subcommand};
use gltree::check;
use gltree::config::RepoConfig;
use gltree::gitlab::GitLab;
use gltree::item::State;
use gltree::login;
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
            let made = write::new_item(&server()?, &rules, parent, from, &title, &text, &labels)?;
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
        Command::Snapshot { out: path } => {
            let items = server()?.all_items()?;
            std::fs::write(&path, SnapshotFile::to_bytes(&items))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            writeln!(out, "{} items -> {}", items.len(), path.display())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
