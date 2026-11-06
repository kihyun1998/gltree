//! A tracker read from a snapshot file: a JSON list of items.

use crate::item::Item;
use crate::tracker::{Created, Error, Linked, Tracker, level_of};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub struct SnapshotFile {
    pub path: PathBuf,
}

impl SnapshotFile {
    /// The snapshot text `parse` reads back: a JSON list of items, by number.
    pub fn to_bytes(items: &[Item]) -> Vec<u8> {
        let mut sorted: Vec<&Item> = items.iter().collect();
        sorted.sort_by_key(|it| it.iid);
        let mut out = serde_json::to_vec_pretty(&sorted).expect("items serialise");
        out.push(b'\n');
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Vec<Item>, Error> {
        serde_json::from_slice(bytes).map_err(|e| Error::Malformed(e.to_string()))
    }

    /// Saves `items` at `path`, replacing what is there only once the new file is complete.
    pub fn save(path: &Path, items: &[Item]) -> io::Result<()> {
        replace_file(path, |f| f.write_all(&Self::to_bytes(items)))
    }
}

/// `write` fills a temporary file beside `path`, which is then renamed over it. On any failure the
/// temporary file is removed and `path` is as it was. A read-only `path` is refused. The new file
/// takes the old file's permissions; with no old file it is its owner's alone on Unix, and takes the
/// folder's access on Windows.
pub fn replace_file(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file name"))?;
    let existing = fs::metadata(path).ok();
    if existing
        .as_ref()
        .is_some_and(|m| m.permissions().readonly())
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the file is read only",
        ));
    }
    let temp = path.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&temp)?;
    let filled = (|| {
        if let Some(m) = &existing {
            file.set_permissions(m.permissions())?;
        }
        write(&mut file)?;
        file.sync_all()
    })();
    drop(file);
    let done = filled.and_then(|()| fs::rename(&temp, path));
    if done.is_err() {
        let _ = fs::remove_file(&temp);
    }
    done
}

impl Tracker for SnapshotFile {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        let bytes = std::fs::read(&self.path)
            .map_err(|e| Error::Source(format!("{}: {e}", self.path.display())))?;
        Self::parse(&bytes)
    }

    fn item_with_comments(&self, iid: u64) -> Result<Item, Error> {
        self.all_items()?
            .into_iter()
            .find(|it| it.iid == iid)
            .ok_or(Error::NotFound(iid))
    }

    fn level(&self, iids: &[u64]) -> Result<HashMap<u64, Linked>, Error> {
        Ok(level_of(&self.all_items()?, iids))
    }

    fn create_item(&self, _: &str, _: &str, _: &[String]) -> Result<Created, Error> {
        Err(Error::Refused("a snapshot is read only".into()))
    }

    fn add_link(&self, _: u64, _: u64) -> Result<(), Error> {
        Err(Error::Refused("a snapshot is read only".into()))
    }

    fn set_desc(&self, _: u64, _: &str) -> Result<(), Error> {
        Err(Error::Refused("a snapshot is read only".into()))
    }

    fn remove_link(&self, _: u64, _: u64) -> Result<(), Error> {
        Err(Error::Refused("a snapshot is read only".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::{SnapshotFile, replace_file};
    use crate::item::{Comment, State};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn empty_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gltree-replace-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        out.sort();
        out
    }

    #[test]
    fn a_write_that_fails_halfway_leaves_the_old_file_and_no_temporary_file() {
        let dir = empty_dir("halfway");
        let target = dir.join("t.json");
        std::fs::write(&target, "the previous bytes").unwrap();
        let r = replace_file(&target, |f| {
            f.write_all(b"half of the new")?;
            Err(std::io::Error::other("disk full"))
        });
        assert_eq!(r.unwrap_err().to_string(), "disk full");
        assert_eq!(std::fs::read(&target).unwrap(), b"the previous bytes");
        assert_eq!(names(&dir), ["t.json"]);
    }

    #[test]
    fn a_complete_write_replaces_the_old_file_and_leaves_nothing_beside_it() {
        let dir = empty_dir("complete");
        let target = dir.join("t.json");
        std::fs::write(&target, "a much longer previous content").unwrap();
        replace_file(&target, |f| f.write_all(b"new")).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert_eq!(names(&dir), ["t.json"]);
    }

    #[test]
    fn the_old_file_is_untouched_while_the_new_one_is_being_written() {
        let dir = empty_dir("during");
        let target = dir.join("t.json");
        std::fs::write(&target, "old").unwrap();
        let during = target.clone();
        replace_file(&target, move |f| {
            f.write_all(b"new")?;
            assert_eq!(std::fs::read(&during).unwrap(), b"old");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_target_that_is_a_directory_fails_and_leaves_no_temporary_file() {
        let dir = empty_dir("directory");
        std::fs::create_dir(dir.join("taken")).unwrap();
        assert!(replace_file(&dir.join("taken"), |f| f.write_all(b"new")).is_err());
        assert_eq!(names(&dir), ["taken"]);
    }

    #[test]
    fn a_read_only_target_is_refused_and_kept() {
        let dir = empty_dir("read-only");
        let target = dir.join("t.json");
        std::fs::write(&target, "old").unwrap();
        let mut p = std::fs::metadata(&target).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&target, p.clone()).unwrap();
        let r = replace_file(&target, |f| f.write_all(b"new"));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
        #[allow(clippy::permissions_set_readonly_false)]
        p.set_readonly(false);
        std::fs::set_permissions(&target, p).unwrap();
        assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(names(&dir), ["t.json"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_new_file_keeps_the_old_file_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = empty_dir("mode");
        let target = dir.join("t.json");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        replace_file(&target, |f| f.write_all(b"new")).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_is_its_owners_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = empty_dir("new-mode");
        let target = dir.join("t.json");
        replace_file(&target, |f| f.write_all(b"new")).unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn comments_survive_the_file_and_an_item_without_them_stays_not_recorded() {
        let mut a = crate::item::Item::new(1, State::Open, "t", "");
        a.comments = Some(vec![Comment {
            author: None,
            created_at: "2026-01-02T03:04:05Z".into(),
            body: "text".into(),
            internal: true,
        }]);
        let mut b = crate::item::Item::new(2, State::Open, "t", "");
        b.comments = Some(vec![]);
        let c = crate::item::Item::new(3, State::Open, "t", "");
        let bytes = SnapshotFile::to_bytes(&[a.clone(), b.clone(), c.clone()]);
        assert_eq!(SnapshotFile::parse(&bytes).unwrap(), vec![a, b, c]);
    }

    #[test]
    fn reads_an_item_and_ignores_fields_gltree_does_not_use() {
        let items = SnapshotFile::parse(
            br#"[{"iid": 3, "gid": "g", "title": "t", "state": "closed", "type": "issue", "hparent": null,
                 "hchildren": [], "desc": "Parent: #1", "labels": ["SPEC"], "links": [1], "created": "2026-01-01"}]"#,
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            (
                items[0].iid,
                items[0].state,
                items[0].links.clone().unwrap()
            ),
            (3, State::Closed, vec![1])
        );
    }

    #[test]
    fn a_snapshot_without_links_or_labels_still_reads() {
        let items =
            SnapshotFile::parse(br#"[{"iid": 3, "title": "t", "state": "open", "desc": ""}]"#)
                .unwrap();
        assert!(items[0].links.is_none() && items[0].labels.is_empty());
    }

    #[test]
    fn what_to_bytes_writes_parse_reads_back() {
        let mut a =
            crate::item::Item::new(2, State::Closed, "설정 「외관」", "Parent: #1\r\n\r\nx");
        a.kind = "task".into();
        a.labels = vec!["SPEC".into()];
        a.links = Some(vec![1]);
        a.closed_at = Some("2026-10-21T03:34:56Z".into());
        a.url = Some("https://gitlab.example.com/team/app/-/issues/2".into());
        let b = crate::item::Item::new(1, State::Open, "t1", "");
        let back = SnapshotFile::parse(&SnapshotFile::to_bytes(&[a.clone(), b.clone()])).unwrap();
        assert_eq!(back, vec![b, a]);
    }

    #[test]
    fn a_file_that_is_not_a_list_of_items_is_refused() {
        assert!(SnapshotFile::parse(br#"{"iid": 3}"#).is_err());
        assert!(SnapshotFile::parse(br#"[{"iid": 3, "title": "t", "state": "half"}]"#).is_err());
    }
}
