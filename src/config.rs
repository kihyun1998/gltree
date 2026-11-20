//! The repo config: `.gltree.toml` at a consuming repo's root, saying it uses gltree and how.

use crate::check::{EveryParent, RootsOnly, StatePolicy};
use crate::scheme::{FrontMatter, RelationScheme};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const FILE: &str = ".gltree.toml";
pub const SCHEMES: &[&str] = &["front-matter"];
pub const POLICIES: &[&str] = &["roots-only", "every-parent"];

/// Why the repo config was refused or could not be read; shown after the file's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{FILE}: {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoConfig {
    pub root_label: String,
    pub relation_scheme: String,
    pub state_policy: String,
    /// The keys the repo's header may hold under `[extra]`, kept as written and given no meaning.
    pub extra_keys: Vec<String>,
    /// The repo's own words of the first-line format, beyond the ones `check` knows, so a body
    /// still opening with one is reported as `old-header`.
    pub old_head_keys: Vec<String>,
    /// The label prefixes `serve` shows as badges on roots; gltree reads them and knows none by name.
    pub badges: Vec<Badge>,
    /// Whether the repo committed a config file, which is what lets gltree write to its tracker.
    pub present: bool,
}

/// A label prefix shown as a badge on a root, the label's rest being its value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Badge {
    pub prefix: String,
    pub name: String,
    /// An `[extra]` key whose value is shown beside the badge; one of `extra_keys`.
    pub extra: Option<String>,
    /// What to show for a closed root that carries no label with the prefix.
    pub missing: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    root_label: Option<String>,
    relation_scheme: Option<String>,
    state_policy: Option<String>,
    extra_keys: Option<Vec<String>>,
    old_head_keys: Option<Vec<String>>,
    badges: Option<Vec<Badge>>,
}

impl RepoConfig {
    pub fn defaults() -> RepoConfig {
        RepoConfig {
            root_label: "SPEC".into(),
            relation_scheme: SCHEMES[0].into(),
            state_policy: POLICIES[0].into(),
            extra_keys: vec![],
            old_head_keys: vec![],
            badges: vec![],
            present: false,
        }
    }

    /// The config from the file's text, or the defaults when there is no file.
    pub fn parse(text: Option<&str>) -> Result<RepoConfig, Error> {
        let Some(text) = text else {
            return Ok(RepoConfig::defaults());
        };
        let f: File = toml::from_str(text).map_err(|e| Error(e.to_string()))?;
        let d = RepoConfig::defaults();
        let known = |what: &str, v: Option<String>, all: &[&str], default: String| match v {
            None => Ok(default),
            Some(v) if all.contains(&v.as_str()) => Ok(v),
            Some(v) => Err(Error(format!(
                "unknown {what} `{v}`; known: {}",
                all.join(", ")
            ))),
        };
        let extra_keys = f.extra_keys.unwrap_or_default();
        if let Some(bad) = extra_keys.iter().find(|k| !bare_key(k)) {
            return Err(Error(format!(
                "extra key `{bad}` must be a bare TOML key: letters, digits, `_` or `-`"
            )));
        }
        let badges = f.badges.unwrap_or_default();
        for b in &badges {
            if b.prefix.is_empty() {
                return Err(Error("a badge's prefix must not be empty".into()));
            }
            if let Some(key) = &b.extra
                && !extra_keys.contains(key)
            {
                return Err(Error(format!(
                    "badge `{}` names extra key `{key}`, which extra_keys does not declare",
                    b.prefix
                )));
            }
        }
        Ok(RepoConfig {
            root_label: f.root_label.unwrap_or(d.root_label),
            relation_scheme: known(
                "relation_scheme",
                f.relation_scheme,
                SCHEMES,
                d.relation_scheme,
            )?,
            state_policy: known("state_policy", f.state_policy, POLICIES, d.state_policy)?,
            extra_keys,
            old_head_keys: f.old_head_keys.unwrap_or_default(),
            badges,
            present: true,
        })
    }

    /// The scheme `relation_scheme` names; `parse` admits only the names in `SCHEMES`.
    pub fn scheme(&self) -> Box<dyn RelationScheme> {
        Box::new(FrontMatter)
    }

    /// The policy `state_policy` names; `parse` admits only the names in `POLICIES`.
    pub fn policy(&self) -> Box<dyn StatePolicy> {
        match self.state_policy.as_str() {
            "every-parent" => Box::new(EveryParent),
            _ => Box::new(RootsOnly),
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
            Err(e) => Err(Error(e.to_string())),
        }
    }
}

fn bare_key(k: &str) -> bool {
    !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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
            (
                c.root_label.as_str(),
                c.relation_scheme.as_str(),
                c.state_policy.as_str()
            ),
            ("SPEC", "front-matter", "roots-only")
        );
        assert!(c.extra_keys.is_empty());
    }

    #[test]
    fn each_value_can_be_set() {
        let c = RepoConfig::parse(Some(
            "root_label = \"PRD\"\nrelation_scheme = \"front-matter\"\nstate_policy = \"roots-only\"\nextra_keys = [\"marker\", \"x-y_2\"]\nold_head_keys = [\"Origin\"]\n",
        ))
        .unwrap();
        assert_eq!(c.root_label, "PRD");
        assert_eq!(c.extra_keys, ["marker", "x-y_2"]);
        assert_eq!(c.old_head_keys, ["Origin"]);
    }

    #[test]
    fn an_extra_key_must_be_a_bare_toml_key() {
        for bad in ["", "a b", "a.b", "출처"] {
            let text = format!("extra_keys = [\"{bad}\"]");
            assert!(RepoConfig::parse(Some(&text)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_first_line_settings_are_gone() {
        for old in [
            "relation_scheme = \"parent-line\"",
            "origin_word = \"From\"",
            "links = \"shared\"",
        ] {
            assert!(RepoConfig::parse(Some(old)).is_err(), "{old}");
        }
    }

    #[test]
    fn an_unknown_scheme_or_policy_names_the_known_ones() {
        let e = RepoConfig::parse(Some("relation_scheme = \"hierarchy\""))
            .unwrap_err()
            .to_string();
        assert!(e.contains("hierarchy") && e.contains("front-matter"), "{e}");
        let e = RepoConfig::parse(Some("state_policy = \"leaf-only\""))
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("leaf-only") && e.contains("roots-only") && e.contains("every-parent"),
            "{e}"
        );
    }

    #[test]
    fn every_parent_reaches_the_policy_itself() {
        let c = RepoConfig::parse(Some("state_policy = \"every-parent\"")).unwrap();
        // a closed non-root over an open child is judged by every-parent, not by roots-only
        use crate::check::Family;
        use crate::item::{Item, State};
        let parent = Item::new(2, State::Closed, "t2", "+++\nparent = 1\n+++\n");
        let child = Item::new(3, State::Open, "t3", "+++\nparent = 2\n+++\n");
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
    }

    #[test]
    fn badges_are_read_with_their_optional_parts() {
        let c = RepoConfig::parse(Some(
            "extra_keys = [\"marker\"]\n\n[[badges]]\nprefix = \"reg::\"\nname = \"Registration\"\nextra = \"marker\"\nmissing = \"undecided\"\n\n[[badges]]\nprefix = \"area::\"\nname = \"Area\"\n",
        ))
        .unwrap();
        assert_eq!(
            c.badges,
            [
                Badge {
                    prefix: "reg::".into(),
                    name: "Registration".into(),
                    extra: Some("marker".into()),
                    missing: Some("undecided".into()),
                },
                Badge {
                    prefix: "area::".into(),
                    name: "Area".into(),
                    extra: None,
                    missing: None,
                },
            ]
        );
        assert!(RepoConfig::defaults().badges.is_empty());
    }

    #[test]
    fn a_badge_naming_an_undeclared_extra_key_is_refused() {
        let e = RepoConfig::parse(Some(
            "extra_keys = [\"marker\"]\n[[badges]]\nprefix = \"reg::\"\nname = \"R\"\nextra = \"other\"\n",
        ))
        .unwrap_err()
        .to_string();
        assert!(e.contains("other") && e.contains("extra_keys"), "{e}");
    }

    #[test]
    fn a_badge_needs_a_prefix_and_a_name_and_takes_no_other_key() {
        for bad in [
            "[[badges]]\nname = \"R\"\n",
            "[[badges]]\nprefix = \"reg::\"\n",
            "[[badges]]\nprefix = \"\"\nname = \"R\"\n",
            "[[badges]]\nprefix = \"reg::\"\nname = \"R\"\ncolour = \"red\"\n",
        ] {
            assert!(RepoConfig::parse(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_unknown_key_or_broken_toml_is_an_error() {
        assert!(RepoConfig::parse(Some("root_lable = \"SPEC\"")).is_err());
        assert!(RepoConfig::parse(Some("root_label = ")).is_err());
    }
}
