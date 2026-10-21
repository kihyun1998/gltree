//! The API address, token and CA certificate for a host, from the user's glab login.

use crate::tracker::Error;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    /// `https://host`, without a trailing slash.
    pub api_base: String,
    pub token: String,
    pub ca_cert: Option<PathBuf>,
}

/// Overrides glab's token for every host.
pub const TOKEN_ENV: &str = "GITLAB_TOKEN";
/// Overrides glab's `ca_cert` for every host.
pub const CA_ENV: &str = "GLTREE_CA_CERT";

#[derive(Deserialize, Default)]
struct GlabConfig {
    #[serde(default)]
    hosts: HashMap<String, GlabHost>,
}

#[derive(Deserialize, Default)]
struct GlabHost {
    token: Option<String>,
    ca_cert: Option<String>,
    api_host: Option<String>,
    api_protocol: Option<String>,
}

/// Where glab keeps its config, in the order glab searches: `GLAB_CONFIG_DIR` alone when set,
/// else `~/.config/glab-cli`, then the platform config directory.
pub fn glab_config_paths(
    env: &dyn Fn(&str) -> Option<String>,
    home: Option<PathBuf>,
) -> Vec<PathBuf> {
    let set = |k: &str| env(k).filter(|v| !v.is_empty());
    if let Some(dir) = set("GLAB_CONFIG_DIR") {
        return vec![PathBuf::from(dir).join("config.yml")];
    }
    let mut paths = Vec::new();
    if let Some(h) = &home {
        paths.push(h.join(".config").join("glab-cli").join("config.yml"));
    }
    let platform = if cfg!(windows) {
        set("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home.as_ref()
            .map(|h| h.join("Library").join("Application Support"))
    } else {
        home.as_ref().map(|h| h.join(".config"))
    };
    if let Some(dir) = set("XDG_CONFIG_HOME").map(PathBuf::from).or(platform) {
        let p = dir.join("glab-cli").join("config.yml");
        if !paths.contains(&p) {
            paths.push(p);
        }
    }
    paths
}

/// The login for `host` from glab's config text (`None` when there is no config), with the
/// environment overrides applied.
pub fn login(
    glab_yaml: Option<&str>,
    host: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Login, Error> {
    let config: GlabConfig = match glab_yaml {
        Some(y) => {
            serde_yaml_ng::from_str(y).map_err(|e| Error::Malformed(format!("glab config: {e}")))?
        }
        None => GlabConfig::default(),
    };
    let entry = config.hosts.get(host);
    let set = |v: Option<String>| v.filter(|s| !s.is_empty());
    let token = set(env(TOKEN_ENV))
        .or_else(|| set(entry.and_then(|h| h.token.clone())))
        .ok_or_else(|| {
            Error::Source(format!(
                "no token for {host}: glab's config file holds none (glab may keep it in the OS keyring); set {TOKEN_ENV}"
            ))
        })?;
    let protocol =
        set(entry.and_then(|h| h.api_protocol.clone())).unwrap_or_else(|| "https".into());
    let api_host = set(entry.and_then(|h| h.api_host.clone())).unwrap_or_else(|| host.into());
    let ca_cert = set(env(CA_ENV))
        .or_else(|| set(entry.and_then(|h| h.ca_cert.clone())))
        .map(PathBuf::from);
    Ok(Login {
        api_base: format!("{protocol}://{api_host}"),
        token,
        ca_cert,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "\
git_protocol: ssh
host: gitlab.com
hosts:
    gitlab.com:
        api_protocol: https
        api_host: gitlab.com
        token: public-token
    gitlab.example.com:
        ca_cert: C:\\certs\\example.crt
        token: example-token
        api_host: gitlab.example.com
        api_protocol: https
        user: someone
";

    fn none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn reads_the_hosts_token_api_address_and_ca() {
        assert_eq!(
            login(Some(YAML), "gitlab.example.com", &none).unwrap(),
            Login {
                api_base: "https://gitlab.example.com".into(),
                token: "example-token".into(),
                ca_cert: Some(PathBuf::from("C:\\certs\\example.crt")),
            }
        );
    }

    #[test]
    fn a_host_without_a_ca_uses_the_default_roots() {
        assert_eq!(
            login(Some(YAML), "gitlab.com", &none).unwrap().ca_cert,
            None
        );
    }

    #[test]
    fn the_environment_overrides_token_and_ca() {
        let env = |k: &str| match k {
            TOKEN_ENV => Some("env-token".to_string()),
            CA_ENV => Some("/etc/ca.pem".to_string()),
            _ => None,
        };
        let l = login(Some(YAML), "gitlab.example.com", &env).unwrap();
        assert_eq!(
            (l.token.as_str(), l.ca_cert),
            ("env-token", Some(PathBuf::from("/etc/ca.pem")))
        );
    }

    #[test]
    fn the_environment_alone_is_enough_without_glab() {
        let env = |k: &str| (k == TOKEN_ENV).then(|| "env-token".to_string());
        let l = login(None, "gitlab.example.com", &env).unwrap();
        assert_eq!(
            (l.api_base.as_str(), l.token.as_str()),
            ("https://gitlab.example.com", "env-token")
        );
    }

    #[test]
    fn no_token_anywhere_names_the_variable_to_set() {
        let yaml = "hosts:\n    gitlab.example.com:\n        use_keyring: true\n";
        let e = login(Some(yaml), "gitlab.example.com", &none)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains(TOKEN_ENV) && e.contains("gitlab.example.com"),
            "{e}"
        );
        let e = login(Some(YAML), "other.example.com", &none)
            .unwrap_err()
            .to_string();
        assert!(e.contains(TOKEN_ENV), "{e}");
    }

    #[test]
    fn a_broken_config_is_an_error_not_a_missing_host() {
        assert!(matches!(
            login(Some("hosts: [unclosed"), "gitlab.example.com", &none),
            Err(Error::Malformed(_))
        ));
    }

    #[test]
    fn glab_config_dir_alone_when_set() {
        let env = |k: &str| (k == "GLAB_CONFIG_DIR").then(|| "/cfg".to_string());
        assert_eq!(
            glab_config_paths(&env, Some("/home/u".into())),
            [PathBuf::from("/cfg").join("config.yml")]
        );
    }

    #[test]
    fn otherwise_the_legacy_place_then_the_platform_one() {
        let env = |k: &str| (k == "XDG_CONFIG_HOME").then(|| "/xdg".to_string());
        assert_eq!(
            glab_config_paths(&env, Some("/home/u".into())),
            [
                PathBuf::from("/home/u")
                    .join(".config")
                    .join("glab-cli")
                    .join("config.yml"),
                PathBuf::from("/xdg").join("glab-cli").join("config.yml"),
            ]
        );
    }
}
