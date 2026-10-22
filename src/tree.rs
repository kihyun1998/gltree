//! A tree of items under a root, with closed/all descendants per node.

use crate::item::{Item, State};
use crate::scheme::RelationScheme;
use crate::tracker::{Error, Tracker};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The version of the JSON shape `to_json` prints.
pub const JSON_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub iid: u64,
    pub title: String,
    pub state: State,
    pub closed: usize,
    pub total: usize,
    /// The items its `blocked_by` names that are still open.
    pub blocked_by_open: Vec<u64>,
    pub children: Vec<Node>,
}

/// The tree under `root`, or every root's tree when `root` is `None`, from every item.
pub fn tree(
    tracker: &dyn Tracker,
    scheme: &dyn RelationScheme,
    root: Option<u64>,
) -> Result<Vec<Node>, Error> {
    let items = tracker.all_items()?;
    draw(&items, &states(&items), scheme, root)
}

/// The tree under `root`, reading only that tree: one `level` request per depth along the links,
/// and one more for the blockers outside it.
pub fn live_tree(
    tracker: &dyn Tracker,
    scheme: &dyn RelationScheme,
    root: u64,
) -> Result<Vec<Node>, Error> {
    let items = subtree(tracker, scheme, root)?;
    let mut known = states(&items);
    let outside: Vec<u64> = blockers(&items)
        .into_iter()
        .filter(|b| !known.contains_key(b))
        .collect();
    if !outside.is_empty() {
        for (n, l) in tracker.level(&outside)? {
            known.insert(n, l.item.state);
        }
    }
    draw(&items, &known, scheme, Some(root))
}

/// Each item's state by number.
pub fn states(items: &[Item]) -> HashMap<u64, State> {
    items.iter().map(|it| (it.iid, it.state)).collect()
}

/// Every item the given items' `blocked_by` names, once each, in ascending order.
fn blockers(items: &[Item]) -> Vec<u64> {
    let set: std::collections::BTreeSet<u64> = items
        .iter()
        .flat_map(|it| crate::header::refs_of(&it.desc))
        .filter(|(k, _)| *k == "blocked_by")
        .map(|(_, b)| b)
        .collect();
    set.into_iter().collect()
}

/// The items `desc`'s `blocked_by` names that are open in `states`; an item the states do not know
/// is left out, since `check` reports it as missing.
pub fn open_blockers(desc: &str, states: &HashMap<u64, State>) -> Vec<u64> {
    crate::header::refs_of(desc)
        .into_iter()
        .filter(|(k, b)| *k == "blocked_by" && states.get(b) == Some(&State::Open))
        .map(|(_, b)| b)
        .collect()
}

/// `root` and its descendants. A linked item is a child only when the scheme names the node as its parent.
pub fn subtree(
    tracker: &dyn Tracker,
    scheme: &dyn RelationScheme,
    root: u64,
) -> Result<Vec<Item>, Error> {
    let mut found = tracker.level(&[root])?;
    let first = found.get(&root).ok_or(Error::NotFound(root))?;
    let mut out = vec![first.item.clone()];
    let mut seen = HashSet::from([root]);
    let mut nodes = vec![root];
    while !nodes.is_empty() {
        let mut next = Vec::new();
        for n in &nodes {
            let Some(at) = found.get(n) else { continue };
            for c in &at.linked {
                if scheme.parent_of(c) == Some(*n) && seen.insert(c.iid) {
                    out.push(c.clone());
                    next.push(c.iid);
                }
            }
        }
        found = if next.is_empty() {
            HashMap::new()
        } else {
            tracker.level(&next)?
        };
        nodes = next;
    }
    Ok(out)
}

fn draw(
    items: &[Item],
    states: &HashMap<u64, State>,
    scheme: &dyn RelationScheme,
    root: Option<u64>,
) -> Result<Vec<Node>, Error> {
    let by: HashMap<u64, &Item> = items.iter().map(|it| (it.iid, it)).collect();
    let mut kids: BTreeMap<u64, Vec<&Item>> = BTreeMap::new();
    let mut sorted: Vec<&Item> = items.iter().collect();
    sorted.sort_by_key(|it| it.iid);
    for it in &sorted {
        if let Some(p) = scheme.parent_of(it) {
            kids.entry(p).or_default().push(it);
        }
    }
    let roots: Vec<u64> = match root {
        Some(r) if by.contains_key(&r) => vec![r],
        Some(r) => return Err(Error::NotFound(r)),
        None => sorted
            .iter()
            .filter(|it| scheme.parent_of(it).is_none_or(|p| !by.contains_key(&p)))
            .map(|it| it.iid)
            .collect(),
    };
    Ok(roots
        .into_iter()
        .map(|r| node(r, &by, &kids, states, &mut HashSet::from([r])))
        .collect())
}

// `path` holds the items from the root down to this one, so a cycle ends where it meets itself.
fn node(
    n: u64,
    by: &HashMap<u64, &Item>,
    kids: &BTreeMap<u64, Vec<&Item>>,
    states: &HashMap<u64, State>,
    path: &mut HashSet<u64>,
) -> Node {
    let it = by[&n];
    let mut children = Vec::new();
    for c in kids.get(&n).map(Vec::as_slice).unwrap_or_default() {
        if path.insert(c.iid) {
            children.push(node(c.iid, by, kids, states, path));
            path.remove(&c.iid);
        }
    }
    let total = children.iter().map(|c| 1 + c.total).sum();
    let closed = children
        .iter()
        .map(|c| usize::from(c.state == State::Closed) + c.closed)
        .sum();
    Node {
        iid: n,
        title: it.title.clone(),
        state: it.state,
        closed,
        total,
        blocked_by_open: open_blockers(&it.desc, states),
        children,
    }
}

/// The tree as text: `#N [state] title`, then `  closed/total` when it has descendants and
/// `  blocked by #B, #C` while any blocker is open, two spaces per depth.
pub fn render(nodes: &[Node]) -> Vec<String> {
    let mut lines = Vec::new();
    for n in nodes {
        render_into(n, 0, &mut lines);
    }
    lines
}

fn render_into(n: &Node, depth: usize, lines: &mut Vec<String>) {
    let mut tail = if n.total > 0 {
        format!("  {}/{}", n.closed, n.total)
    } else {
        String::new()
    };
    if !n.blocked_by_open.is_empty() {
        let bs: Vec<String> = n.blocked_by_open.iter().map(|b| format!("#{b}")).collect();
        tail.push_str(&format!("  blocked by {}", bs.join(", ")));
    }
    lines.push(format!(
        "{}#{} [{}] {}{}",
        "  ".repeat(depth),
        n.iid,
        n.state.as_str(),
        n.title,
        tail
    ));
    for c in &n.children {
        render_into(c, depth + 1, lines);
    }
}

pub fn to_json(nodes: &[Node]) -> Value {
    json!({ "version": JSON_VERSION, "roots": nodes.iter().map(node_json).collect::<Vec<_>>() })
}

fn node_json(n: &Node) -> Value {
    json!({
        "number": n.iid, "title": n.title, "state": n.state.as_str(), "closed": n.closed, "total": n.total,
        "blocked_by_open": n.blocked_by_open,
        "children": n.children.iter().map(node_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme::FrontMatter;
    use crate::tracker::FakeTracker;

    fn item(iid: u64, state: State, desc: &str) -> Item {
        Item::new(iid, state, &format!("t{iid}"), desc)
    }

    fn draw(items: Vec<Item>, root: Option<u64>) -> Vec<String> {
        render(&tree(&FakeTracker::new(items), &FrontMatter, root).unwrap())
    }

    #[test]
    fn an_item_with_an_open_blocker_says_so_and_a_closed_blocker_is_dropped() {
        let mut items = three_levels();
        items[2].desc = "+++\nparent = 1\nblocked_by = [2, 5, 99]\n+++\n".into();
        let lines = draw(items.clone(), Some(1));
        assert_eq!(lines[2], "  #3 [open] t3  1/2  blocked by #5");
        let v = to_json(&tree(&FakeTracker::new(items), &FrontMatter, Some(1)).unwrap());
        assert_eq!(v["version"], 2);
        assert_eq!(v["roots"][0]["children"][1]["blocked_by_open"], json!([5]));
    }

    #[test]
    fn a_live_tree_reads_the_blockers_outside_it_in_one_more_request() {
        let mut items = three_levels();
        items.push(Item::new(7, State::Open, "elsewhere", ""));
        items[4].desc = "+++\nparent = 3\nblocked_by = [7]\n+++\n".into();
        let t = FakeTracker::new(linked(items));
        let lines = render(&live_tree(&t, &FrontMatter, 3).unwrap());
        assert_eq!(lines[2], "  #5 [open] t5  blocked by #7");
        assert_eq!(t.levels.borrow().last().unwrap(), &vec![7]);
    }

    fn three_levels() -> Vec<Item> {
        vec![
            Item::new(1, State::Open, "root", ""),
            item(2, State::Closed, "+++\nparent = 1\n+++\n"),
            item(3, State::Open, "+++\nparent = 1\n+++\n"),
            item(4, State::Closed, "+++\nparent = 3\n+++\n"),
            item(5, State::Open, "+++\nparent = 3\n+++\n"),
        ]
    }

    #[test]
    fn progress_counts_every_descendant_at_any_depth() {
        assert_eq!(
            draw(three_levels(), Some(1)),
            [
                "#1 [open] root  2/4",
                "  #2 [closed] t2",
                "  #3 [open] t3  1/2",
                "    #4 [closed] t4",
                "    #5 [open] t5",
            ]
        );
    }

    #[test]
    fn children_print_in_number_order_whatever_the_source_order() {
        let items = vec![
            item(7, State::Open, "+++\nparent = 1\n+++\n"),
            item(1, State::Open, ""),
            item(3, State::Open, "+++\nparent = 1\n+++\n"),
        ];
        assert_eq!(
            draw(items, Some(1))[1..],
            ["  #3 [open] t3", "  #7 [open] t7"]
        );
    }

    #[test]
    fn no_root_draws_every_root_and_an_item_whose_parent_is_absent() {
        let mut items = three_levels();
        items.push(item(9, State::Open, ""));
        items.push(item(8, State::Open, "+++\nparent = 99\n+++\n"));
        let roots: Vec<String> = draw(items, None)
            .into_iter()
            .filter(|l| !l.starts_with(' '))
            .collect();
        assert_eq!(
            roots,
            ["#1 [open] root  2/4", "#8 [open] t8", "#9 [open] t9"]
        );
    }

    #[test]
    fn a_cycle_stops_instead_of_looping() {
        let items = vec![
            item(1, State::Open, "+++\nparent = 2\n+++\n"),
            item(2, State::Open, "+++\nparent = 1\n+++\n"),
        ];
        assert_eq!(
            draw(items, Some(1)),
            ["#1 [open] t1  0/1", "  #2 [open] t2"]
        );
    }

    #[test]
    fn a_root_that_is_not_in_the_tracker_is_an_error() {
        let r = tree(&FakeTracker::new(three_levels()), &FrontMatter, Some(42));
        assert!(matches!(r, Err(Error::NotFound(42))));
    }

    fn linked(mut items: Vec<Item>) -> Vec<Item> {
        // every parent and child joined both ways, as the tracker convention requires
        let pairs: Vec<(u64, u64)> = items
            .iter()
            .filter_map(|it| FrontMatter.parent_of(it).map(|p| (it.iid, p)))
            .collect();
        for it in &mut items {
            for (c, p) in &pairs {
                if it.iid == *c {
                    it.links.get_or_insert_with(Vec::new).push(*p);
                }
                if it.iid == *p {
                    it.links.get_or_insert_with(Vec::new).push(*c);
                }
            }
        }
        items
    }

    #[test]
    fn the_walk_asks_one_level_per_depth() {
        let t = FakeTracker::new(linked(three_levels()));
        subtree(&t, &FrontMatter, 1).unwrap();
        assert_eq!(*t.levels.borrow(), vec![vec![1], vec![2, 3], vec![4, 5]]);
    }

    #[test]
    fn the_walk_draws_what_reading_every_item_draws() {
        let t = FakeTracker::new(linked(three_levels()));
        assert_eq!(
            live_tree(&t, &FrontMatter, 1).unwrap(),
            tree(&t, &FrontMatter, Some(1)).unwrap()
        );
        assert_eq!(
            live_tree(&t, &FrontMatter, 3).unwrap(),
            tree(&t, &FrontMatter, Some(3)).unwrap()
        );
    }

    #[test]
    fn a_linked_item_that_names_another_parent_is_not_a_child() {
        let mut items = linked(three_levels());
        let mut stray = item(6, State::Open, "+++\nparent = 9\n+++\n");
        stray.links = Some(vec![1]);
        items[0].links.get_or_insert_with(Vec::new).push(6);
        items.push(stray);
        let got: Vec<u64> = subtree(&FakeTracker::new(items), &FrontMatter, 1)
            .unwrap()
            .iter()
            .map(|it| it.iid)
            .collect();
        assert!(!got.contains(&6), "{got:?}");
        assert_eq!(got.len(), 5);
    }

    #[test]
    fn a_child_without_its_link_is_not_found_by_the_walk() {
        let mut items = linked(three_levels());
        items.iter_mut().for_each(|it| {
            let me = it.iid;
            if let Some(ls) = it.links.as_mut() {
                ls.retain(|l| *l != 5 && me != 5)
            }
        });
        let got: Vec<u64> = subtree(&FakeTracker::new(items), &FrontMatter, 1)
            .unwrap()
            .iter()
            .map(|it| it.iid)
            .collect();
        assert!(!got.contains(&5), "{got:?}");
    }

    #[test]
    fn the_walk_stops_at_a_cycle() {
        let items = linked(vec![
            item(1, State::Open, "+++\nparent = 2\n+++\n"),
            item(2, State::Open, "+++\nparent = 1\n+++\n"),
        ]);
        let t = FakeTracker::new(items);
        assert_eq!(subtree(&t, &FrontMatter, 1).unwrap().len(), 2);
        assert_eq!(
            render(&live_tree(&t, &FrontMatter, 1).unwrap()),
            ["#1 [open] t1  0/1", "  #2 [open] t2"]
        );
    }

    #[test]
    fn a_walk_from_an_absent_root_is_an_error() {
        let r = subtree(&FakeTracker::new(three_levels()), &FrontMatter, 42);
        assert!(matches!(r, Err(Error::NotFound(42))));
    }

    #[test]
    fn json_nests_the_same_tree_under_a_version() {
        let v = to_json(&tree(&FakeTracker::new(three_levels()), &FrontMatter, Some(3)).unwrap());
        assert_eq!(
            v,
            json!({
                "version": 2,
                "roots": [{
                    "number": 3, "title": "t3", "state": "open", "closed": 1, "total": 2, "blocked_by_open": [],
                    "children": [
                        {"number": 4, "title": "t4", "state": "closed", "closed": 0, "total": 0, "blocked_by_open": [], "children": []},
                        {"number": 5, "title": "t5", "state": "open", "closed": 0, "total": 0, "blocked_by_open": [], "children": []}
                    ]
                }]
            })
        );
    }
}
