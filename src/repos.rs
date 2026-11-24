//! The repos `gltree serve` shows: `~/.gltree/repos.toml`, each a name and the path of a clone.

use crate::remote::Remote;
use crate::snapshot::replace_file;
use crate::tracker::Error;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// One registered repo: the name its tab and `?repo=` use, and the absolute path of its clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default, rename = "repo")]
    repos: Vec<Repo>,
}

/// The list as kept in its file, in the order the repos were registered.
#[derive(Debug)]
pub struct RepoList {
    file: PathBuf,
    pub repos: Vec<Repo>,
}

impl RepoList {
    /// `repos.toml` in `.gltree` under `home`.
    pub fn file_in(home: &Path) -> PathBuf {
        home.join(".gltree").join("repos.toml")
    }

    /// The list `file` holds; empty when there is no file.
    pub fn load(file: PathBuf) -> Result<RepoList, Error> {
        let what = format!("the repo list {}", file.display());
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RepoList {
                    file,
                    repos: vec![],
                });
            }
            Err(e) => return Err(Error::unreadable(what, e.to_string())),
        };
        let read: File =
            toml::from_str(&text).map_err(|e| Error::malformed(&what, e.to_string()))?;
        let mut list = RepoList {
            file,
            repos: Vec::with_capacity(read.repos.len()),
        };
        for repo in read.repos {
            list.add(&repo.name, repo.path)
                .map_err(|e| Error::malformed(&what, e.to_string()))?;
        }
        Ok(list)
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Adds `name` for the clone at `path`; refuses a name taken or not made of letters, digits,
    /// `.`, `_` and `-`, and a path that is not absolute.
    pub fn add(&mut self, name: &str, path: PathBuf) -> Result<(), Error> {
        if let Some(why) = name_error(name) {
            return Err(Error::Refused(why));
        }
        if self.repos.iter().any(|r| r.name == name) {
            return Err(Error::Refused(format!(
                "a repo is already registered as `{name}`; give another with --name"
            )));
        }
        if !path.is_absolute() {
            return Err(Error::Refused(format!(
                "`{}` is not an absolute path",
                path.display()
            )));
        }
        self.repos.push(Repo {
            name: name.into(),
            path,
        });
        Ok(())
    }

    /// Takes `name` off the list and returns it.
    pub fn remove(&mut self, name: &str) -> Result<Repo, Error> {
        match self.repos.iter().position(|r| r.name == name) {
            Some(i) => Ok(self.repos.remove(i)),
            None if self.repos.is_empty() => Err(Error::Refused(format!(
                "no repo is registered as `{name}`; none is registered"
            ))),
            None => Err(Error::Refused(format!(
                "no repo is registered as `{name}`; registered: {}",
                self.repos
                    .iter()
                    .map(|r| r.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }

    /// Writes the list to its file, making its folder when there is none; the file is replaced only
    /// once the new one is written.
    pub fn save(&self) -> Result<(), Error> {
        let what = || format!("the repo list {}", self.file.display());
        let text = toml::to_string(&File {
            repos: self.repos.clone(),
        })
        .map_err(|e| Error::malformed(what(), e.to_string()))?;
        let unwritten = |e: std::io::Error| Error::Refused(format!("cannot write {}: {e}", what()));
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).map_err(unwritten)?;
        }
        replace_file(&self.file, |f| f.write_all(text.as_bytes())).map_err(unwritten)
    }
}

/// Why `name` cannot name a repo, or `None` when it can.
pub fn name_error(name: &str) -> Option<String> {
    let fits = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    (!fits).then(|| {
        format!("`{name}` cannot name a repo: use 1 to 64 letters, digits, `.`, `_` or `-`")
    })
}

/// The clone `serve add` registers for `path`: its top folder and the GitLab project its `origin`
/// names. Refuses a path that is not inside a git clone.
pub fn clone_at(path: &Path) -> Result<(PathBuf, Remote), Error> {
    let Some(top) = crate::config::toplevel(path) else {
        return Err(Error::Refused(format!(
            "`{}` is not a git clone",
            path.display()
        )));
    };
    let remote = Remote::of_clone(&top)?;
    Ok((top, remote))
}

/// The name a clone takes when none is given: the last part of its project's path.
pub fn default_name(remote: &Remote) -> &str {
    remote.path.rsplit('/').next().unwrap_or(&remote.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gltree-{}-repos-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_file_is_an_empty_list() {
        let dir = scratch("none");
        let r = RepoList::load(RepoList::file_in(&dir)).unwrap();
        assert!(r.repos.is_empty());
        assert_eq!(r.file(), dir.join(".gltree").join("repos.toml"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_saved_list_reads_back_in_the_order_it_was_registered_and_makes_its_folder() {
        let dir = scratch("round");
        let file = RepoList::file_in(&dir);
        let mut r = RepoList::load(file.clone()).unwrap();
        r.add("web", dir.join("web")).unwrap();
        r.add("app", dir.join("app")).unwrap();
        r.save().unwrap();
        let back = RepoList::load(file.clone()).unwrap();
        assert_eq!(
            back.repos,
            [
                Repo {
                    name: "web".into(),
                    path: dir.join("web")
                },
                Repo {
                    name: "app".into(),
                    path: dir.join("app")
                }
            ]
        );
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(text.matches("[[repo]]").count(), 2, "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hand_written_list_reads_as_written() {
        let dir = scratch("hand");
        let file = RepoList::file_in(&dir);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let app = dir.join("app");
        std::fs::write(
            &file,
            format!(
                "[[repo]]\nname = \"app\"\npath = {}\n",
                toml::Value::String(app.to_string_lossy().into_owned())
            ),
        )
        .unwrap();
        let r = RepoList::load(file).unwrap();
        assert_eq!(
            r.repos,
            [Repo {
                name: "app".into(),
                path: app
            }]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_refuses_a_taken_name_a_bad_name_and_a_relative_path_and_keeps_the_list() {
        let dir = scratch("refuse");
        let mut r = RepoList::load(RepoList::file_in(&dir)).unwrap();
        r.add("app", dir.join("app")).unwrap();
        for (name, path, says) in [
            ("app", dir.join("other"), "already registered as `app`"),
            ("", dir.join("x"), "cannot name a repo"),
            ("a b", dir.join("x"), "cannot name a repo"),
            ("a&b", dir.join("x"), "cannot name a repo"),
            ("a/b", dir.join("x"), "cannot name a repo"),
            ("x", PathBuf::from("relative"), "not an absolute path"),
        ] {
            let e = r.add(name, path).unwrap_err().to_string();
            assert!(e.contains(says), "{name}: {e}");
        }
        assert!(r.add(&"a".repeat(65), dir.join("x")).is_err());
        r.add(&"a".repeat(64), dir.join("x")).unwrap();
        r.add("my.app_2-x", dir.join("y")).unwrap();
        assert_eq!(r.repos.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_takes_one_off_and_an_unknown_name_names_the_registered_ones() {
        let dir = scratch("remove");
        let mut r = RepoList::load(RepoList::file_in(&dir)).unwrap();
        let e = r.remove("app").unwrap_err().to_string();
        assert!(e.contains("none is registered"), "{e}");
        r.add("app", dir.join("app")).unwrap();
        r.add("web", dir.join("web")).unwrap();
        let e = r.remove("nope").unwrap_err().to_string();
        assert!(
            e.contains("`nope`") && e.contains("registered: app, web"),
            "{e}"
        );
        assert_eq!(r.remove("app").unwrap().path, dir.join("app"));
        assert_eq!(
            r.repos.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["web"]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_list_that_breaks_the_rules_is_malformed_and_names_its_file() {
        let dir = scratch("malformed");
        let file = RepoList::file_in(&dir);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let abs = toml::Value::String(dir.join("a").to_string_lossy().into_owned()).to_string();
        for (text, says) in [
            (
                format!(
                    "[[repo]]\nname = \"a\"\npath = {abs}\n[[repo]]\nname = \"a\"\npath = {abs}\n"
                ),
                "already registered",
            ),
            (
                format!("[[repo]]\nname = \"a b\"\npath = {abs}\n"),
                "cannot name a repo",
            ),
            (
                "[[repo]]\nname = \"a\"\npath = \"rel\"\n".to_string(),
                "not an absolute path",
            ),
            (
                format!("[[repo]]\nname = \"a\"\npath = {abs}\nport = 1\n"),
                "port",
            ),
            ("not toml".to_string(), "repos.toml"),
        ] {
            std::fs::write(&file, &text).unwrap();
            let e = RepoList::load(file.clone()).unwrap_err().to_string();
            assert!(
                e.contains("the repo list")
                    && e.contains("repos.toml")
                    && e.contains("malformed")
                    && e.contains(says),
                "{text}: {e}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_list_that_cannot_be_written_says_it_could_not_write_it() {
        let dir = scratch("unwritable");
        std::fs::write(dir.join(".gltree"), "a file where the folder goes").unwrap();
        let mut r = RepoList::load(RepoList::file_in(&dir)).unwrap();
        r.add("app", dir.join("app")).unwrap();
        let e = r.save().unwrap_err().to_string();
        assert!(
            e.starts_with("cannot write the repo list ") && e.contains("repos.toml"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_name_is_the_last_part_of_the_project_path() {
        let r = Remote {
            host: "gitlab.example.com".into(),
            path: "team/sub/app".into(),
        };
        assert_eq!(default_name(&r), "app");
    }
}
