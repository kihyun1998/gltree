//! The repo config: `.gltree.toml` at a consuming repo's root, saying it uses gltree and how.

use crate::check::{RootsOnly, StatePolicy};
use crate::scheme::{ParentLine, RelationScheme};
use crate::tracker::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FILE: &str = ".gltree.toml";
pub const SCHEMES: &[&str] = &["parent-line"];
pub const POLICIES: &[&str] = &["roots-only"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoConfig {
    pub root_label: String,
    pub relation_scheme: String,
    pub state_policy: String,
    /// Whether the repo committed a config file, which is what lets gltree write to its tracker.
    pub present: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    root_label: Option<String>,
    relation_scheme: Option<String>,
    state_policy: Option<String>,
}

impl RepoConfig {
    pub fn defaults() -> RepoConfig {
        RepoConfig {
            root_label: "SPEC".into(),
            relation_scheme: SCHEMES[0].into(),
            state_policy: POLICIES[0].into(),
            present: false,
        }
    }

    /// The config from the file's text, or the defaults when there is no file.
    pub fn parse(text: Option<&str>) -> Result<RepoConfig, Error> {
        let Some(text) = text else {
            return Ok(RepoConfig::defaults());
        };
        let f: File = toml::from_str(text).map_err(|e| Error::Malformed(format!("{FILE}: {e}")))?;
        let d = RepoConfig::defaults();
        let known = |what: &str, v: Option<String>, all: &[&str], default: String| match v {
            None => Ok(default),
            Some(v) if all.contains(&v.as_str()) => Ok(v),
            Some(v) => Err(Error::Malformed(format!(
                "{FILE}: unknown {what} `{v}`; known: {}",
                all.join(", ")
            ))),
        };
        Ok(RepoConfig {
            root_label: f.root_label.unwrap_or(d.root_label),
            relation_scheme: known(
                "relation_scheme",
                f.relation_scheme,
                SCHEMES,
                d.relation_scheme,
            )?,
            state_policy: known("state_policy", f.state_policy, POLICIES, d.state_policy)?,
            present: true,
        })
    }

    /// The scheme `relation_scheme` names; `parse` admits only the names in `SCHEMES`.
    pub fn scheme(&self) -> Box<dyn RelationScheme> {
        Box::new(ParentLine)
    }

    /// The policy `state_policy` names; `parse` admits only the names in `POLICIES`.
    pub fn policy(&self) -> Box<dyn StatePolicy> {
        Box::new(RootsOnly)
    }

    /// The config of the clone `dir` is in; the defaults outside a clone.
    pub fn of_clone(dir: &Path) -> Result<RepoConfig, Error> {
        let Some(root) = toplevel(dir) else {
            return RepoConfig::parse(None);
        };
        match std::fs::read_to_string(root.join(FILE)) {
            Ok(text) => RepoConfig::parse(Some(&text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => RepoConfig::parse(None),
            Err(e) => Err(Error::Source(format!("{FILE}: {e}"))),
        }
    }
}

fn toplevel(dir: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_file_means_the_defaults_and_no_writing() {
        let c = RepoConfig::parse(None).unwrap();
        assert_eq!(c, RepoConfig::defaults());
        assert!(!c.present);
    }

    #[test]
    fn an_empty_file_opts_in_with_the_defaults() {
        let c = RepoConfig::parse(Some("")).unwrap();
        assert!(c.present);
        assert_eq!(
            (c.root_label.as_str(), c.state_policy.as_str()),
            ("SPEC", "roots-only")
        );
    }

    #[test]
    fn each_value_can_be_set() {
        let c = RepoConfig::parse(Some(
            "root_label = \"PRD\"\nrelation_scheme = \"parent-line\"\nstate_policy = \"roots-only\"\n",
        ))
        .unwrap();
        assert_eq!(c.root_label, "PRD");
    }

    #[test]
    fn an_unknown_scheme_or_policy_names_the_known_ones() {
        let e = RepoConfig::parse(Some("relation_scheme = \"hierarchy\""))
            .unwrap_err()
            .to_string();
        assert!(e.contains("hierarchy") && e.contains("parent-line"), "{e}");
        let e = RepoConfig::parse(Some("state_policy = \"every-parent\""))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("every-parent") && e.contains("roots-only"),
            "{e}"
        );
    }

    #[test]
    fn an_unknown_key_or_broken_toml_is_an_error() {
        assert!(RepoConfig::parse(Some("root_lable = \"SPEC\"")).is_err());
        assert!(RepoConfig::parse(Some("root_label = ")).is_err());
    }
}
