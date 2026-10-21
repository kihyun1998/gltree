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
