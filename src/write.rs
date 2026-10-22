//! Writing the tree: a new item with its Parent line, origin line and links in one step.

use crate::check::{Family, StatePolicy};
use crate::item::{Item, State};
use crate::scheme::{OriginLine, ParentLine};
use crate::tracker::{Created, Error, Tracker};
use serde_json::{Value, json};
use std::collections::HashSet;

/// The version of the JSON shape `new_json` prints.
pub const JSON_VERSION: u32 = 1;

/// What a write must respect: the repo's opt-in, its state policy and its origin line.
pub struct Rules<'a> {
    pub opted_in: bool,
    pub policy: &'a dyn StatePolicy,
    pub origin: Option<&'a OriginLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct New {
    pub created: Created,
    /// `(parent, child)` when the item has a parent.
    pub link: Option<(u64, u64)>,
    /// The item it was found while working, when one was given.
    pub origin: Option<u64>,
}

/// The body gltree writes: `Parent: #P`, a blank line, then `text`; `text` alone for a root.
pub fn body_with_parent(parent: Option<u64>, text: &str) -> String {
    body_with(parent, None, text)
}

/// `Parent: #P`, then `<word>: #F`, then a blank line and `text`, each line only when given.
pub fn body_with(parent: Option<u64>, origin: Option<(&OriginLine, u64)>, text: &str) -> String {
    let mut head = Vec::new();
    if let Some(p) = parent {
        head.push(format!("Parent: #{p}"));
    }
    if let Some((r, f)) = origin {
        head.push(format!("{}: #{f}", r.word));
    }
    match (head.is_empty(), text.is_empty()) {
        (true, _) => text.into(),
        (false, true) => head.join("\n"),
        (false, false) => format!("{}\n\n{text}", head.join("\n")),
    }
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
        let up = ParentLine::parse(&above.item.desc);
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

/// Creates an item under `parent` (a root when `None`), found while working `from`, and links it
/// to each. Refuses before any write when the repo has not opted in, `parent` or `from` does not
/// exist, `text` already opens with a Parent or origin line, or `parent` is closed where the
/// policy would then report it.
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
    if ParentLine::parse(text).is_some() {
        return Err(Error::Refused(
            "the body already opens with a Parent line; drop it, gltree writes the line from --parent".into(),
        ));
    }
    if let Some(r) = rules.origin
        && r.parse_line(text.split('\n').next().unwrap_or(""))
            .is_some()
    {
        return Err(Error::Refused(format!(
            "the body already opens with a {} line; drop it, gltree writes the line from --from",
            r.word
        )));
    }
    let origin = match (from, rules.origin) {
        (Some(f), Some(r)) => Some((r, f)),
        (Some(_), None) => {
            return Err(Error::Refused(
                "this repo's config names no origin_word, so there is no origin line to write"
                    .into(),
            ));
        }
        (None, _) => None,
    };
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
    let created = tracker.create_item(title, &body_with(parent, origin, text), labels)?;
    let c = created.iid;
    let targets: Vec<u64> = parent
        .into_iter()
        .chain(from.filter(|f| Some(*f) != parent))
        .collect();
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
        origin: from,
    })
}

pub fn new_json(n: &New) -> Value {
    let link = n
        .link
        .map(|(p, c)| json!({ "parent": p, "child": c }))
        .unwrap_or(Value::Null);
    json!({
        "version": JSON_VERSION,
        "created": { "number": n.created.iid, "url": n.created.url },
        "link": link,
        "origin": n.origin,
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

/// `desc` with its parent set to `to`: the first line replaced or a Parent line added, with a
/// blank line under it unless an origin line follows; for `None`, the Parent line and the one
/// blank line under it removed, an origin line taking its place.
pub fn body_moved(desc: &str, to: Option<u64>, origin: Option<&OriginLine>) -> String {
    if ParentLine::parse(desc).is_none() {
        let first = desc.split('\n').next().unwrap_or("");
        return match to {
            Some(p) if origin.is_some_and(|r| r.parse_line(first).is_some()) => {
                let eol = if first.ends_with('\r') { "\r\n" } else { "\n" };
                format!("Parent: #{p}{eol}{desc}")
            }
            Some(_) => body_with_parent(to, desc),
            None => desc.into(),
        };
    }
    let (first, rest) = match desc.find('\n') {
        Some(i) => (&desc[..i], &desc[i..]),
        None => (desc, ""),
    };
    match to {
        Some(p) => {
            let cr = if first.ends_with('\r') { "\r" } else { "" };
            format!("Parent: #{p}{cr}{rest}")
        }
        None => {
            let rest = rest.strip_prefix('\n').unwrap_or(rest);
            let rest = rest
                .strip_prefix("\r\n")
                .or_else(|| rest.strip_prefix('\n'))
                .unwrap_or(rest);
            rest.into()
        }
    }
}

/// Gives `n` the parent `to`, or makes it a root for `None`: the body first, then the new link,
/// then the old link removed. Refuses before any write a repo that has not opted in, a missing
/// item or parent, a parent that is `n` or below it, and a closed parent the policy would then
/// report.
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
    let from = ParentLine::parse(&item.desc);
    if from == to {
        return Ok(Moved {
            iid: n,
            from,
            to,
            changed: false,
        });
    }
    if let Some(p) = to {
        let below = crate::tree::subtree(tracker, &ParentLine, n)?;
        if below.iter().any(|it| it.iid == p) {
            return Err(Error::Refused(format!(
                "#{p} is #{n} or below it; moving #{n} there would make a cycle"
            )));
        }
        if !tracker.level(&[p])?.contains_key(&p) {
            return Err(Error::NotFound(p));
        }
        let below: Vec<&Item> = below.iter().collect();
        let alternative = format!("record #{p} as #{n}'s origin instead");
        refuse_closed_above(tracker, rules.policy, p, &item, &below, &alternative)?;
    }
    let mut left = Vec::new();
    if let Some(p) = to {
        left.push(format!("link #{p} and #{n}"));
    }
    if let Some(f) = from {
        left.push(format!("unlink #{f} and #{n}"));
    }
    tracker.set_desc(n, &body_moved(&item.desc, to, rules.origin))?;
    let unfinished = |done: usize, e: Error| {
        Error::Unfinished(format!(
            "#{n}'s Parent line now names {}, but a link step failed ({e}); left to do: {}",
            to.map_or("no parent".into(), |p| format!("#{p}")),
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

    fn from() -> OriginLine {
        OriginLine::new("From")
    }

    fn rules(opted_in: bool) -> Rules<'static> {
        static FROM: std::sync::OnceLock<OriginLine> = std::sync::OnceLock::new();
        Rules {
            opted_in,
            policy: &crate::check::RootsOnly,
            origin: Some(FROM.get_or_init(from)),
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

    #[test]
    fn a_child_is_created_with_its_parent_line_then_linked_in_that_order() {
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
            "Parent: #1\n\n설정 「외관」\r\nbody"
        );
    }

    #[test]
    fn the_new_child_passes_check() {
        let t = tracker();
        new_item(&t, &rules(true), Some(1), None, "A child", "x", &[]).unwrap();
        let found = crate::check::check(
            &t.all_items().unwrap(),
            &ParentLine,
            &crate::check::RootsOnly,
            crate::check::Links::TreeOnly,
            Some(&from()),
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn without_a_parent_it_is_a_root_with_no_link() {
        let t = tracker();
        let n = new_item(&t, &rules(true), None, None, "Alone", "text", &[]).unwrap();
        assert_eq!(n.link, None);
        assert_eq!(t.item(2).unwrap().desc, "text");
        assert_eq!(t.writes.borrow().len(), 1);
    }

    #[test]
    fn labels_repeat_or_split_on_commas_and_blanks_drop() {
        assert_eq!(
            split_labels(&labels(&["bug, UI:Dialog", "ready-for-agent", " ", ""])),
            labels(&["bug", "UI:Dialog", "ready-for-agent"])
        );
    }

    #[test]
    fn an_empty_body_is_the_parent_line_alone() {
        assert_eq!(body_with_parent(Some(7), ""), "Parent: #7");
        assert_eq!(body_with_parent(None, ""), "");
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
    fn a_body_that_already_opens_with_a_parent_line_is_refused() {
        let t = tracker();
        let r = new_item(&t, &rules(true), Some(1), None, "x", "Parent: #1\n\ny", &[]);
        assert!(matches!(r, Err(Error::Refused(m)) if m.contains("Parent")));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failed_link_names_the_item_and_what_is_left_to_do() {
        let t = tracker();
        t.fail_links.set(true);
        let e = new_item(&t, &rules(true), Some(1), None, "x", "y", &[]).unwrap_err();
        let m = e.to_string();
        assert!(matches!(e, Error::Unfinished(_)), "{m}");
        assert!(
            m.contains("#2") && m.contains("#1") && m.contains("link"),
            "{m}"
        );
    }

    fn tree3() -> FakeTracker {
        // 1 ─ 2 ─ 3, and 4 a lone root; every parent and child linked
        let mut items = vec![
            Item::new(1, State::Open, "t1", ""),
            Item::new(
                2,
                State::Open,
                "t2",
                "Parent: #1\n\nBlocked by: #4\nNIT: skipped\nbody",
            ),
            Item::new(3, State::Open, "t3", "Parent: #2"),
            Item::new(4, State::Open, "t4", "lone"),
        ];
        let links = [vec![2], vec![1, 3], vec![2], vec![]];
        for (it, l) in items.iter_mut().zip(links) {
            it.links = Some(l);
        }
        FakeTracker::new(items)
    }

    fn clean(t: &FakeTracker) -> Vec<crate::check::Finding> {
        crate::check::check(
            &t.all_items().unwrap(),
            &ParentLine,
            &crate::check::RootsOnly,
            crate::check::Links::TreeOnly,
            Some(&from()),
        )
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
            "Parent: #4\n\nBlocked by: #4\nNIT: skipped\nbody"
        );
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_root_moves_under_a_parent_with_the_line_new_writes() {
        let t = tree3();
        move_item(&t, &rules(true), 4, Some(3)).unwrap();
        assert_eq!(t.item(4).unwrap().desc, "Parent: #3\n\nlone");
        assert_eq!(*t.writes.borrow(), ["desc #4", "link #3 #4"]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn to_root_drops_the_line_and_its_blank_line_and_the_old_link_only() {
        let t = tree3();
        let m = move_item(&t, &rules(true), 2, None).unwrap();
        assert_eq!((m.from, m.to), (Some(1), None));
        assert_eq!(
            t.item(2).unwrap().desc,
            "Blocked by: #4\nNIT: skipped\nbody"
        );
        assert_eq!(*t.writes.borrow(), ["desc #2", "unlink #1 #2"]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
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
    fn a_crlf_first_line_keeps_its_line_ending() {
        assert_eq!(
            body_moved("Parent: #1\r\n\r\nx", Some(4), None),
            "Parent: #4\r\n\r\nx"
        );
        assert_eq!(body_moved("Parent: #1\r\n\r\nx", None, None), "x");
        assert_eq!(body_moved("Parent: #1", None, None), "");
        assert_eq!(body_moved("Parent: #1\nx", None, None), "x");
    }

    #[test]
    fn the_origin_line_stays_directly_under_the_parent_line() {
        let o = from();
        assert_eq!(
            body_moved("From: #9 (x)\n\nbody", Some(4), Some(&o)),
            "Parent: #4\nFrom: #9 (x)\n\nbody"
        );
        assert_eq!(
            body_moved("From: #9\r\n\r\nbody", Some(4), Some(&o)),
            "Parent: #4\r\nFrom: #9\r\n\r\nbody"
        );
        assert_eq!(
            body_moved("Parent: #4\nFrom: #9\n\nbody", None, Some(&o)),
            "From: #9\n\nbody"
        );
        assert_eq!(
            body_moved("Parent: #4\nFrom: #9\n\nbody", Some(5), Some(&o)),
            "Parent: #5\nFrom: #9\n\nbody"
        );
    }

    #[test]
    fn a_find_is_created_with_its_origin_line_and_both_links() {
        let t = tree3();
        let n = new_item(&t, &rules(true), Some(4), Some(2), "found", "body", &[]).unwrap();
        assert_eq!((n.link, n.origin), (Some((4, 5)), Some(2)));
        assert_eq!(t.item(5).unwrap().desc, "Parent: #4\nFrom: #2\n\nbody");
        assert_eq!(
            t.writes.borrow()[1..],
            ["link #4 #5".to_string(), "link #2 #5".to_string()]
        );
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
        let r = new_item(&t, &rules(true), None, Some(3), "alone", "", &[]).unwrap();
        assert_eq!(t.item(r.created.iid).unwrap().desc, "From: #3");
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_body_opening_with_an_origin_line_or_a_missing_origin_is_refused() {
        let t = tree3();
        assert!(matches!(
            new_item(&t, &rules(true), None, Some(2), "x", "From: #1\n\ny", &[]),
            Err(Error::Refused(m)) if m.contains("From")
        ));
        assert!(matches!(
            new_item(&t, &rules(true), None, Some(99), "x", "y", &[]),
            Err(Error::NotFound(99))
        ));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failed_origin_link_leaves_only_that_link_to_do() {
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
        assert!(m.contains("#3 is closed") && m.contains("origin"), "{m}");
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
            move_item(&t, &rules(true), 2, Some(99)),
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
        assert!(m.contains("link #4 and #2") && m.contains("#1"), "{m}");
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
    fn json_reports_the_item_and_the_link() {
        let n = New {
            created: Created {
                iid: 2,
                url: "u".into(),
            },
            link: Some((1, 2)),
            origin: None,
        };
        assert_eq!(
            new_json(&n),
            json!({ "version": 1, "created": { "number": 2, "url": "u" }, "link": { "parent": 1, "child": 2 }, "origin": null })
        );
        let root = New {
            created: Created {
                iid: 3,
                url: "v".into(),
            },
            link: None,
            origin: Some(9),
        };
        assert_eq!(new_json(&root)["link"], Value::Null);
        assert_eq!(new_json(&root)["origin"], 9);
    }
}
