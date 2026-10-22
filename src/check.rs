//! Every way a tracker breaks the tree convention.

use crate::header;
use crate::item::{Item, State};
use crate::scheme::RelationScheme;
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

pub fn check(
    items: &[Item],
    scheme: &dyn RelationScheme,
    policy: &dyn StatePolicy,
    extra_keys: &[String],
) -> Vec<Finding> {
    check_with(items, scheme, policy, extra_keys, &[])
}

/// `check`, with the repo's own first-line words added to the ones `old-header` knows.
pub fn check_with(
    items: &[Item],
    scheme: &dyn RelationScheme,
    policy: &dyn StatePolicy,
    extra_keys: &[String],
    old_head_keys: &[String],
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
    let refs: HashMap<u64, Vec<(&'static str, u64)>> = items
        .iter()
        .map(|it| (it.iid, header::refs_of(&it.desc)))
        .collect();
    let names = |a: u64, b: u64| refs.get(&a).is_some_and(|r| r.iter().any(|(_, n)| *n == b));
    let mut out = Vec::new();
    for it in sorted {
        let mut add = |kind: &'static str, detail: String| {
            out.push(Finding {
                kind,
                iid: it.iid,
                detail,
            })
        };
        let body = match header::parse(&it.desc, extra_keys) {
            Err(e) => {
                add("header", e);
                header::loose(&it.desc).body
            }
            Ok(split) => {
                if let Some(key) = split.header.as_ref().and_then(|h| h.naming(it.iid)) {
                    add("header", format!("`{key}` names the item itself"));
                }
                split.body
            }
        };
        if let Some(line) = header::old_head(body, old_head_keys) {
            add(
                "old-header",
                format!("the body still opens with `{line}`; move it into the header"),
            );
        }
        if it.kind == "task" {
            add("task", "still a Task".into());
        }
        let p = scheme.parent_of(it);
        if let Some(p) = p.filter(|p| !by.contains_key(p)) {
            add("missing-parent", format!("#{p} does not exist"));
        }
        let mine = &refs[&it.iid];
        for (key, n) in mine
            .iter()
            .filter(|(k, n)| *k != "parent" && !by.contains_key(n))
        {
            add(
                "missing-reference",
                format!("`{key}` names #{n}, which does not exist"),
            );
        }
        if let (Some(p), Some(links)) = (p, &it.links)
            && !links.contains(&p)
        {
            add(
                "unlinked",
                format!("no relates_to link to #{p}, so tree <root> misses it"),
            );
        }
        if let Some(links) = &it.links {
            for (key, n) in mine
                .iter()
                .filter(|(k, n)| *k != "parent" && !links.contains(n))
            {
                add(
                    "unlinked",
                    format!("no relates_to link to #{n}, named by `{key}`"),
                );
            }
            for &l in links.iter().filter(|l| **l > it.iid) {
                if !names(it.iid, l) && !names(l, it.iid) {
                    add(
                        "stray-link",
                        format!("the link to #{l} is explained by no reference on either end"),
                    );
                }
            }
        }
        let blockers = |n: u64| {
            refs.get(&n)
                .into_iter()
                .flatten()
                .filter(|(k, _)| *k == "blocked_by")
                .map(|(_, b)| *b)
        };
        let (mut seen, mut stack): (HashSet<u64>, Vec<u64>) =
            (HashSet::new(), blockers(it.iid).collect());
        while let Some(b) = stack.pop() {
            if b == it.iid {
                add(
                    "blocker-cycle",
                    "following blocked_by leads back here".into(),
                );
                break;
            }
            if seen.insert(b) {
                stack.extend(blockers(b));
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
    use crate::scheme::FrontMatter;
    use State::{Closed, Open};

    /// A body whose header names `n` as the parent.
    fn p(n: u64) -> String {
        format!("+++\nparent = {n}\n+++\n")
    }

    fn item(iid: u64, state: State, desc: &str, links: &[u64]) -> Item {
        let mut it = Item::new(iid, state, &format!("t{iid}"), desc);
        it.links = Some(links.to_vec());
        it
    }

    fn kinds(items: &[Item]) -> Vec<(&'static str, u64)> {
        check(items, &FrontMatter, &RootsOnly, &[])
            .iter()
            .map(|f| (f.kind, f.iid))
            .collect()
    }

    fn every(items: &[Item]) -> Vec<(&'static str, u64)> {
        check(items, &FrontMatter, &EveryParent, &[])
            .iter()
            .map(|f| (f.kind, f.iid))
            .collect()
    }

    #[test]
    fn every_parent_holds_a_closed_slice_to_its_open_child_at_any_depth() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Closed, &p(1), &[1, 3]),
            item(3, Open, &p(2), &[2]),
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
            item(2, Open, &p(1), &[1, 3]),
            item(3, Closed, &p(2), &[2]),
        ];
        assert_eq!(every(&items), [("open-parent-all-closed", 2)]);
    }

    #[test]
    fn every_parent_judges_direct_children_only() {
        // #1 is closed, its child #2 closed, its grandchild #3 open: only #2 is reported
        let items = [
            item(1, Closed, "", &[2]),
            item(2, Closed, &p(1), &[1, 3]),
            item(3, Open, &p(2), &[2]),
        ];
        assert_eq!(every(&items), [("closed-parent-open-child", 2)]);
    }

    #[test]
    fn every_parent_leaves_a_consistent_tree_and_a_childless_item_alone() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Open, &p(1), &[1]),
            item(4, Closed, "", &[]),
        ];
        assert!(every(&items).is_empty());
    }

    #[test]
    fn a_linked_tree_with_an_open_child_under_an_open_root_is_clean() {
        assert!(kinds(&[item(1, Open, "", &[2]), item(2, Open, &p(1), &[1])]).is_empty());
    }

    #[test]
    fn a_header_out_of_form_is_reported_with_its_reason() {
        let found = check(
            &[
                item(1, Open, "", &[]),
                item(2, Open, "+++\nparnet = 1\n+++\n", &[]),
                item(3, Open, "+++\nparent = #1\n+++\n", &[]),
                item(4, Open, "+++\n[extra]\nmarker = \"x\"\n+++\n", &[]),
            ],
            &FrontMatter,
            &RootsOnly,
            &[],
        );
        let got: Vec<(&str, u64)> = found.iter().map(|f| (f.kind, f.iid)).collect();
        assert_eq!(got, [("header", 2), ("header", 3), ("header", 4)]);
        assert!(found[0].detail.contains("parnet"), "{}", found[0].detail);
        let declared = ["marker".to_string()];
        let found = check(
            &[item(4, Open, "+++\n[extra]\nmarker = \"x\"\n+++\n", &[])],
            &FrontMatter,
            &RootsOnly,
            &declared,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_header_naming_the_item_itself_is_reported() {
        for (desc, key) in [
            ("+++\nfrom = 2\n+++\n", "`from`"),
            ("+++\nrelated = [1, 2]\n+++\n", "`related`"),
            ("+++\nblocked_by = [2]\n+++\n", "`blocked_by`"),
        ] {
            let found = check(
                &[item(1, Open, "", &[]), item(2, Open, desc, &[])],
                &FrontMatter,
                &RootsOnly,
                &[],
            );
            assert!(
                found
                    .iter()
                    .any(|f| f.kind == "header" && f.iid == 2 && f.detail.contains(key)),
                "{desc:?}: {found:?}"
            );
        }
    }

    /// A small tracker where every link is explained by one reference: #2 under #1, found while
    /// working #3, related to #4 (written on #4's end), blocked by #5.
    fn explained() -> Vec<Item> {
        vec![
            item(1, Open, "", &[2]),
            item(
                2,
                Open,
                "+++\nparent = 1\nfrom = 3\nblocked_by = [5]\n+++\n",
                &[1, 3, 4, 5],
            ),
            item(3, Open, "", &[2]),
            item(4, Open, "+++\nrelated = [2]\n+++\n", &[2]),
            item(5, Open, "", &[2]),
        ]
    }

    #[test]
    fn a_tracker_whose_every_link_is_explained_is_clean() {
        assert!(kinds(&explained()).is_empty(), "{:?}", kinds(&explained()));
    }

    #[test]
    fn a_link_no_reference_explains_is_stray_once_at_the_lower_number() {
        for (at, desc) in [
            (1, "+++\nparent = 1\nblocked_by = [5]\n+++\n"),
            (3, "+++\nparent = 1\nfrom = 3\n+++\n"),
        ] {
            let mut items = explained();
            items[1].desc = desc.into();
            let got = kinds(&items);
            assert_eq!(got, [("stray-link", 2)], "{at}: {got:?}");
        }
        let mut items = explained();
        items[3].desc = String::new();
        assert_eq!(kinds(&items), [("stray-link", 2)]);
    }

    #[test]
    fn a_reference_without_its_link_names_its_key() {
        let mut items = explained();
        items[1].links = Some(vec![1, 4]);
        items[2].links = Some(vec![]);
        items[4].links = Some(vec![]);
        let found = check(&items, &FrontMatter, &RootsOnly, &[]);
        let got: Vec<String> = found
            .iter()
            .map(|f| format!("{} #{} {}", f.kind, f.iid, f.detail))
            .collect();
        assert_eq!(
            got,
            [
                "unlinked #2 no relates_to link to #3, named by `from`",
                "unlinked #2 no relates_to link to #5, named by `blocked_by`"
            ]
        );
    }

    #[test]
    fn a_reference_to_an_item_that_does_not_exist() {
        let items = [item(
            2,
            Open,
            "+++\nfrom = 98\nrelated = [99]\n+++\n",
            &[98, 99],
        )];
        let found = check(&items, &FrontMatter, &RootsOnly, &[]);
        let got: Vec<(&str, &str)> = found.iter().map(|f| (f.kind, f.detail.as_str())).collect();
        assert_eq!(
            got,
            [
                (
                    "missing-reference",
                    "`from` names #98, which does not exist"
                ),
                (
                    "missing-reference",
                    "`related` names #99, which does not exist"
                )
            ]
        );
    }

    #[test]
    fn a_blocker_cycle_is_reported_on_every_item_in_it_and_a_chain_is_not() {
        let items = [
            item(1, Open, "+++\nblocked_by = [2]\n+++\n", &[2, 3]),
            item(2, Open, "+++\nblocked_by = [3]\n+++\n", &[1, 3]),
            item(3, Open, "+++\nblocked_by = [1]\n+++\n", &[1, 2]),
        ];
        assert_eq!(
            kinds(&items),
            [
                ("blocker-cycle", 1),
                ("blocker-cycle", 2),
                ("blocker-cycle", 3)
            ]
        );
        let chain = [
            item(1, Open, "+++\nblocked_by = [2]\n+++\n", &[2]),
            item(2, Open, "+++\nblocked_by = [3]\n+++\n", &[1, 3]),
            item(3, Open, "", &[2]),
        ];
        assert!(kinds(&chain).is_empty());
    }

    #[test]
    fn a_body_still_opening_with_a_first_line_head_is_reported() {
        let items = [
            item(1, Open, "", &[2]),
            item(
                2,
                Open,
                "+++\nparent = 1\n+++\n\nBlocked by: #1\nbody",
                &[1],
            ),
            item(3, Open, "Parent: #1\n\nbody", &[]),
            item(4, Open, "Parent: #1 is quoted here", &[]),
        ];
        assert_eq!(kinds(&items), [("old-header", 2), ("old-header", 3)]);
        let own = [item(5, Open, "Origin: #1\n\nbody", &[])];
        assert!(kinds(&own).is_empty());
        let found = check_with(&own, &FrontMatter, &RootsOnly, &[], &["Origin".into()]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].kind, "old-header");
    }

    #[test]
    fn an_old_first_line_makes_no_parent() {
        // reported as an unmigrated head, never as a child of #1 (no unlinked, no policy finding)
        assert_eq!(
            kinds(&[item(1, Open, "", &[]), item(2, Open, "Parent: #1", &[])]),
            [("old-header", 2)]
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
            kinds(&[item(2, Open, &p(99), &[99])]),
            [("missing-parent", 2)]
        );
    }

    #[test]
    fn a_parent_without_its_link_but_only_where_links_were_recorded() {
        assert_eq!(
            kinds(&[item(1, Open, "", &[]), item(2, Open, &p(1), &[])]),
            [("unlinked", 2)]
        );
        let mut a = Item::new(1, Open, "t1", "");
        let mut b = Item::new(2, Open, "t2", &p(1));
        a.links = None;
        b.links = None;
        assert!(kinds(&[a, b]).is_empty());
    }

    #[test]
    fn a_cycle_is_reported_on_every_item_in_it() {
        let items = [item(1, Open, &p(2), &[2]), item(2, Open, &p(1), &[1])];
        assert_eq!(kinds(&items), [("cycle", 1), ("cycle", 2)]);
    }

    #[test]
    fn a_closed_root_with_an_open_grandchild_is_reported_at_the_root() {
        let items = [
            item(1, Closed, "", &[2]),
            item(2, Closed, &p(1), &[1, 3]),
            item(3, Open, &p(2), &[2]),
        ];
        assert_eq!(kinds(&items), [("closed-parent-open-child", 1)]);
    }

    #[test]
    fn an_open_root_whose_every_descendant_is_closed_is_reported() {
        let items = [item(1, Open, "", &[2]), item(2, Closed, &p(1), &[1])];
        assert_eq!(kinds(&items), [("open-parent-all-closed", 1)]);
    }

    #[test]
    fn under_roots_only_a_closed_child_over_an_open_one_is_normal() {
        let items = [
            item(1, Open, "", &[2]),
            item(2, Closed, &p(1), &[1, 3]),
            item(3, Open, &p(2), &[2]),
        ];
        assert!(kinds(&items).is_empty());
    }

    #[test]
    fn an_item_under_a_missing_parent_is_not_held_to_the_root_rule() {
        let items = [
            item(2, Closed, &p(99), &[99, 3]),
            item(3, Open, &p(2), &[2]),
        ];
        assert_eq!(kinds(&items), [("missing-parent", 2)]);
    }

    #[test]
    fn findings_come_by_number_and_header_first() {
        let mut t = item(5, Open, "+++\nparnet = 1\n+++\n", &[]);
        t.kind = "task".into();
        let items = [item(1, Open, "", &[]), t];
        assert_eq!(kinds(&items), [("header", 5), ("task", 5)]);
    }

    #[test]
    fn the_text_lines_pad_the_kind_and_count_the_findings() {
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
