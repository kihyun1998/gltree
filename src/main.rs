use clap::{Parser, Subcommand};
use gltree::gitlab::GitLab;
use gltree::login;
use gltree::remote::Remote;
use gltree::scheme::ParentLine;
use gltree::snapshot::SnapshotFile;
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

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gltree: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Tree {
            root,
            snapshot,
            json,
        } => {
            let nodes = match (snapshot, root) {
                (Some(path), _) => tree::tree(&SnapshotFile { path }, &ParentLine, root)?,
                (None, Some(r)) => tree::live_tree(&server()?, &ParentLine, r)?,
                (None, None) => {
                    return Err("every root's tree reads every item, which is not built for the server yet; name a root, or pass --snapshot".into());
                }
            };
            let mut out = std::io::stdout().lock();
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
            Ok(())
        }
    }
}
