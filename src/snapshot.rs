//! A tracker read from a snapshot file: a JSON list of items.

use crate::item::Item;
use crate::tracker::{Error, Tracker};
use std::path::PathBuf;

pub struct SnapshotFile {
    pub path: PathBuf,
}

impl SnapshotFile {
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
            (items[0].iid, items[0].state, items[0].links.clone()),
            (3, State::Closed, vec![1])
        );
    }

    #[test]
    fn a_snapshot_without_links_or_labels_still_reads() {
        let items =
            SnapshotFile::parse(br#"[{"iid": 3, "title": "t", "state": "open", "desc": ""}]"#)
                .unwrap();
        assert!(items[0].links.is_empty() && items[0].labels.is_empty());
    }

    #[test]
    fn a_file_that_is_not_a_list_of_items_is_refused() {
        assert!(SnapshotFile::parse(br#"{"iid": 3}"#).is_err());
        assert!(SnapshotFile::parse(br#"[{"iid": 3, "title": "t", "state": "half"}]"#).is_err());
    }
}
