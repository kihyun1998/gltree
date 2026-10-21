use clap::{Parser, Subcommand};
use gltree::check;
use gltree::config::RepoConfig;
use gltree::gitlab::GitLab;
use gltree::login;
use gltree::remote::Remote;
use gltree::snapshot::SnapshotFile;
use gltree::tracker::Tracker;
use gltree::tree;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "gltree",
    version,
    about = "Issue trees for GitLab trackers whose parent is the body's first line `Parent: #N`"
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
            let found = check::check(&items, scheme.as_ref(), config.policy().as_ref());
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
        Command::Snapshot { out: path } => {
            let items = server()?.all_items()?;
            std::fs::write(&path, SnapshotFile::to_bytes(&items))
                .map_err(|e| format!("{}: {e}", path.display()))?;
            writeln!(out, "{} items -> {}", items.len(), path.display())?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
