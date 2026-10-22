//! The repo config: `.gltree.toml` at a consuming repo's root, saying it uses gltree and how.

use crate::check::{EveryParent, Links, RootsOnly, StatePolicy};
use crate::scheme::{OriginLine, ParentLine, RelationScheme};
use crate::tracker::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FILE: &str = ".gltree.toml";
pub const SCHEMES: &[&str] = &["parent-line"];
pub const POLICIES: &[&str] = &["roots-only", "every-parent"];
pub const LINKS: &[&str] = &["tree-only", "shared"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoConfig {
    pub root_label: String,
    pub relation_scheme: String,
    pub state_policy: String,
    /// What the repo's links may join: `tree-only` or `shared` with related issues.
    pub links: String,
    /// The word of the origin line, `<word>: #N`; a committed config must set it.
    pub origin_word: Option<String>,
    /// Whether the repo committed a config file, which is what lets gltree write to its tracker.
    pub present: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    root_label: Option<String>,
    relation_scheme: Option<String>,
    state_policy: Option<String>,
    links: Option<String>,
    origin_word: Option<String>,
}

impl RepoConfig {
    pub fn defaults() -> RepoConfig {
        RepoConfig {
            root_label: "SPEC".into(),
            relation_scheme: SCHEMES[0].into(),
            state_policy: POLICIES[0].into(),
            links: LINKS[0].into(),
            origin_word: None,
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
            links: known("links", f.links, LINKS, d.links)?,
            origin_word: Some(origin_word(f.origin_word)?),
            present: true,
        })
    }

    /// The scheme `relation_scheme` names; `parse` admits only the names in `SCHEMES`.
    pub fn scheme(&self) -> Box<dyn RelationScheme> {
        Box::new(ParentLine)
    }

    /// The policy `state_policy` names; `parse` admits only the names in `POLICIES`.
    pub fn policy(&self) -> Box<dyn StatePolicy> {
        match self.state_policy.as_str() {
            "every-parent" => Box::new(EveryParent),
            _ => Box::new(RootsOnly),
        }
    }

    /// The origin line this repo writes; none without a config file.
    pub fn origin(&self) -> Option<OriginLine> {
        self.origin_word.as_deref().map(OriginLine::new)
    }

    pub fn links(&self) -> Links {
        if self.links == "shared" {
            Links::Shared
        } else {
            Links::TreeOnly
        }
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

fn origin_word(w: Option<String>) -> Result<String, Error> {
    let Some(w) = w else {
        return Err(Error::Malformed(format!(
            "{FILE}: `origin_word` is required: the word of the line `<word>: #N` that says where an item was found, e.g. origin_word = \"From\""
        )));
    };
    let foreign = |c: char| c == ':' || c == '#' || c.is_whitespace();
    if w.is_empty() || w == "Parent" || w.contains(foreign) {
        return Err(Error::Malformed(format!(
            "{FILE}: origin_word `{w}` must be one word, not `Parent`, without spaces, `:` or `#`"
        )));
    }
    Ok(w)
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

    const WORD: &str = "origin_word = \"From\"\n";

    fn with_word(rest: &str) -> String {
        format!("{WORD}{rest}")
    }

    #[test]
    fn a_file_with_only_the_origin_word_opts_in_with_the_defaults() {
        let c = RepoConfig::parse(Some(WORD)).unwrap();
        assert!(c.present);
        assert_eq!(
            (c.root_label.as_str(), c.state_policy.as_str()),
            ("SPEC", "roots-only")
        );
        assert_eq!(c.origin(), Some(OriginLine::new("From")));
        assert_eq!(RepoConfig::defaults().origin(), None);
    }

    #[test]
    fn the_origin_word_is_required_and_one_word() {
        let e = RepoConfig::parse(Some("")).unwrap_err().to_string();
        assert!(e.contains("origin_word") && e.contains("required"), "{e}");
        for bad in ["", "Parent", "Came from", "From:", "#", " From"] {
            let text = format!("origin_word = \"{bad}\"");
            assert!(RepoConfig::parse(Some(&text)).is_err(), "{bad:?}");
        }
        let c = RepoConfig::parse(Some("origin_word = \"출처\"")).unwrap();
        assert_eq!(c.origin_word.as_deref(), Some("출처"));
    }

    #[test]
    fn each_value_can_be_set() {
        let c = RepoConfig::parse(Some(&with_word(
            "root_label = \"PRD\"\nrelation_scheme = \"parent-line\"\nstate_policy = \"roots-only\"\n",
        )))
        .unwrap();
        assert_eq!(c.root_label, "PRD");
    }

    #[test]
    fn an_unknown_scheme_or_policy_names_the_known_ones() {
        let e = RepoConfig::parse(Some("relation_scheme = \"hierarchy\""))
            .unwrap_err()
            .to_string();
        assert!(e.contains("hierarchy") && e.contains("parent-line"), "{e}");
        let e = RepoConfig::parse(Some("state_policy = \"leaf-only\""))
            .unwrap_err()
            .to_string();
        assert!(e.contains("leaf-only") && e.contains("roots-only"), "{e}");
    }

    #[test]
    fn every_parent_is_a_known_policy_and_unknown_ones_list_both() {
        let c = RepoConfig::parse(Some(&with_word("state_policy = \"every-parent\""))).unwrap();
        assert_eq!(c.state_policy, "every-parent");
        // the name reaches the policy itself: a closed non-root over an open child is judged
        use crate::check::Family;
        use crate::item::{Item, State};
        let parent = Item::new(2, State::Closed, "t2", "Parent: #1");
        let child = Item::new(3, State::Open, "t3", "Parent: #2");
        let kids = [&child];
        let family = Family {
            is_root: false,
            children: &kids,
            descendants: &kids,
        };
        assert!(c.policy().judge(&parent, &family).is_some());
        assert!(
            RepoConfig::defaults()
                .policy()
                .judge(&parent, &family)
                .is_none()
        );
        let e = RepoConfig::parse(Some("state_policy = \"leaf-only\""))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("roots-only") && e.contains("every-parent"),
            "{e}"
        );
    }

    #[test]
    fn links_are_tree_only_unless_the_repo_shares_them() {
        assert_eq!(RepoConfig::parse(None).unwrap().links(), Links::TreeOnly);
        let shared = RepoConfig::parse(Some(&with_word("links = \"shared\""))).unwrap();
        assert_eq!(shared.links(), Links::Shared);
        let e = RepoConfig::parse(Some("links = \"any\""))
            .unwrap_err()
            .to_string();
        assert!(e.contains("tree-only") && e.contains("shared"), "{e}");
    }

    #[test]
    fn an_unknown_key_or_broken_toml_is_an_error() {
        assert!(RepoConfig::parse(Some("root_lable = \"SPEC\"")).is_err());
        assert!(RepoConfig::parse(Some("root_label = ")).is_err());
    }
}
