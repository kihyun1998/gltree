//! The lists gltree keeps on the other end of a relation — `children`, `blocks` and the `related`
//! mirror — worked out from the truth, checked, and written back by `sync` (ADR-0004).

use crate::header::{self, Header};
use crate::item::Item;
use crate::tracker::{Error, Tracker};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

/// The version of the JSON shape `to_json` prints.
pub const JSON_VERSION: u32 = 1;

/// What an item's derived lists should hold: the items whose `parent` names it, whose
/// `blocked_by` names it, and whose `related` names it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expected {
    pub children: Vec<u64>,
    pub blocks: Vec<u64>,
    pub related_in: Vec<u64>,
}

/// The derived lists for `n`, from the items that may name it.
pub fn expected<'a>(n: u64, others: impl IntoIterator<Item = &'a Item>) -> Expected {
    let (mut children, mut blocks, mut related) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    for o in others.into_iter().filter(|o| o.iid != n) {
        for (key, m) in header::refs_of(&o.desc) {
            if m != n {
                continue;
            }
            match key {
                "parent" => children.insert(o.iid),
                "blocked_by" => blocks.insert(o.iid),
                "related" => related.insert(o.iid),
                _ => false,
            };
        }
    }
    Expected {
        children: children.into_iter().collect(),
        blocks: blocks.into_iter().collect(),
        related_in: related.into_iter().collect(),
    }
}

/// One derived list that disagrees with the truth: its key, what it holds, what it should hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub iid: u64,
    pub key: &'static str,
    pub before: Vec<u64>,
    pub after: Vec<u64>,
}

/// `h` with its derived lists set from `e`: `related` becomes the union of both ends.
pub fn rebuilt(h: &Header, e: &Expected) -> Header {
    let related: BTreeSet<u64> = h.related.iter().chain(&e.related_in).copied().collect();
    Header {
        children: e.children.clone(),
        blocks: e.blocks.clone(),
        related: related.into_iter().collect(),
        ..h.clone()
    }
}

/// The changes `rebuilt` makes to `h` for item `n`, in the header's key order.
pub fn changes(n: u64, h: &Header, e: &Expected) -> Vec<Change> {
    let r = rebuilt(h, e);
    [
        ("children", &h.children, &r.children),
        ("related", &h.related, &r.related),
        ("blocks", &h.blocks, &r.blocks),
    ]
    .into_iter()
    .filter(|(_, b, a)| b != a)
    .map(|(key, b, a)| Change {
        iid: n,
        key,
        before: b.clone(),
        after: a.clone(),
    })
    .collect()
}

/// Rewrites `n`'s derived lists from the items linked to it — every reference has a link, so
/// one `level` read is enough. Writes nothing when nothing changes, or when `dry_run`. Refuses a
/// header out of form, since rewriting it would drop what it cannot read.
pub fn sync_one(
    tracker: &dyn Tracker,
    extra_keys: &[String],
    n: u64,
    dry_run: bool,
) -> Result<Vec<Change>, Error> {
    let at = tracker.level(&[n])?.remove(&n).ok_or(Error::NotFound(n))?;
    let e = expected(n, &at.linked);
    write(tracker, extra_keys, &at.item, &e, dry_run)
}

/// Rewrites every item's derived lists from one read of the whole tracker.
pub fn sync_all(
    tracker: &dyn Tracker,
    extra_keys: &[String],
    dry_run: bool,
) -> Result<Vec<Change>, Error> {
    let items = tracker.all_items()?;
    let naming = naming_index(&items);
    let mut sorted: Vec<&Item> = items.iter().collect();
    sorted.sort_by_key(|it| it.iid);
    let mut out = Vec::new();
    for it in sorted {
        let e = expected(it.iid, naming.get(&it.iid).into_iter().flatten().copied());
        out.extend(write(tracker, extra_keys, it, &e, dry_run)?);
    }
    Ok(out)
}

/// For each item number, the items whose header names it under any key.
pub fn naming_index(items: &[Item]) -> HashMap<u64, Vec<&Item>> {
    let mut naming: HashMap<u64, Vec<&Item>> = HashMap::new();
    for it in items {
        let named: BTreeSet<u64> = header::refs_of(&it.desc)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        for n in named {
            naming.entry(n).or_default().push(it);
        }
    }
    naming
}

fn write(
    tracker: &dyn Tracker,
    extra_keys: &[String],
    it: &Item,
    e: &Expected,
    dry_run: bool,
) -> Result<Vec<Change>, Error> {
    let h = header::parse(&it.desc, extra_keys)
        .map_err(|err| {
            Error::Refused(format!(
                "#{}'s header is out of form ({err}); fix it first",
                it.iid
            ))
        })?
        .header
        .unwrap_or_default();
    let found = changes(it.iid, &h, e);
    if !found.is_empty() && !dry_run {
        let body = header::with_header(&it.desc, &rebuilt(&h, e)).map_err(Error::Refused)?;
        tracker.set_desc(it.iid, &body)?;
    }
    Ok(found)
}

/// `#N key [before] -> [after]` per change, then the count and whether it was written.
pub fn render(changes: &[Change], dry_run: bool) -> Vec<String> {
    let list = |ns: &[u64]| {
        let ns: Vec<String> = ns.iter().map(|n| format!("#{n}")).collect();
        format!("[{}]", ns.join(", "))
    };
    let mut lines: Vec<String> = changes
        .iter()
        .map(|c| {
            format!(
                "#{} {} {} -> {}",
                c.iid,
                c.key,
                list(&c.before),
                list(&c.after)
            )
        })
        .collect();
    let items: BTreeSet<u64> = changes.iter().map(|c| c.iid).collect();
    let verb = if dry_run { "would change" } else { "changed" };
    lines.push(format!("{} items {verb}", items.len()));
    lines
}

pub fn to_json(changes: &[Change], dry_run: bool) -> Value {
    let list: Vec<Value> = changes
        .iter()
        .map(|c| json!({ "item": c.iid, "key": c.key, "before": c.before, "after": c.after }))
        .collect();
    json!({ "version": JSON_VERSION, "written": !dry_run && !changes.is_empty(), "changes": list })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::State::Open;
    use crate::tracker::FakeTracker;

    fn item(iid: u64, desc: &str, links: &[u64]) -> Item {
        let mut it = Item::new(iid, Open, &format!("t{iid}"), desc);
        it.links = Some(links.to_vec());
        it
    }

    /// #1 has children #2 and #3; #3 is blocked by #4; #2 lists #4 as related. No derived list yet.
    fn bare() -> Vec<Item> {
        vec![
            item(1, "", &[2, 3]),
            item(
                2,
                "+++\nparent = 1\nrelated = [4]\n+++\n\nbody two",
                &[1, 4],
            ),
            item(3, "+++\nparent = 1\nblocked_by = [4]\n+++\n", &[1, 4]),
            item(4, "Body four.", &[2, 3]),
        ]
    }

    #[test]
    fn expected_lists_who_names_the_item_under_each_key() {
        let items = bare();
        assert_eq!(
            expected(1, &items),
            Expected {
                children: vec![2, 3],
                ..Expected::default()
            }
        );
        assert_eq!(
            expected(4, &items),
            Expected {
                blocks: vec![3],
                related_in: vec![2],
                ..Expected::default()
            }
        );
    }

    #[test]
    fn sync_all_restores_every_list_and_check_then_reports_nothing() {
        let t = FakeTracker::new(bare());
        let got = sync_all(&t, &[], false).unwrap();
        assert_eq!(
            got,
            [
                Change {
                    iid: 1,
                    key: "children",
                    before: vec![],
                    after: vec![2, 3]
                },
                Change {
                    iid: 4,
                    key: "related",
                    before: vec![],
                    after: vec![2]
                },
                Change {
                    iid: 4,
                    key: "blocks",
                    before: vec![],
                    after: vec![3]
                },
            ]
        );
        assert_eq!(t.item(1).unwrap().desc, "+++\nchildren = [2, 3]\n+++\n");
        assert_eq!(
            t.item(4).unwrap().desc,
            "+++\nrelated = [2]\nblocks = [3]\n+++\n\nBody four."
        );
        assert_eq!(
            t.item(2).unwrap().desc,
            bare()[1].desc,
            "the truth is never rewritten"
        );
        let found = crate::check::check(
            &t.all_items().unwrap(),
            &crate::scheme::FrontMatter,
            &crate::check::RootsOnly,
            &[],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_second_sync_writes_nothing() {
        let t = FakeTracker::new(bare());
        sync_all(&t, &[], false).unwrap();
        let writes = t.writes.borrow().len();
        assert!(sync_all(&t, &[], false).unwrap().is_empty());
        assert!(sync_one(&t, &[], 4, false).unwrap().is_empty());
        assert_eq!(t.writes.borrow().len(), writes);
    }

    #[test]
    fn sync_one_reads_one_level_and_writes_only_that_item() {
        let t = FakeTracker::new(bare());
        let got = sync_one(&t, &[], 4, false).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(*t.levels.borrow(), [vec![4]]);
        assert_eq!(*t.writes.borrow(), ["desc #4"]);
    }

    #[test]
    fn a_lost_entry_and_an_extra_one_are_both_repaired() {
        let mut items = bare();
        items[0].desc = "+++\nchildren = [2, 9]\n+++\n".into();
        let t = FakeTracker::new(items);
        let got = sync_one(&t, &[], 1, false).unwrap();
        assert_eq!(got[0].after, [2, 3]);
        assert_eq!(t.item(1).unwrap().desc, "+++\nchildren = [2, 3]\n+++\n");
    }

    #[test]
    fn related_is_the_union_of_both_ends() {
        let mut items = bare();
        items[3].desc = "+++\nrelated = [3]\n+++\n".into();
        items[2].links = Some(vec![1, 4]);
        let t = FakeTracker::new(items);
        sync_one(&t, &[], 4, false).unwrap();
        assert!(t.item(4).unwrap().desc.contains("related = [2, 3]"));
    }

    #[test]
    fn a_dry_run_reports_and_writes_nothing() {
        let t = FakeTracker::new(bare());
        assert_eq!(sync_all(&t, &[], true).unwrap().len(), 3);
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_header_out_of_form_is_refused_before_writing() {
        let mut items = bare();
        items[0].desc = "+++\nparnet = 2\n+++\n".into();
        let t = FakeTracker::new(items);
        let e = sync_one(&t, &[], 1, false).unwrap_err().to_string();
        assert!(e.contains("parnet") && e.contains("fix it first"), "{e}");
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn text_and_json_report_each_change() {
        let c = [Change {
            iid: 1,
            key: "children",
            before: vec![2],
            after: vec![2, 3],
        }];
        assert_eq!(
            render(&c, true),
            ["#1 children [#2] -> [#2, #3]", "1 items would change"]
        );
        assert_eq!(
            to_json(&c, false),
            json!({ "version": 1, "written": true, "changes": [{ "item": 1, "key": "children", "before": [2], "after": [2, 3] }] })
        );
    }
}
