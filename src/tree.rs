//! A tree of items under a root, with closed/all descendants per node.

use crate::item::{Item, State};
use crate::scheme::RelationScheme;
use crate::tracker::{Error, Tracker};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The version of the JSON shape `to_json` prints.
pub const JSON_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub iid: u64,
    pub title: String,
    pub state: State,
    pub closed: usize,
    pub total: usize,
    pub children: Vec<Node>,
}

/// The tree under `root`, or every root's tree when `root` is `None`.
pub fn tree(
    tracker: &dyn Tracker,
    scheme: &dyn RelationScheme,
    root: Option<u64>,
) -> Result<Vec<Node>, Error> {
    let items = tracker.all_items()?;
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
        .map(|r| node(r, &by, &kids, &mut HashSet::from([r])))
        .collect())
}

// `path` holds the items from the root down to this one, so a cycle ends where it meets itself.
fn node(
    n: u64,
    by: &HashMap<u64, &Item>,
    kids: &BTreeMap<u64, Vec<&Item>>,
    path: &mut HashSet<u64>,
) -> Node {
    let it = by[&n];
    let mut children = Vec::new();
    for c in kids.get(&n).map(Vec::as_slice).unwrap_or_default() {
        if path.insert(c.iid) {
            children.push(node(c.iid, by, kids, path));
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
        children,
    }
}

/// The tree as text: `#N [state] title`, then `  closed/total` when it has descendants, two spaces per depth.
pub fn render(nodes: &[Node]) -> Vec<String> {
    let mut lines = Vec::new();
    for n in nodes {
        render_into(n, 0, &mut lines);
    }
    lines
}

fn render_into(n: &Node, depth: usize, lines: &mut Vec<String>) {
    let tail = if n.total > 0 {
        format!("  {}/{}", n.closed, n.total)
    } else {
        String::new()
    };
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
        "children": n.children.iter().map(node_json).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme::ParentLine;
    use crate::tracker::FakeTracker;

    fn item(iid: u64, state: State, desc: &str) -> Item {
        Item::new(iid, state, &format!("t{iid}"), desc)
    }

    fn draw(items: Vec<Item>, root: Option<u64>) -> Vec<String> {
        render(&tree(&FakeTracker { items }, &ParentLine, root).unwrap())
    }

    fn three_levels() -> Vec<Item> {
        vec![
            Item::new(1, State::Open, "root", ""),
            item(2, State::Closed, "Parent: #1"),
            item(3, State::Open, "Parent: #1"),
            item(4, State::Closed, "Parent: #3"),
            item(5, State::Open, "Parent: #3"),
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
            item(7, State::Open, "Parent: #1"),
            item(1, State::Open, ""),
            item(3, State::Open, "Parent: #1"),
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
        items.push(item(8, State::Open, "Parent: #99"));
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
            item(1, State::Open, "Parent: #2"),
            item(2, State::Open, "Parent: #1"),
        ];
        assert_eq!(
            draw(items, Some(1)),
            ["#1 [open] t1  0/1", "  #2 [open] t2"]
        );
    }

    #[test]
    fn a_root_that_is_not_in_the_tracker_is_an_error() {
        let r = tree(
            &FakeTracker {
                items: three_levels(),
            },
            &ParentLine,
            Some(42),
        );
        assert!(matches!(r, Err(Error::NotFound(42))));
    }

    #[test]
    fn json_nests_the_same_tree_under_a_version() {
        let v = to_json(
            &tree(
                &FakeTracker {
                    items: three_levels(),
                },
                &ParentLine,
                Some(3),
            )
            .unwrap(),
        );
        assert_eq!(
            v,
            json!({
                "version": 1,
                "roots": [{
                    "number": 3, "title": "t3", "state": "open", "closed": 1, "total": 2,
                    "children": [
                        {"number": 4, "title": "t4", "state": "closed", "closed": 0, "total": 0, "children": []},
                        {"number": 5, "title": "t5", "state": "open", "closed": 0, "total": 0, "children": []}
                    ]
                }]
            })
        );
    }
}
