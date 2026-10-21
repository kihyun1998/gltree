//! Writing the tree: a new item with its Parent line and link in one step.

use crate::scheme::ParentLine;
use crate::tracker::{Created, Error, Tracker};
use serde_json::{Value, json};

/// The version of the JSON shape `new_json` prints.
pub const JSON_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct New {
    pub created: Created,
    /// `(parent, child)` when the item has a parent.
    pub link: Option<(u64, u64)>,
}

/// The body gltree writes: `Parent: #P`, a blank line, then `text`; `text` alone for a root.
pub fn body_with_parent(parent: Option<u64>, text: &str) -> String {
    match parent {
        None => text.into(),
        Some(p) if text.is_empty() => format!("Parent: #{p}"),
        Some(p) => format!("Parent: #{p}\n\n{text}"),
    }
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

/// Creates an item under `parent` (a root when `None`) and links it, refusing before any write
/// when the repo has not opted in, the parent does not exist, or `text` already opens with a
/// Parent line.
pub fn new_item(
    tracker: &dyn Tracker,
    opted_in: bool,
    parent: Option<u64>,
    title: &str,
    text: &str,
    labels: &[String],
) -> Result<New, Error> {
    require_opt_in(opted_in)?;
    if ParentLine::parse(text).is_some() {
        return Err(Error::Refused(
            "the body already opens with a Parent line; drop it, gltree writes the line from --parent".into(),
        ));
    }
    if let Some(p) = parent
        && !tracker.level(&[p])?.contains_key(&p)
    {
        return Err(Error::NotFound(p));
    }
    let created = tracker.create_item(title, &body_with_parent(parent, text), labels)?;
    let Some(p) = parent else {
        return Ok(New {
            created,
            link: None,
        });
    };
    if let Err(e) = tracker.add_link(p, created.iid) {
        return Err(Error::Unfinished(format!(
            "created #{} ({}), but linking it to its parent failed ({e}); left to do: link #{p} and #{}",
            created.iid, created.url, created.iid
        )));
    }
    let child = created.iid;
    Ok(New {
        created,
        link: Some((p, child)),
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

/// `desc` with its parent set to `to`: the first line replaced or a Parent line and blank line
/// added; for `None`, the Parent line and the one blank line under it removed.
pub fn body_moved(desc: &str, to: Option<u64>) -> String {
    if ParentLine::parse(desc).is_none() {
        return match to {
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
/// item or parent, and a parent that is `n` or below it.
pub fn move_item(
    tracker: &dyn Tracker,
    opted_in: bool,
    n: u64,
    to: Option<u64>,
) -> Result<Moved, Error> {
    require_opt_in(opted_in)?;
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
    }
    let mut left = Vec::new();
    if let Some(p) = to {
        left.push(format!("link #{p} and #{n}"));
    }
    if let Some(f) = from {
        left.push(format!("unlink #{f} and #{n}"));
    }
    tracker.set_desc(n, &body_moved(&item.desc, to))?;
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
    use crate::item::{Item, State};
    use crate::tracker::FakeTracker;

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
            true,
            Some(1),
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
        new_item(&t, true, Some(1), "A child", "x", &[]).unwrap();
        let found = crate::check::check(
            &t.all_items().unwrap(),
            &ParentLine,
            &crate::check::RootsOnly,
            crate::check::Links::TreeOnly,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn without_a_parent_it_is_a_root_with_no_link() {
        let t = tracker();
        let n = new_item(&t, true, None, "Alone", "text", &[]).unwrap();
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
            new_item(&t, false, Some(1), "x", "y", &[]),
            Err(Error::Refused(_))
        ));
        assert!(t.writes.borrow().is_empty() && t.levels.borrow().is_empty());
    }

    #[test]
    fn a_parent_that_does_not_exist_is_refused_before_writing() {
        let t = tracker();
        assert!(matches!(
            new_item(&t, true, Some(99), "x", "y", &[]),
            Err(Error::NotFound(99))
        ));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_body_that_already_opens_with_a_parent_line_is_refused() {
        let t = tracker();
        let r = new_item(&t, true, Some(1), "x", "Parent: #1\n\ny", &[]);
        assert!(matches!(r, Err(Error::Refused(m)) if m.contains("Parent")));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failed_link_names_the_item_and_what_is_left_to_do() {
        let t = tracker();
        t.fail_links.set(true);
        let e = new_item(&t, true, Some(1), "x", "y", &[]).unwrap_err();
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
        )
    }

    #[test]
    fn a_child_moves_to_a_new_parent_body_first_then_new_link_then_old_unlinked() {
        let t = tree3();
        let m = move_item(&t, true, 2, Some(4)).unwrap();
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
        move_item(&t, true, 4, Some(3)).unwrap();
        assert_eq!(t.item(4).unwrap().desc, "Parent: #3\n\nlone");
        assert_eq!(*t.writes.borrow(), ["desc #4", "link #3 #4"]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn to_root_drops_the_line_and_its_blank_line_and_the_old_link_only() {
        let t = tree3();
        let m = move_item(&t, true, 2, None).unwrap();
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
        let made = new_item(&t, true, Some(1), "x", text, &[]).unwrap();
        move_item(&t, true, made.created.iid, None).unwrap();
        assert_eq!(t.item(made.created.iid).unwrap().desc, text);
    }

    #[test]
    fn a_crlf_first_line_keeps_its_line_ending() {
        assert_eq!(
            body_moved("Parent: #1\r\n\r\nx", Some(4)),
            "Parent: #4\r\n\r\nx"
        );
        assert_eq!(body_moved("Parent: #1\r\n\r\nx", None), "x");
        assert_eq!(body_moved("Parent: #1", None), "");
        assert_eq!(body_moved("Parent: #1\nx", None), "x");
    }

    #[test]
    fn a_parent_that_is_the_item_or_below_it_is_refused_before_any_write() {
        let t = tree3();
        assert!(
            matches!(move_item(&t, true, 1, Some(1)), Err(Error::Refused(m)) if m.contains("cycle"))
        );
        assert!(
            matches!(move_item(&t, true, 1, Some(3)), Err(Error::Refused(m)) if m.contains("cycle"))
        );
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_rerun_changes_nothing() {
        let t = tree3();
        let m = move_item(&t, true, 2, Some(1)).unwrap();
        assert!(!m.changed);
        let m = move_item(&t, true, 4, None).unwrap();
        assert!(!m.changed);
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_missing_item_or_parent_and_a_repo_without_config_are_refused_before_any_write() {
        let t = tree3();
        assert!(matches!(
            move_item(&t, true, 99, Some(1)),
            Err(Error::NotFound(99))
        ));
        assert!(matches!(
            move_item(&t, true, 2, Some(99)),
            Err(Error::NotFound(99))
        ));
        assert!(matches!(
            move_item(&t, false, 2, Some(4)),
            Err(Error::Refused(_))
        ));
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn a_failure_after_the_body_was_written_says_what_is_left() {
        let t = tree3();
        t.fail_links.set(true);
        let e = move_item(&t, true, 2, Some(4)).unwrap_err();
        let m = e.to_string();
        assert!(matches!(e, Error::Unfinished(_)), "{m}");
        assert!(m.contains("link #4 and #2") && m.contains("#1"), "{m}");
    }

    #[test]
    fn when_only_the_old_link_failed_only_the_unlink_is_left() {
        let t = tree3();
        t.fail_unlinks.set(true);
        let m = move_item(&t, true, 2, Some(4)).unwrap_err().to_string();
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
        };
        assert_eq!(
            new_json(&n),
            json!({ "version": 1, "created": { "number": 2, "url": "u" }, "link": { "parent": 1, "child": 2 } })
        );
        let root = New {
            created: Created {
                iid: 3,
                url: "v".into(),
            },
            link: None,
        };
        assert_eq!(new_json(&root)["link"], Value::Null);
    }
}
