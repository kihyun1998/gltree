//! The GitLab host and project path a clone's `origin` remote points at.

use crate::tracker::Error;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub host: String,
    /// The project's full path, `group/sub/project`.
    pub path: String,
}

impl Remote {
    /// Reads `https://host/…`, `ssh://user@host[:port]/…` and `user@host:…`, with or without `.git`.
    pub fn parse(url: &str) -> Option<Remote> {
        let s = url.trim();
        let schemes = ["https://", "http://", "ssh://"];
        let (host, path) = if let Some(rest) = schemes.iter().find_map(|p| s.strip_prefix(p)) {
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?.split(':').next()?;
            (host, path)
        } else {
            let (user_host, path) = s.split_once(':')?;
            if user_host.contains('/') {
                return None;
            }
            (user_host.rsplit('@').next()?, path)
        };
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path).trim_matches('/');
        if host.is_empty() || !path.contains('/') {
            return None;
        }
        Some(Remote {
            host: host.into(),
            path: path.into(),
        })
    }

    pub fn of_clone(dir: &Path) -> Result<Remote, Error> {
        let out = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(dir)
            .output()
            .map_err(|e| Error::Source(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            return Err(Error::Source(
                "no `origin` remote here; run gltree inside a clone of the tracker's project"
                    .into(),
            ));
        }
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Remote::parse(&url)
            .ok_or_else(|| Error::Source(format!("cannot read the remote URL `{url}`")))
    }
}

#[cfg(test)]
mod tests {
    use super::Remote;

    fn r(host: &str, path: &str) -> Option<Remote> {
        Some(Remote {
            host: host.into(),
            path: path.into(),
        })
    }

    #[test]
    fn https_with_and_without_dot_git() {
        assert_eq!(
            Remote::parse("https://gitlab.example.com/team/app.git"),
            r("gitlab.example.com", "team/app")
        );
        assert_eq!(
            Remote::parse("https://gitlab.example.com/team/app"),
            r("gitlab.example.com", "team/app")
        );
        assert_eq!(
            Remote::parse("https://gitlab.example.com/team/app/"),
            r("gitlab.example.com", "team/app")
        );
    }

    #[test]
    fn https_with_a_user_and_a_port() {
        assert_eq!(
            Remote::parse("https://me@gitlab.example.com:8443/team/sub/app.git"),
            r("gitlab.example.com", "team/sub/app")
        );
    }

    #[test]
    fn scp_like_ssh() {
        assert_eq!(
            Remote::parse("git@gitlab.example.com:team/app.git"),
            r("gitlab.example.com", "team/app")
        );
    }

    #[test]
    fn ssh_url_with_a_port() {
        assert_eq!(
            Remote::parse("ssh://git@gitlab.example.com:2222/team/app.git"),
            r("gitlab.example.com", "team/app")
        );
    }

    #[test]
    fn not_a_remote() {
        assert_eq!(Remote::parse("team/app"), None);
        assert_eq!(Remote::parse("https://gitlab.example.com/"), None);
        assert_eq!(Remote::parse(""), None);
    }
}
