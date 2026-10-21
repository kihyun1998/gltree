//! Every way a tracker breaks the tree convention.

use crate::item::{Item, State};
use crate::scheme::{ParentLine, RelationScheme};
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

/// The rule for when an item may be closed or open against its descendants.
pub trait StatePolicy {
    /// A finding for `item`, given whether it is a root and its descendants.
    fn judge(
        &self,
        item: &Item,
        is_root: bool,
        descendants: &[&Item],
    ) -> Option<(&'static str, &'static str)>;
}

/// A root closes when every descendant is closed; anything below a root closes on its own work.
pub struct RootsOnly;

impl StatePolicy for RootsOnly {
    fn judge(
        &self,
        item: &Item,
        is_root: bool,
        descendants: &[&Item],
    ) -> Option<(&'static str, &'static str)> {
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
        if it.kind == "task" {
            add("task", "still a Task".into());
        }
        let p = scheme.parent_of(it);
        if let Some(p) = p.filter(|p| !by.contains_key(p)) {
            add("missing-parent", format!("#{p} does not exist"));
        }
        if let (Some(line), Some(links)) = (first, &it.links)
            && !links.contains(&line)
        {
            add(
                "unlinked",
                format!("no relates_to link to #{line}, so tree <root> misses it"),
            );
        }
        let strays = if links == Links::TreeOnly {
            it.link_ends()
        } else {
            &[]
        };
        for &l in strays.iter().filter(|l| **l > it.iid) {
            let joined = p == Some(l)
                || by
                    .get(&l)
                    .is_some_and(|o| scheme.parent_of(o) == Some(it.iid));
            if !joined {
                add(
                    "stray-link",
                    format!("the link to #{l} joins no parent and child"),
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
        if let Some((kind, detail)) = policy.judge(it, p.is_none(), &ds) {
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
        check(items, &ParentLine, &RootsOnly, Links::TreeOnly)
            .iter()
            .map(|f| (f.kind, f.iid))
            .collect()
    }

    use State::{Closed, Open};

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
        assert!(check(&items, &ParentLine, &RootsOnly, Links::Shared).is_empty());
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
