//! Every way a tracker breaks the tree convention.

use crate::item::{Item, State};
use crate::scheme::{OriginLine, ParentLine, RelationScheme};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The version of the JSON shape `to_json` prints.
pub const JSON_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: &'static str,
    pub iid: u64,
    pub detail: String,
}

/// What a state policy sees of an item's place in the tree.
pub struct Family<'a> {
    pub is_root: bool,
    pub children: &'a [&'a Item],
    pub descendants: &'a [&'a Item],
}

/// The rule for when an item may be closed or open against the items below it.
pub trait StatePolicy {
    fn judge(&self, item: &Item, family: &Family) -> Option<(&'static str, &'static str)>;
}

/// A root closes when every descendant is closed; anything below a root closes on its own work.
pub struct RootsOnly;

/// Every parent stays open while any direct child is open, and closes when all of them are.
pub struct EveryParent;

impl StatePolicy for EveryParent {
    fn judge(&self, item: &Item, family: &Family) -> Option<(&'static str, &'static str)> {
        let children = family.children;
        if children.is_empty() {
            return None;
        }
        let any_open = children.iter().any(|c| c.state == State::Open);
        match item.state {
            State::Closed if any_open => {
                Some(("closed-parent-open-child", "closed, with an open child"))
            }
            State::Open if !any_open => {
                Some(("open-parent-all-closed", "open, every child closed"))
            }
            _ => None,
        }
    }
}

impl StatePolicy for RootsOnly {
    fn judge(&self, item: &Item, family: &Family) -> Option<(&'static str, &'static str)> {
        let (is_root, descendants) = (family.is_root, family.descendants);
        if !is_root || descendants.is_empty() {
            return None;
        }
        let open = |d: &&&Item| d.state == State::Open;
        match item.state {
            State::Closed if descendants.iter().any(|d| open(&d)) => Some((
                "closed-parent-open-child",
                "closed, with an open descendant",
            )),
            State::Open if !descendants.iter().any(|d| open(&d)) => {
                Some(("open-parent-all-closed", "open, every descendant closed"))
            }
            _ => None,
        }
    }
}

/// What a repo's `relates_to` links may join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Links {
    /// Only a parent and its child; any other link is a `stray-link` (ADR-0001).
    TreeOnly,
    /// Also related issues; no link is stray.
    Shared,
}

pub fn check(
    items: &[Item],
    scheme: &dyn RelationScheme,
    policy: &dyn StatePolicy,
    links: Links,
    origin: Option<&OriginLine>,
) -> Vec<Finding> {
    let by: HashMap<u64, &Item> = items.iter().map(|it| (it.iid, it)).collect();
    let mut sorted: Vec<&Item> = items.iter().collect();
    sorted.sort_by_key(|it| it.iid);
    let mut kids: BTreeMap<u64, Vec<&Item>> = BTreeMap::new();
    for it in &sorted {
        if let Some(p) = scheme.parent_of(it) {
            kids.entry(p).or_default().push(it);
        }
    }
    let mut out = Vec::new();
    for it in sorted {
        let mut add = |kind: &'static str, detail: String| {
            out.push(Finding {
                kind,
                iid: it.iid,
                detail,
            })
        };
        let first = ParentLine::parse(&it.desc);
        let parent_lines = it
            .desc
            .split('\n')
            .filter(|l| ParentLine::parse(l).is_some())
            .count();
        if parent_lines > 1 || (parent_lines == 1 && first.is_none()) {
            add(
                "parent-line",
                "one Parent line, and only as the first line".into(),
            );
        }
        let o = origin.and_then(|r| r.origin_of(&it.desc));
        if let Some(r) = origin {
            let origin_lines = it
                .desc
                .split('\n')
                .filter(|l| r.parse_line(l).is_some())
                .count();
            if origin_lines > 1 || (origin_lines == 1 && o.is_none()) {
                add(
                    "origin-line",
                    format!(
                        "one {} line, and only under the Parent line or first",
                        r.word
                    ),
                );
            }
        }
        if it.kind == "task" {
            add("task", "still a Task".into());
        }
        let p = scheme.parent_of(it);
        if let Some(p) = p.filter(|p| !by.contains_key(p)) {
            add("missing-parent", format!("#{p} does not exist"));
        }
        if let Some(o) = o.filter(|o| !by.contains_key(o)) {
            add("missing-origin", format!("#{o} does not exist"));
        }
        if let (Some(line), Some(links)) = (first, &it.links)
            && !links.contains(&line)
        {
            add(
                "unlinked",
                format!("no relates_to link to #{line}, so tree <root> misses it"),
            );
        }
        if let (Some(o), Some(links)) = (o, &it.links)
            && !links.contains(&o)
        {
            add(
                "unlinked",
                format!("no relates_to link to #{o}, its origin"),
            );
        }
        let strays = if links == Links::TreeOnly {
            it.link_ends()
        } else {
            &[]
        };
        for &l in strays.iter().filter(|l| **l > it.iid) {
            let names_it = |other: &Item| {
                scheme.parent_of(other) == Some(it.iid)
                    || origin.and_then(|r| r.origin_of(&other.desc)) == Some(it.iid)
            };
            let joined = p == Some(l) || o == Some(l) || by.get(&l).is_some_and(|x| names_it(x));
            if !joined {
                add(
                    "stray-link",
                    format!("the link to #{l} joins no parent and child, nor an origin"),
                );
            }
        }
        let (mut seen, mut cur) = (HashSet::from([it.iid]), p);
        while let Some(c) = cur.filter(|c| by.contains_key(c)) {
            if !seen.insert(c) {
                add("cycle", "following parents leads back here".into());
                break;
            }
            cur = scheme.parent_of(by[&c]);
        }
        let ds = descendants(it.iid, &kids);
        let cs = kids.get(&it.iid).cloned().unwrap_or_default();
        let family = Family {
            is_root: p.is_none(),
            children: &cs,
            descendants: &ds,
        };
        if let Some((kind, detail)) = policy.judge(it, &family) {
            add(kind, detail.into());
        }
    }
    out
}

fn descendants<'a>(n: u64, kids: &BTreeMap<u64, Vec<&'a Item>>) -> Vec<&'a Item> {
    let (mut out, mut seen) = (Vec::new(), HashSet::from([n]));
    let mut stack: Vec<&Item> = kids.get(&n).cloned().unwrap_or_default();
    while let Some(c) = stack.pop() {
        if seen.insert(c.iid) {
            out.push(c);
            stack.extend(kids.get(&c.iid).into_iter().flatten());
        }
    }
    out
}

/// `kind` padded to 26, `#N`, two spaces, the detail; then the count.
pub fn render(findings: &[Finding]) -> Vec<String> {
    let mut lines: Vec<String> = findings
        .iter()
        .map(|f| format!("{:<26} #{}  {}", f.kind, f.iid, f.detail))
        .collect();
    lines.push(format!("{} anomalies", findings.len()));
    lines
}

pub fn to_json(findings: &[Finding]) -> Value {
    let list: Vec<Value> = findings
        .iter()
        .map(|f| json!({ "kind": f.kind, "item": f.iid, "detail": f.detail }))
        .collect();
    json!({ "version": JSON_VERSION, "findings": list })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(iid: u64, state: State, desc: &str, links: &[u64]) -> Item {
        let mut it = Item::new(iid, state, &format!("t{iid}"), desc);
        it.links = Some(links.to_vec());
        it
    }

    fn kinds(items: &[Item]) -> Vec<(&'static str, u64)> {
        check(items, &ParentLine, &RootsOnly, Links::TreeOnly, None)
            .iter()
            .map(|f| (f.kind, f.iid))
            .collect()
    }

    fn every(items: &[Item]) -> Vec<(&'static str, u64)> {
        check(items, &ParentLine, &EveryParent, Links::TreeOnly, None)
            .iter()
            .map(|f| (f.kind, f.iid))
            .collect()
    }

    #[test]
    fn every_parent_holds_a_closed_slice_to_its_open_child_at_any_depth() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Closed, "Parent: #1", &[1, 3]),
            item(3, Open, "Parent: #2", &[2]),
        ];
        // #1 is open over its only child #2, which is closed: that is reported too
        assert_eq!(
            every(&items),
            [
                ("open-parent-all-closed", 1),
                ("closed-parent-open-child", 2)
            ]
        );
    }

    #[test]
    fn every_parent_reports_an_open_parent_whose_children_are_all_closed() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Open, "Parent: #1", &[1, 3]),
            item(3, Closed, "Parent: #2", &[2]),
        ];
        assert_eq!(every(&items), [("open-parent-all-closed", 2)]);
    }

    #[test]
    fn every_parent_judges_direct_children_only() {
        // #1 is closed, its child #2 closed, its grandchild #3 open: only #2 is reported
        let items = [
            item(1, Closed, "", &[2]),
            item(2, Closed, "Parent: #1", &[1, 3]),
            item(3, Open, "Parent: #2", &[2]),
        ];
        assert_eq!(every(&items), [("closed-parent-open-child", 2)]);
    }

    #[test]
    fn every_parent_leaves_a_consistent_tree_and_a_childless_item_alone() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Open, "Parent: #1", &[1]),
            item(4, Closed, "", &[]),
        ];
        assert!(every(&items).is_empty());
    }

    use State::{Closed, Open};

    fn with_origin(items: &[Item], links: Links) -> Vec<(&'static str, u64)> {
        check(
            items,
            &ParentLine,
            &EveryParent,
            links,
            Some(&OriginLine::new("From")),
        )
        .iter()
        .map(|f| (f.kind, f.iid))
        .collect()
    }

    #[test]
    fn a_linked_find_does_not_hold_its_closed_origin_open() {
        let items = [
            item(1, Closed, "", &[2]),
            item(2, Open, "From: #1 (found while measuring)\n\nx", &[1]),
            item(3, Open, "", &[4]),
            item(4, Open, "Parent: #3\nFrom: #1\n\ny", &[3, 1]),
        ];
        // #1 also links #4 as its origin
        let mut items = items.to_vec();
        items[0].links = Some(vec![2, 4]);
        assert!(with_origin(&items, Links::TreeOnly).is_empty());
    }

    #[test]
    fn an_origin_link_is_not_stray_but_any_other_still_is() {
        let items = [
            item(1, Open, "", &[2, 3]),
            item(2, Open, "From: #1", &[1, 3]),
            item(3, Open, "", &[1, 2]),
        ];
        assert_eq!(
            with_origin(&items, Links::TreeOnly),
            [("stray-link", 1), ("stray-link", 2)]
        );
    }

    #[test]
    fn an_origin_line_out_of_place_or_doubled() {
        let items = [
            item(1, Open, "", &[]),
            item(2, Open, "x\nFrom: #1", &[]),
            item(3, Open, "From: #1\nFrom: #1", &[]),
            item(4, Open, "Parent: #1\n\nFrom: #1", &[]),
        ];
        let found = with_origin(&items, Links::Shared);
        assert!(found.contains(&("origin-line", 2)), "{found:?}");
        assert!(found.contains(&("origin-line", 3)), "{found:?}");
        assert!(found.contains(&("origin-line", 4)), "{found:?}");
    }

    #[test]
    fn an_origin_that_does_not_exist_or_is_not_linked() {
        assert_eq!(
            with_origin(&[item(2, Open, "From: #99", &[99])], Links::TreeOnly),
            [("missing-origin", 2)]
        );
        assert_eq!(
            with_origin(
                &[item(1, Open, "", &[]), item(2, Open, "From: #1", &[])],
                Links::TreeOnly
            ),
            [("unlinked", 2)]
        );
    }

    #[test]
    fn without_an_origin_word_an_origin_line_is_just_text() {
        let items = [item(1, Open, "", &[]), item(2, Open, "x\nFrom: #1", &[])];
        assert!(kinds(&items).is_empty());
    }

    #[test]
    fn a_linked_tree_with_an_open_child_under_an_open_root_is_clean() {
        assert!(kinds(&[item(1, Open, "", &[2]), item(2, Open, "Parent: #1", &[1])]).is_empty());
    }

    #[test]
    fn a_parent_line_that_is_not_first_or_is_doubled() {
        assert_eq!(
            kinds(&[item(1, Open, "", &[]), item(2, Open, "x\nParent: #1", &[])]),
            [("parent-line", 2)]
        );
        assert_eq!(
            kinds(&[
                item(1, Open, "", &[2]),
                item(2, Open, "Parent: #1\r\nParent: #1", &[1])
            ]),
            [("parent-line", 2)]
        );
    }

    #[test]
    fn a_leftover_task() {
        let mut t = item(2, Open, "", &[]);
        t.kind = "task".into();
        assert_eq!(kinds(&[t]), [("task", 2)]);
    }

    #[test]
    fn a_parent_that_does_not_exist() {
        assert_eq!(
            kinds(&[item(2, Open, "Parent: #99", &[99])]),
            [("missing-parent", 2)]
        );
    }

    #[test]
    fn a_parent_line_without_its_link_but_only_where_links_were_recorded() {
        assert_eq!(
            kinds(&[item(1, Open, "", &[]), item(2, Open, "Parent: #1", &[])]),
            [("unlinked", 2)]
        );
        let mut a = Item::new(1, Open, "t1", "");
        let mut b = Item::new(2, Open, "t2", "Parent: #1");
        a.links = None;
        b.links = None;
        assert!(kinds(&[a, b]).is_empty());
    }

    #[test]
    fn a_link_that_joins_no_parent_and_child_is_reported_once_at_the_lower_number() {
        let items = [
            item(1, Open, "", &[2, 3]),
            item(2, Open, "Parent: #1", &[1, 3]),
            item(3, Open, "Parent: #1", &[1, 2]),
        ];
        assert_eq!(kinds(&items), [("stray-link", 2)]);
    }

    #[test]
    fn a_repo_that_shares_links_with_related_issues_has_no_stray_link() {
        let items = [
            item(1, Open, "", &[2, 3]),
            item(2, Open, "Parent: #1", &[1, 3]),
            item(3, Open, "Parent: #1", &[1, 2]),
        ];
        assert!(check(&items, &ParentLine, &RootsOnly, Links::Shared, None).is_empty());
    }

    #[test]
    fn a_cycle_is_reported_on_every_item_in_it() {
        let items = [
            item(1, Open, "Parent: #2", &[2]),
            item(2, Open, "Parent: #1", &[1]),
        ];
        assert_eq!(kinds(&items), [("cycle", 1), ("cycle", 2)]);
    }

    #[test]
    fn a_closed_root_with_an_open_grandchild_is_reported_at_the_root() {
        let items = [
            item(1, Closed, "", &[2]),
            item(2, Closed, "Parent: #1", &[1, 3]),
            item(3, Open, "Parent: #2", &[2]),
        ];
        assert_eq!(kinds(&items), [("closed-parent-open-child", 1)]);
    }

    #[test]
    fn an_open_root_whose_every_descendant_is_closed_is_reported() {
        let items = [item(1, Open, "", &[2]), item(2, Closed, "Parent: #1", &[1])];
        assert_eq!(kinds(&items), [("open-parent-all-closed", 1)]);
    }

    #[test]
    fn a_closed_child_with_an_open_follow_up_is_normal() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Closed, "Parent: #1", &[1, 3]),
            item(3, Open, "Parent: #2", &[2]),
        ];
        assert!(kinds(&items).is_empty());
    }

    #[test]
    fn an_item_under_a_missing_parent_is_not_held_to_the_root_rule() {
        let items = [
            item(2, Closed, "Parent: #99", &[99, 3]),
            item(3, Open, "Parent: #2", &[2]),
        ];
        assert_eq!(kinds(&items), [("missing-parent", 2)]);
    }

    #[test]
    fn findings_come_by_number_and_in_the_python_tools_kind_order() {
        let mut t = item(5, Open, "x\nParent: #1", &[]);
        t.kind = "task".into();
        let items = [item(1, Open, "", &[]), t];
        assert_eq!(kinds(&items), [("parent-line", 5), ("task", 5)]);
    }

    #[test]
    fn the_text_lines_match_the_python_tool() {
        let f = [Finding {
            kind: "missing-parent",
            iid: 2,
            detail: "#99 does not exist".into(),
        }];
        assert_eq!(
            render(&f),
            [
                "missing-parent             #2  #99 does not exist",
                "1 anomalies"
            ]
        );
        assert_eq!(render(&[]), ["0 anomalies"]);
    }

    #[test]
    fn json_lists_kind_item_and_detail_under_a_version() {
        let f = [Finding {
            kind: "cycle",
            iid: 3,
            detail: "d".into(),
        }];
        assert_eq!(
            to_json(&f),
            json!({ "version": 1, "findings": [{ "kind": "cycle", "item": 3, "detail": "d" }] })
        );
    }
}
