//! Writing the tree: a new item with its header and links in one step, and moving an item.

use crate::check::{Family, StatePolicy};
use crate::header::{self, Header};
use crate::item::{Item, State};
use crate::scheme::FrontMatter;
use crate::tracker::{Created, Error, Tracker};
use serde_json::{Value, json};
use std::collections::HashSet;

/// The version of the JSON shape `moved_json` prints.
pub const JSON_VERSION: u32 = 1;
/// The version of the JSON shape `new_json` prints.
pub const NEW_JSON_VERSION: u32 = 2;

/// What a write must respect: the repo's opt-in, its state policy and its declared extra keys.
pub struct Rules<'a> {
    pub opted_in: bool,
    pub policy: &'a dyn StatePolicy,
    pub extra_keys: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct New {
    pub created: Created,
    /// `(parent, child)` when the item has a parent.
    pub link: Option<(u64, u64)>,
    /// The item it was found while working, when one was given.
    pub from: Option<u64>,
}

/// Refuses to hang `top`, with `below` (itself and what hangs under it), under `p` when the
/// state policy would then report `p` or an item above it as closed over an open item.
fn refuse_closed_above(
    tracker: &dyn Tracker,
    policy: &dyn StatePolicy,
    p: u64,
    top: &Item,
    below: &[&Item],
    alternative: &str,
) -> Result<(), Error> {
    let (mut cur, mut seen) = (Some(p), HashSet::new());
    while let Some(a) = cur.filter(|a| seen.insert(*a)) {
        let Some(above) = tracker.level(&[a])?.remove(&a) else {
            break;
        };
        let up = header::parent_of(&above.item.desc);
        let direct = [top];
        let family = Family {
            is_root: up.is_none(),
            children: if a == p { &direct } else { &[] },
            descendants: below,
        };
        if let Some(("closed-parent-open-child", _)) = policy.judge(&above.item, &family) {
            return Err(Error::Refused(format!(
                "#{a} is closed, and an open item under #{p} would leave it closed over an open one; reopen #{a}, or {alternative}"
            )));
        }
        cur = up;
    }
    Ok(())
}

/// Labels from repeated `--label` values, each of which may hold several separated by commas.
pub fn split_labels(given: &[String]) -> Vec<String> {
    given
        .iter()
        .flat_map(|l| l.split(','))
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

/// Refuses a write in a repo that has not committed a repo config.
pub fn require_opt_in(opted_in: bool) -> Result<(), Error> {
    if opted_in {
        return Ok(());
    }
    Err(Error::Refused(format!(
        "this repo has no {}; gltree writes only to a tracker whose repo commits one",
        crate::config::FILE
    )))
}

/// Creates an item under `parent` (a root when `None`), found while working `from`, with its
/// header above `text`, and links it to each. Refuses before any write when the repo has not
/// opted in, `parent` or `from` does not exist, `from` is `parent`, `text` already opens with a
/// header, or `parent` is closed where the policy would then report it.
pub fn new_item(
    tracker: &dyn Tracker,
    rules: &Rules,
    parent: Option<u64>,
    from: Option<u64>,
    title: &str,
    text: &str,
    labels: &[String],
) -> Result<New, Error> {
    require_opt_in(rules.opted_in)?;
    if text.starts_with(header::FENCE) {
        return Err(Error::Refused(format!(
            "the body already opens with `{}`; drop the header, gltree writes it from --parent and --from",
            header::FENCE
        )));
    }
    if from.is_some() && from == parent {
        return Err(Error::Refused(
            "--from names the parent; an item found while working its own parent is part of it, so give --parent alone".into(),
        ));
    }
    let wanted: Vec<u64> = parent.into_iter().chain(from).collect();
    if !wanted.is_empty() {
        let found = tracker.level(&wanted)?;
        if let Some(&n) = wanted.iter().find(|n| !found.contains_key(n)) {
            return Err(Error::NotFound(n));
        }
    }
    if let Some(p) = parent {
        let new = Item::new(0, State::Open, title, "");
        let alternative = format!("give the new item --from {p} instead of --parent");
        refuse_closed_above(tracker, rules.policy, p, &new, &[&new], &alternative)?;
    }
    let h = Header {
        parent,
        from,
        ..Header::default()
    };
    let body = header::with_header(text, &h).map_err(Error::Refused)?;
    let created = tracker.create_item(title, &body, labels)?;
    let c = created.iid;
    let targets: Vec<u64> = parent.into_iter().chain(from).collect();
    for (i, &n) in targets.iter().enumerate() {
        if let Err(e) = tracker.add_link(n, c) {
            let left: Vec<String> = targets[i..]
                .iter()
                .map(|n| format!("link #{n} and #{c}"))
                .collect();
            return Err(Error::Unfinished(format!(
                "created #{c} ({}), but a link failed ({e}); left to do: {}",
                created.url,
                left.join(", then ")
            )));
        }
    }
    Ok(New {
        created,
        link: parent.map(|p| (p, c)),
        from,
    })
}

pub fn new_json(n: &New) -> Value {
    let link = n
        .link
        .map(|(p, c)| json!({ "parent": p, "child": c }))
        .unwrap_or(Value::Null);
    json!({
        "version": NEW_JSON_VERSION,
        "created": { "number": n.created.iid, "url": n.created.url },
        "link": link,
        "from": n.from,
    })
}

/// A move: the item, its parent before and after, and whether anything was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    pub iid: u64,
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub changed: bool,
}

/// Gives `n` the parent `to`, or makes it a root for `None`: the body first, then the new link,
/// then the old link removed. Only the header's `parent` changes. Refuses before any write a repo
/// that has not opted in, a missing item or parent, a header out of form, a parent that is `n`'s
/// `from`, `n` or below it, and a closed parent the policy would then report.
pub fn move_item(
    tracker: &dyn Tracker,
    rules: &Rules,
    n: u64,
    to: Option<u64>,
) -> Result<Moved, Error> {
    require_opt_in(rules.opted_in)?;
    let item = tracker
        .level(&[n])?
        .remove(&n)
        .ok_or(Error::NotFound(n))?
        .item;
    let mut h = header::parse(&item.desc, rules.extra_keys)
        .map_err(|e| Error::Refused(format!("#{n}'s header is out of form ({e}); fix it first")))?
        .header
        .unwrap_or_default();
    let from = h.parent;
    if from == to {
        return Ok(Moved {
            iid: n,
            from,
            to,
            changed: false,
        });
    }
    if let Some(p) = to {
        if h.from == Some(p) {
            return Err(Error::Refused(format!(
                "#{p} is #{n}'s `from`; drop it before making #{p} its parent"
            )));
        }
        let below = crate::tree::subtree(tracker, &FrontMatter, n)?;
        if below.iter().any(|it| it.iid == p) {
            return Err(Error::Refused(format!(
                "#{p} is #{n} or below it; moving #{n} there would make a cycle"
            )));
        }
        if !tracker.level(&[p])?.contains_key(&p) {
            return Err(Error::NotFound(p));
        }
        let below: Vec<&Item> = below.iter().collect();
        let alternative = format!("record #{p} as #{n}'s `from` instead");
        refuse_closed_above(tracker, rules.policy, p, &item, &below, &alternative)?;
    }
    let mut left = Vec::new();
    if let Some(p) = to {
        left.push(format!("link #{p} and #{n}"));
    }
    if let Some(f) = from {
        left.push(format!("unlink #{f} and #{n}"));
    }
    h.parent = to;
    let body = header::with_header(&item.desc, &h).map_err(Error::Refused)?;
    tracker.set_desc(n, &body)?;
    let unfinished = |done: usize, e: Error| {
        Error::Unfinished(format!(
            "#{n}'s header now names {}, but a link step failed ({e}); left to do: {}",
            to.map_or("no parent".into(), |p| format!("#{p} as its parent")),
            left[done..].join(", then ")
        ))
    };
    let mut done = 0;
    if let Some(p) = to {
        tracker.add_link(p, n).map_err(|e| unfinished(done, e))?;
        done += 1;
    }
    if let Some(f) = from {
        tracker.remove_link(f, n).map_err(|e| unfinished(done, e))?;
    }
    Ok(Moved {
        iid: n,
        from,
        to,
        changed: true,
    })
}

pub fn moved_json(m: &Moved) -> Value {
    json!({ "version": JSON_VERSION, "item": m.iid, "from": m.from, "to": m.to, "changed": m.changed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::FakeTracker;

    /// A body whose header names `n` as the parent.
    fn p(n: u64) -> String {
        format!("+++\nparent = {n}\n+++\n")
    }

    fn rules(opted_in: bool) -> Rules<'static> {
        Rules {
            opted_in,
            policy: &crate::check::RootsOnly,
            extra_keys: &[],
        }
    }

    fn tracker() -> FakeTracker {
        let mut root = Item::new(1, State::Open, "root", "");
        root.links = Some(vec![]);
        FakeTracker::new(vec![root])
    }

    fn labels(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    fn clean(t: &FakeTracker) -> Vec<crate::check::Finding> {
        crate::check::check(
            &t.all_items().unwrap(),
            &FrontMatter,
            &crate::check::RootsOnly,
            &[],
        )
    }

    #[test]
    fn a_child_is_created_with_its_header_then_linked_in_that_order() {
        let t = tracker();
        let n = new_item(
            &t,
            &rules(true),
            Some(1),
            None,
            "A child",
            "설정 「외관」\r\nbody",
            &labels(&["bug", "UI:Dialog"]),
        )
        .unwrap();
        assert_eq!(n.link, Some((1, 2)));
        assert_eq!(
            *t.writes.borrow(),
            vec![
                r#"create #2 "A child" ["bug", "UI:Dialog"]"#.to_string(),
                "link #1 #2".to_string()
            ]
        );
        assert_eq!(
            t.item(2).unwrap().desc,
            "+++\nparent = 1\n+++\n\n설정 「외관」\r\nbody"
        );
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn without_a_parent_or_from_it_is_a_root_with_no_header_and_no_link() {
        let t = tracker();
        let n = new_item(&t, &rules(true), None, None, "Alone", "text", &[]).unwrap();
        assert_eq!(n.link, None);
        assert_eq!(t.item(2).unwrap().desc, "text");
        assert_eq!(t.writes.borrow().len(), 1);
    }

    #[test]
    fn an_empty_body_is_the_header_alone() {
        let t = tracker();
        new_item(&t, &rules(true), Some(1), None, "x", "", &[]).unwrap();
        assert_eq!(t.item(2).unwrap().desc, p(1));
    }

    #[test]
    fn labels_repeat_or_split_on_commas_and_blanks_drop() {
        assert_eq!(
            split_labels(&labels(&["bug, UI:Dialog", "ready-for-agent", " ", ""])),
            labels(&["bug", "UI:Dialog", "ready-for-agent"])
        );
    }

    #[test]
    fn a_repo_that_has_not_opted_in_is_refused_before_anything_is_read() {
        let t = tracker();
        assert!(matches!(
            new_item(&t, &rules(false), Some(1), None, "x", "y", &[]),
            Err(Error::Refused(_))
        ));
        assert!(t.writes.borrow().is_empty() && t.levels.borrow().is_empty());
    }

    #[test]
    fn a_parent_that_does_not_exist_is_refused_before_writing() {
        let t = tracker();
        assert!(matches!(
            new_item(&t, &rules(true), Some(99), None, "x", "y", &[]),
            Err(Error::NotFound(99))
        ));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_body_that_already_opens_with_a_header_is_refused() {
        let t = tracker();
        let r = new_item(
            &t,
            &rules(true),
            Some(1),
            None,
            "x",
            &format!("{}\ny", p(1)),
            &[],
        );
        assert!(matches!(r, Err(Error::Refused(m)) if m.contains("+++")));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failed_link_names_the_item_and_what_is_left_to_do() {
        let t = tracker();
        t.fail_links.set(true);
        let e = new_item(&t, &rules(true), Some(1), None, "x", "y", &[]).unwrap_err();
        let m = e.to_string();
        assert!(matches!(e, Error::Unfinished(_)), "{m}");
        assert!(m.ends_with("left to do: link #1 and #2"), "{m}");
    }

    fn tree3() -> FakeTracker {
        // 1 ─ 2 ─ 3, and 4 a lone root; every parent and child linked
        let mut items = vec![
            Item::new(1, State::Open, "t1", ""),
            Item::new(
                2,
                State::Open,
                "t2",
                "+++\nparent = 1\nrelated = [4]\n\n[notes]\nrelated = \"same screen\"\n+++\n\nWaits on the store.\nbody",
            ),
            Item::new(3, State::Open, "t3", &p(2)),
            Item::new(4, State::Open, "t4", "lone"),
        ];
        let links = [vec![2], vec![1, 3, 4], vec![2], vec![2]];
        for (it, l) in items.iter_mut().zip(links) {
            it.links = Some(l);
        }
        FakeTracker::new(items)
    }

    #[test]
    fn a_child_moves_to_a_new_parent_body_first_then_new_link_then_old_unlinked() {
        let t = tree3();
        let m = move_item(&t, &rules(true), 2, Some(4)).unwrap();
        assert_eq!((m.from, m.to, m.changed), (Some(1), Some(4), true));
        let writes: Vec<String> = t.writes.borrow().clone();
        assert_eq!(writes, ["desc #2", "link #4 #2", "unlink #1 #2"]);
        assert_eq!(
            t.item(2).unwrap().desc,
            "+++\nparent = 4\nrelated = [4]\n\n[notes]\nrelated = \"same screen\"\n+++\n\nWaits on the store.\nbody"
        );
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_root_moves_under_a_parent_with_the_header_new_writes() {
        let t = tree3();
        move_item(&t, &rules(true), 4, Some(3)).unwrap();
        assert_eq!(t.item(4).unwrap().desc, format!("{}\nlone", p(3)));
        assert_eq!(*t.writes.borrow(), ["desc #4", "link #3 #4"]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn to_root_keeps_the_rest_of_the_header_and_drops_a_header_left_empty() {
        let t = tree3();
        let m = move_item(&t, &rules(true), 2, None).unwrap();
        assert_eq!((m.from, m.to), (Some(1), None));
        assert_eq!(
            t.item(2).unwrap().desc,
            "+++\nrelated = [4]\n\n[notes]\nrelated = \"same screen\"\n+++\n\nWaits on the store.\nbody"
        );
        assert_eq!(*t.writes.borrow(), ["desc #2", "unlink #1 #2"]);
        move_item(&t, &rules(true), 3, None).unwrap();
        assert_eq!(t.item(3).unwrap().desc, "");
    }

    #[test]
    fn new_then_to_root_gives_back_the_text() {
        let t = tree3();
        let text = "first\r\n\r\nsecond";
        let made = new_item(&t, &rules(true), Some(1), None, "x", text, &[]).unwrap();
        move_item(&t, &rules(true), made.created.iid, None).unwrap();
        assert_eq!(t.item(made.created.iid).unwrap().desc, text);
    }

    #[test]
    fn a_header_out_of_form_is_refused_before_any_write() {
        let t = tree3();
        t.items.borrow_mut()[3].desc = "+++\nparnet = 1\n+++\n".into();
        let m = move_item(&t, &rules(true), 4, Some(1))
            .unwrap_err()
            .to_string();
        assert!(m.contains("parnet") && m.contains("fix it first"), "{m}");
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_find_is_created_with_from_in_its_header_and_both_links() {
        let t = tree3();
        let n = new_item(&t, &rules(true), Some(4), Some(2), "found", "body", &[]).unwrap();
        assert_eq!((n.link, n.from), (Some((4, 5)), Some(2)));
        assert_eq!(
            t.item(5).unwrap().desc,
            "+++\nparent = 4\nfrom = 2\n+++\n\nbody"
        );
        assert_eq!(
            t.writes.borrow()[1..],
            ["link #4 #5".to_string(), "link #2 #5".to_string()]
        );
        let r = new_item(&t, &rules(true), None, Some(3), "alone", "", &[]).unwrap();
        assert_eq!(t.item(r.created.iid).unwrap().desc, "+++\nfrom = 3\n+++\n");
    }

    #[test]
    fn from_equal_to_parent_or_missing_is_refused_before_any_write() {
        let t = tree3();
        assert!(matches!(
            new_item(&t, &rules(true), Some(2), Some(2), "x", "y", &[]),
            Err(Error::Refused(m)) if m.contains("--parent alone")
        ));
        assert!(matches!(
            new_item(&t, &rules(true), None, Some(99), "x", "y", &[]),
            Err(Error::NotFound(99))
        ));
        t.items.borrow_mut()[3].desc = "+++\nfrom = 1\n+++\n".into();
        assert!(matches!(
            move_item(&t, &rules(true), 4, Some(1)),
            Err(Error::Refused(m)) if m.contains("`from`")
        ));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failed_from_link_leaves_only_the_links_not_yet_made() {
        let t = tree3();
        t.fail_links.set(true);
        let m = new_item(&t, &rules(true), Some(4), Some(2), "x", "y", &[])
            .unwrap_err()
            .to_string();
        assert!(
            m.ends_with("left to do: link #4 and #5, then link #2 and #5"),
            "{m}"
        );
    }

    fn closed(t: &FakeTracker, n: u64) {
        let mut items = t.items.borrow_mut();
        items.iter_mut().find(|i| i.iid == n).unwrap().state = State::Closed;
    }

    fn every_parent() -> Rules<'static> {
        Rules {
            policy: &crate::check::EveryParent,
            ..rules(true)
        }
    }

    #[test]
    fn an_open_item_is_not_put_under_a_parent_the_policy_would_then_report() {
        let t = tree3();
        closed(&t, 3);
        let m = new_item(&t, &every_parent(), Some(3), None, "x", "y", &[])
            .unwrap_err()
            .to_string();
        assert!(m.contains("#3 is closed") && m.contains("--from 3"), "{m}");
        let m = move_item(&t, &every_parent(), 4, Some(3))
            .unwrap_err()
            .to_string();
        assert!(m.contains("#3 is closed") && m.contains("`from`"), "{m}");
        assert!(t.writes.borrow().is_empty());
        // roots-only lets a closed slice take a child, but not when its root is closed too
        assert!(new_item(&t, &rules(true), Some(3), None, "x", "y", &[]).is_ok());
        closed(&t, 1);
        let m = new_item(&t, &rules(true), Some(3), None, "x", "y", &[])
            .unwrap_err()
            .to_string();
        assert!(m.contains("#1 is closed") && m.contains("under #3"), "{m}");
    }

    #[test]
    fn a_closed_item_may_go_under_a_closed_parent() {
        let t = tree3();
        closed(&t, 3);
        closed(&t, 4);
        assert!(move_item(&t, &every_parent(), 4, Some(3)).unwrap().changed);
    }

    #[test]
    fn a_parent_that_is_the_item_or_below_it_is_refused_before_any_write() {
        let t = tree3();
        assert!(
            matches!(move_item(&t, &rules(true), 1, Some(1)), Err(Error::Refused(m)) if m.contains("cycle"))
        );
        assert!(
            matches!(move_item(&t, &rules(true), 1, Some(3)), Err(Error::Refused(m)) if m.contains("cycle"))
        );
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_rerun_changes_nothing() {
        let t = tree3();
        let m = move_item(&t, &rules(true), 2, Some(1)).unwrap();
        assert!(!m.changed);
        let m = move_item(&t, &rules(true), 4, None).unwrap();
        assert!(!m.changed);
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_missing_item_or_parent_and_a_repo_without_config_are_refused_before_any_write() {
        let t = tree3();
        assert!(matches!(
            move_item(&t, &rules(true), 99, Some(1)),
            Err(Error::NotFound(99))
        ));
        assert!(matches!(
            move_item(&t, &rules(true), 3, Some(99)),
            Err(Error::NotFound(99))
        ));
        assert!(matches!(
            move_item(&t, &rules(false), 2, Some(4)),
            Err(Error::Refused(_))
        ));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failure_after_the_body_was_written_says_what_is_left() {
        let t = tree3();
        t.fail_links.set(true);
        let e = move_item(&t, &rules(true), 2, Some(4)).unwrap_err();
        let m = e.to_string();
        assert!(matches!(e, Error::Unfinished(_)), "{m}");
        assert!(
            m.ends_with("left to do: link #4 and #2, then unlink #1 and #2"),
            "{m}"
        );
    }

    #[test]
    fn when_only_the_old_link_failed_only_the_unlink_is_left() {
        let t = tree3();
        t.fail_unlinks.set(true);
        let m = move_item(&t, &rules(true), 2, Some(4))
            .unwrap_err()
            .to_string();
        assert!(m.ends_with("left to do: unlink #1 and #2"), "{m}");
    }

    #[test]
    fn json_reports_the_move() {
        let m = Moved {
            iid: 2,
            from: Some(1),
            to: None,
            changed: true,
        };
        assert_eq!(
            moved_json(&m),
            json!({ "version": 1, "item": 2, "from": 1, "to": null, "changed": true })
        );
    }

    #[test]
    fn json_reports_the_item_the_link_and_from() {
        let n = New {
            created: Created {
                iid: 2,
                url: "u".into(),
            },
            link: Some((1, 2)),
            from: None,
        };
        assert_eq!(
            new_json(&n),
            json!({ "version": 2, "created": { "number": 2, "url": "u" }, "link": { "parent": 1, "child": 2 }, "from": null })
        );
        let root = New {
            created: Created {
                iid: 3,
                url: "v".into(),
            },
            link: None,
            from: Some(9),
        };
        assert_eq!(new_json(&root)["link"], Value::Null);
        assert_eq!(new_json(&root)["from"], 9);
    }
}
