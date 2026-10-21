//! A tracker read from a snapshot file: a JSON list of items.

use crate::item::Item;
use crate::tracker::{Created, Error, Linked, Tracker, level_of};
use std::collections::HashMap;
use std::path::PathBuf;

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
}

impl Tracker for SnapshotFile {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        let bytes = std::fs::read(&self.path)
            .map_err(|e| Error::Source(format!("{}: {e}", self.path.display())))?;
        Self::parse(&bytes)
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
}

#[cfg(test)]
mod tests {
    use super::SnapshotFile;
    use crate::item::State;

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
