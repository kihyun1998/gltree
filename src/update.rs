//! `gltree update`: installs gltree's latest GitHub release over the running binary, or says what
//! is installed and what is latest.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The environment variable that replaces `https://api.github.com` as the address the latest
/// release is asked from.
pub const API_ENV: &str = "GLTREE_GITHUB_API";
const API: &str = "https://api.github.com";
const LATEST: &str = "/repos/kihyun1998/gltree/releases/latest";
const ARCHIVE_LIMIT: u64 = 256 * 1024 * 1024;

/// Why `gltree update` stopped; the running binary is in place whenever one is returned.
#[derive(Debug)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn fail<T>(message: impl Into<String>) -> Result<T, Error> {
    Err(Error(message.into()))
}

/// A release version, three numbers compared in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u64, u64, u64);

impl Version {
    /// Reads `MAJOR.MINOR.PATCH`, each a number, and nothing else.
    pub fn parse(text: &str) -> Option<Version> {
        let mut parts = text.split('.');
        let mut next = || -> Option<u64> {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            part.parse().ok()
        };
        let version = Version(next()?, next()?, next()?);
        parts.next().is_none().then_some(version)
    }

    /// The version this binary was built as.
    pub fn running() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is three numbers")
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// The release target whose archive replaces this binary, as the release workflow names its
/// archives; on Linux the static musl build, whichever C library this binary was built against.
pub fn target() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("x86_64-unknown-linux-musl")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("aarch64-unknown-linux-musl")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("x86_64-apple-darwin")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("aarch64-apple-darwin")
    } else if cfg!(all(
        target_os = "windows",
        target_arch = "x86_64",
        target_env = "msvc"
    )) {
        Some("x86_64-pc-windows-msvc")
    } else if cfg!(all(
        target_os = "windows",
        target_arch = "aarch64",
        target_env = "msvc"
    )) {
        Some("aarch64-pc-windows-msvc")
    } else {
        None
    }
}

fn platform() -> String {
    let env = if cfg!(target_env = "musl") {
        "-musl"
    } else if cfg!(target_env = "gnu") {
        "-gnu"
    } else if cfg!(target_env = "msvc") {
        "-msvc"
    } else {
        ""
    };
    format!("{}-{}{env}", std::env::consts::ARCH, std::env::consts::OS)
}

fn archive_name(version: Version, target: &str) -> String {
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    format!("gltree-v{version}-{target}.{ext}")
}

/// The package manager that installed a binary, told from where the binary lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Scoop,
    Homebrew,
    Cargo,
}

impl Manager {
    pub fn name(self) -> &'static str {
        match self {
            Manager::Scoop => "Scoop",
            Manager::Homebrew => "Homebrew",
            Manager::Cargo => "cargo",
        }
    }

    /// The manager's own command that updates gltree.
    pub fn command(self) -> &'static str {
        match self {
            Manager::Scoop => "scoop update gltree",
            Manager::Homebrew => "brew upgrade gltree",
            Manager::Cargo => {
                "cargo install --git https://github.com/kihyun1998/gltree --locked --force"
            }
        }
    }
}

/// Scoop keeps an app under `apps/<app>/`, Homebrew a formula under `Cellar/<formula>/`, and
/// cargo installs into a root's `bin` beside the `.crates.toml` it keeps there.
pub fn manager_of(exe: &Path) -> Option<Manager> {
    let names: Vec<String> = exe
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    let holds = |outer: &str| names.windows(2).any(|w| w[0] == outer && w[1] == "gltree");
    if holds("apps") {
        return Some(Manager::Scoop);
    }
    if holds("cellar") {
        return Some(Manager::Homebrew);
    }
    let bin = exe.parent()?;
    let in_bin = bin
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("bin"));
    if in_bin && bin.parent()?.join(".crates.toml").is_file() {
        return Some(Manager::Cargo);
    }
    None
}

/// What `gltree update` found or did.
#[derive(Debug)]
pub enum Report {
    Checked {
        installed: Version,
        latest: Version,
    },
    UpToDate {
        installed: Version,
        latest: Version,
    },
    Managed {
        manager: Manager,
        installed: Version,
        latest: Version,
        path: PathBuf,
    },
    Replaced {
        from: Version,
        to: Version,
        path: PathBuf,
        manager: Option<Manager>,
    },
}

/// The lines `gltree update` prints for a report.
pub fn render(report: &Report) -> Vec<String> {
    match report {
        Report::Checked { installed, latest } => {
            vec![format!("installed {installed}"), format!("latest {latest}")]
        }
        Report::UpToDate { installed, latest } => vec![format!(
            "gltree {installed} is up to date: the latest release is {latest}"
        )],
        Report::Managed {
            manager,
            installed,
            latest,
            path,
        } => vec![
            format!(
                "gltree {installed} at {} was installed by {}; the latest release is {latest}",
                shown(path),
                manager.name()
            ),
            format!(
                "run `{}` to update it, or `gltree update --force` to replace it here",
                manager.command()
            ),
        ],
        Report::Replaced {
            from,
            to,
            path,
            manager,
        } => {
            let mut lines = vec![format!("updated gltree {from} -> {to}: {}", shown(path))];
            if let Some(m) = manager {
                lines.push(format!(
                    "{} still lists {from} until `{}` runs",
                    m.name(),
                    m.command()
                ));
            }
            lines
        }
    }
}

fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_string(),
        _ => text,
    }
}

/// Asks GitHub for the latest release and, unless `check`, installs it over the running binary
/// when it is newer; a binary a package manager installed is replaced only with `force`.
pub fn update(check: bool, force: bool) -> Result<Report, Error> {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .or_else(|e| fail(format!("cannot read the running binary's path: {e}")))?;
    if !check {
        clear_leftovers(&exe);
    }
    let agent = agent();
    let base = std::env::var(API_ENV).unwrap_or_else(|_| API.to_string());
    let release = latest(&agent, format!("{}{LATEST}", base.trim_end_matches('/')))?;
    let installed = Version::running();
    if check {
        return Ok(Report::Checked {
            installed,
            latest: release.version,
        });
    }
    if release.version <= installed {
        return Ok(Report::UpToDate {
            installed,
            latest: release.version,
        });
    }
    let manager = manager_of(&exe);
    if let Some(manager) = manager
        && !force
    {
        return Ok(Report::Managed {
            manager,
            installed,
            latest: release.version,
            path: exe,
        });
    }
    let Some(target) = target() else {
        return fail(format!(
            "no release archive is published for {}; build gltree from source",
            platform()
        ));
    };
    let name = archive_name(release.version, target);
    let sums_address = release.asset("SHA256SUMS")?;
    let archive_address = release.asset(&name)?;
    let sums = String::from_utf8(get(&agent, sums_address, ARCHIVE_LIMIT)?)
        .or_else(|_| fail(format!("SHA256SUMS of {} is not text", release.tag)))?;
    let Some(expected) = sum_for(&sums, &name) else {
        return fail(format!(
            "SHA256SUMS of {} has no line for {name}; nothing was replaced",
            release.tag
        ));
    };
    let archive = get(&agent, archive_address, ARCHIVE_LIMIT)?;
    let actual = hex(&Sha256::digest(&archive));
    if actual != expected {
        return fail(format!(
            "{name} does not match SHA256SUMS of {} (expected {expected}, got {actual}); \
             nothing was replaced",
            release.tag
        ));
    }
    let scratch = Scratch::new()?;
    let binary = unpack(&scratch.0, &name, &archive, release.version, target)?;
    replace(&exe, &binary)?;
    Ok(Report::Replaced {
        from: installed,
        to: release.version,
        path: exe,
        manager,
    })
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent(format!("gltree/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

struct Release {
    address: String,
    tag: String,
    version: Version,
    assets: Vec<(String, String)>,
}

impl Release {
    fn asset(&self, name: &str) -> Result<&str, Error> {
        match self.assets.iter().find(|(n, _)| n == name) {
            Some((_, url)) => Ok(url),
            None => fail(format!(
                "the release {} at {} has no asset {name}",
                self.tag, self.address
            )),
        }
    }
}

fn latest(agent: &ureq::Agent, address: String) -> Result<Release, Error> {
    let body = get(agent, &address, 10 * 1024 * 1024)?;
    let malformed = |why: &str| {
        Error(format!(
            "the release answer from {address} is malformed: {why}"
        ))
    };
    let value: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| malformed(&e.to_string()))?;
    let tag = value["tag_name"]
        .as_str()
        .ok_or_else(|| malformed("no tag_name"))?
        .to_string();
    let version = tag
        .strip_prefix('v')
        .and_then(Version::parse)
        .ok_or_else(|| malformed(&format!("the tag {tag:?} is not vMAJOR.MINOR.PATCH")))?;
    let assets = value["assets"]
        .as_array()
        .ok_or_else(|| malformed("no assets"))?
        .iter()
        .filter_map(|a| {
            Some((
                a["name"].as_str()?.to_string(),
                a["browser_download_url"].as_str()?.to_string(),
            ))
        })
        .collect();
    Ok(Release {
        address,
        tag,
        version,
        assets,
    })
}

fn get(agent: &ureq::Agent, address: &str, limit: u64) -> Result<Vec<u8>, Error> {
    let mut response = agent
        .get(address)
        .header("Accept", "application/vnd.github+json")
        .call()
        .or_else(|e| fail(format!("cannot reach {address}: {e}")))?;
    let status = response.status().as_u16();
    if status != 200 {
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let limited = header("x-ratelimit-remaining").as_deref() == Some("0")
            || (status == 429 && header("retry-after").is_some());
        if (status == 403 || status == 429) && limited {
            let wait = header("retry-after")
                .and_then(|s| s.trim().parse::<u64>().ok())
                .or_else(|| {
                    let reset = header("x-ratelimit-reset")?.trim().parse::<u64>().ok()?;
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
                    Some(reset.saturating_sub(now))
                });
            let when = match wait {
                Some(secs) => format!("it resets in {}", minutes(secs)),
                None => "it resets within the hour".to_string(),
            };
            return fail(format!(
                "GitHub's rate limit for unauthenticated requests is used up ({address} \
                 answered {status}); {when}"
            ));
        }
        return fail(format!("{address} answered HTTP {status}"));
    }
    response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .or_else(|e| fail(format!("cannot read the answer from {address}: {e}")))
}

fn minutes(secs: u64) -> String {
    let m = secs.div_ceil(60).max(1);
    if m == 1 {
        "1 minute".to_string()
    } else {
        format!("{m} minutes")
    }
}

fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*').trim_end();
        (file == name && sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch, Error> {
        use std::hash::BuildHasher;
        let random = std::collections::hash_map::RandomState::new();
        let mut attempt = 0u32;
        loop {
            let token = random.hash_one((std::process::id(), attempt));
            let dir = std::env::temp_dir().join(format!("gltree-update-{token:016x}"));
            match std::fs::create_dir(&dir) {
                Ok(()) => return Ok(Scratch(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && attempt < 16 => {
                    attempt += 1;
                }
                Err(e) => return fail(format!("cannot make the folder {}: {e}", dir.display())),
            }
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unpack(
    dir: &Path,
    name: &str,
    archive: &[u8],
    version: Version,
    target: &str,
) -> Result<PathBuf, Error> {
    let file = dir.join(name);
    std::fs::write(&file, archive)
        .or_else(|e| fail(format!("cannot write {}: {e}", file.display())))?;
    let out = dir.join("unpacked");
    std::fs::create_dir(&out)
        .or_else(|e| fail(format!("cannot make the folder {}: {e}", out.display())))?;
    let ran = std::process::Command::new("tar")
        .arg("-xf")
        .arg(&file)
        .arg("-C")
        .arg(&out)
        .output()
        .or_else(|e| fail(format!("cannot run tar to unpack {name}: {e}")))?;
    if !ran.status.success() {
        return fail(format!(
            "tar could not unpack {name}: {}",
            String::from_utf8_lossy(&ran.stderr).trim()
        ));
    }
    let binary = out
        .join(format!("gltree-v{version}-{target}"))
        .join(format!("gltree{}", std::env::consts::EXE_SUFFIX));
    match std::fs::symlink_metadata(&binary) {
        Ok(m) if m.is_file() => Ok(binary),
        _ => fail(format!(
            "{name} holds no file gltree-v{version}-{target}/gltree{}",
            std::env::consts::EXE_SUFFIX
        )),
    }
}

fn beside(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

fn clear_leftovers(exe: &Path) {
    let _ = std::fs::remove_file(beside(exe, ".old"));
    let (Some(folder), Some(name)) = (exe.parent(), exe.file_name()) else {
        return;
    };
    let mut prefix = name.to_os_string();
    prefix.push(".new-");
    let prefix = prefix.to_string_lossy().into_owned();
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn replace(exe: &Path, binary: &Path) -> Result<(), Error> {
    let staged = beside(exe, &format!(".new-{}", std::process::id()));
    let unchanged = |what: String| Error(format!("{what}; {} was left as it was", shown(exe)));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .and_then(|mut to| std::io::copy(&mut std::fs::File::open(binary)?, &mut to))
        .and_then(|_| std::fs::metadata(exe))
        .and_then(|m| std::fs::set_permissions(&staged, m.permissions()));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&staged);
        return Err(unchanged(format!("cannot write {}: {e}", shown(&staged))));
    }
    swap(exe, &staged).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })
}

#[cfg(not(windows))]
fn swap(exe: &Path, staged: &Path) -> Result<(), Error> {
    std::fs::rename(staged, exe).or_else(|e| {
        fail(format!(
            "cannot move {} over {}: {e}; {} was left as it was",
            shown(staged),
            shown(exe),
            shown(exe)
        ))
    })
}

// Windows refuses to overwrite or delete a running executable but lets it be renamed.
#[cfg(windows)]
fn swap(exe: &Path, staged: &Path) -> Result<(), Error> {
    let old = beside(exe, ".old");
    if let Err(e) = std::fs::rename(exe, &old) {
        return fail(format!(
            "cannot move {} aside: {e}; it was left as it was",
            shown(exe)
        ));
    }
    if let Err(e) = std::fs::rename(staged, exe) {
        return match std::fs::rename(&old, exe) {
            Ok(()) => fail(format!(
                "cannot move {} into place: {e}; {} was put back",
                shown(staged),
                shown(exe)
            )),
            Err(back) => fail(format!(
                "cannot move {} into place: {e}, nor put the old binary back: {back}; \
                 it is at {}",
                shown(staged),
                shown(&old)
            )),
        };
    }
    let _ = std::fs::remove_file(&old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_scratch_folder_is_new_and_goes_when_dropped() {
        let first = Scratch::new().unwrap();
        let second = Scratch::new().unwrap();
        assert_ne!(first.0, second.0);
        assert!(first.0.is_dir() && second.0.is_dir());
        let kept = first.0.clone();
        drop(first);
        assert!(!kept.exists());
        assert!(second.0.is_dir());
    }
}
