//! Writing the tree: a new item, a move and an edit, each with its header, its links and the
//! lists derived on the other ends, in one step.

use crate::check::{Family, StatePolicy};
use crate::derived;
use crate::header::{self, Header};
use crate::item::{Item, State};
use crate::scheme::FrontMatter;
use crate::tracker::{Created, Error, Tracker};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};

/// The version of the JSON shapes `moved_json` and `edited_json` print.
pub const JSON_VERSION: u32 = 1;
/// The version of the JSON shape `new_json` prints.
pub const NEW_JSON_VERSION: u32 = 2;

/// What a write must respect: the repo's opt-in, its state policy and its declared extra keys.
pub struct Rules<'a> {
    pub opted_in: bool,
    pub policy: &'a dyn StatePolicy,
    pub extra_keys: &'a [String],
}

/// The writes that follow the first one, in order, each named for the message a failure gives.
type Steps<'a> = Vec<(String, Box<dyn Fn() -> Result<(), Error> + 'a>)>;

/// Runs `steps` in order; a failure names itself and every step after it, after `done`, which
/// says what the first write already did.
fn run(steps: Steps, done: &str) -> Result<(), Error> {
    for (i, (_, step)) in steps.iter().enumerate() {
        if let Err(e) = step() {
            let left: Vec<&str> = steps[i..].iter().map(|(name, _)| name.as_str()).collect();
            return Err(Error::Unfinished(format!(
                "{done}, but a later step failed ({e}); left to do: {}",
                left.join(", then ")
            )));
        }
    }
    Ok(())
}

fn link<'a>(
    tracker: &'a dyn Tracker,
    a: u64,
    b: u64,
) -> (String, Box<dyn Fn() -> Result<(), Error> + 'a>) {
    (
        format!("link #{a} and #{b}"),
        Box::new(move || tracker.add_link(a, b)),
    )
}

fn sync<'a>(
    tracker: &'a dyn Tracker,
    extra_keys: &'a [String],
    m: u64,
) -> (String, Box<dyn Fn() -> Result<(), Error> + 'a>) {
    (
        format!("gltree sync {m}"),
        Box::new(move || derived::sync_one(tracker, extra_keys, m, false).map(|_| ())),
    )
}

/// Unlinks `a` and `b` unless a reference on either end still explains the link.
fn unlink_unexplained<'a>(
    tracker: &'a dyn Tracker,
    a: u64,
    b: u64,
) -> (String, Box<dyn Fn() -> Result<(), Error> + 'a>) {
    (
        format!("unlink #{a} and #{b}"),
        Box::new(move || {
            let both = tracker.level(&[a, b])?;
            let names = |x: u64, y: u64| {
                both.get(&x)
                    .is_some_and(|l| header::refs_of(&l.item.desc).iter().any(|(_, n)| *n == y))
            };
            if names(a, b) || names(b, a) {
                return Ok(());
            }
            tracker.remove_link(a, b)
        }),
    )
}

/// Ascending, each number once.
fn sorted(ns: &[u64]) -> Vec<u64> {
    ns.iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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

/// Refuses every number in `wanted` the tracker does not hold, in one read.
fn require_all(tracker: &dyn Tracker, wanted: &[u64]) -> Result<(), Error> {
    if wanted.is_empty() {
        return Ok(());
    }
    let found = tracker.level(wanted)?;
    match wanted.iter().find(|n| !found.contains_key(n)) {
        Some(&n) => Err(Error::NotFound(n)),
        None => Ok(()),
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

/// The references a new item starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refs {
    pub parent: Option<u64>,
    pub from: Option<u64>,
    pub related: Vec<u64>,
    pub blocked_by: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct New {
    pub created: Created,
    /// `(parent, child)` when the item has a parent.
    pub link: Option<(u64, u64)>,
    /// The item it was found while working, when one was given.
    pub from: Option<u64>,
    pub related: Vec<u64>,
    pub blocked_by: Vec<u64>,
}

/// Creates an item with `refs` in its header above `text`, links each reference, then syncs the
/// lists derived on the parent, the blockers and the related items. Refuses before any write when
/// the repo has not opted in, a referenced item does not exist, `from` is `parent`, `text` already
/// opens with a header, or `parent` is closed where the policy would then report it.
pub fn new_item(
    tracker: &dyn Tracker,
    rules: &Rules,
    refs: &Refs,
    title: &str,
    text: &str,
    labels: &[String],
) -> Result<New, Error> {
    require_opt_in(rules.opted_in)?;
    if text.starts_with(header::FENCE) {
        return Err(Error::Refused(format!(
            "the body already opens with `{}`; drop the header, gltree writes it from the flags",
            header::FENCE
        )));
    }
    let (parent, from) = (refs.parent, refs.from);
    if from.is_some() && from == parent {
        return Err(Error::Refused(
            "--from names the parent; an item found while working its own parent is part of it, so give --parent alone".into(),
        ));
    }
    let (related, blocked_by) = (sorted(&refs.related), sorted(&refs.blocked_by));
    let wanted: Vec<u64> = parent
        .into_iter()
        .chain(from)
        .chain(related.iter().copied())
        .chain(blocked_by.iter().copied())
        .collect();
    require_all(tracker, &wanted)?;
    if let Some(p) = parent {
        let new = Item::new(0, State::Open, title, "");
        let alternative = format!("give the new item --from {p} instead of --parent");
        refuse_closed_above(tracker, rules.policy, p, &new, &[&new], &alternative)?;
    }
    let h = Header {
        parent,
        from,
        related: related.clone(),
        blocked_by: blocked_by.clone(),
        ..Header::default()
    };
    let body = header::with_header(text, &h).map_err(Error::Refused)?;
    let created = tracker.create_item(title, &body, labels)?;
    let c = created.iid;
    let linked: BTreeSet<u64> = wanted.iter().copied().collect();
    let mut ordered: Vec<u64> = parent.into_iter().chain(from).collect();
    ordered.extend(
        linked
            .iter()
            .filter(|n| Some(**n) != parent && Some(**n) != from),
    );
    let synced: Vec<u64> = parent
        .into_iter()
        .chain(blocked_by.iter().copied())
        .chain(related.iter().copied())
        .collect::<Vec<_>>();
    let mut steps: Steps = ordered.iter().map(|&n| link(tracker, n, c)).collect();
    let mut seen = HashSet::new();
    for m in synced.into_iter().filter(|m| seen.insert(*m)) {
        steps.push(sync(tracker, rules.extra_keys, m));
    }
    run(steps, &format!("created #{c} ({})", created.url))?;
    Ok(New {
        created,
        link: parent.map(|p| (p, c)),
        from,
        related,
        blocked_by,
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
        "related": n.related,
        "blocked_by": n.blocked_by,
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

/// Reads `n` and its header, refusing a header out of form.
fn read(tracker: &dyn Tracker, extra_keys: &[String], n: u64) -> Result<(Item, Header), Error> {
    let item = tracker
        .level(&[n])?
        .remove(&n)
        .ok_or(Error::NotFound(n))?
        .item;
    let h = header::parse(&item.desc, extra_keys)
        .map_err(|e| Error::Refused(format!("#{n}'s header is out of form ({e}); fix it first")))?
        .header
        .unwrap_or_default();
    Ok((item, h))
}

/// Gives `n` the parent `to`, or makes it a root for `None`: the body first, then the new link,
/// the old link removed unless another reference still explains it, then `children` synced on
/// both parents. Only the header's `parent` changes. Refuses before any write a repo that has not
/// opted in, a missing item or parent, a header out of form, a parent that is `n`'s `from`, `n`
/// or below it, and a closed parent the policy would then report.
pub fn move_item(
    tracker: &dyn Tracker,
    rules: &Rules,
    n: u64,
    to: Option<u64>,
) -> Result<Moved, Error> {
    require_opt_in(rules.opted_in)?;
    let (item, mut h) = read(tracker, rules.extra_keys, n)?;
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
        require_all(tracker, &[p])?;
        let below: Vec<&Item> = below.iter().collect();
        let alternative = format!("record #{p} as #{n}'s `from` instead");
        refuse_closed_above(tracker, rules.policy, p, &item, &below, &alternative)?;
    }
    h.parent = to;
    h.drop_orphan_notes();
    let body = header::with_header(&item.desc, &h).map_err(Error::Refused)?;
    tracker.set_desc(n, &body)?;
    let mut steps: Steps = Vec::new();
    if let Some(p) = to {
        steps.push(link(tracker, p, n));
    }
    if let Some(f) = from {
        steps.push(unlink_unexplained(tracker, f, n));
    }
    for m in from.into_iter().chain(to) {
        steps.push(sync(tracker, rules.extra_keys, m));
    }
    let names = to.map_or("no parent".into(), |p| format!("#{p} as its parent"));
    run(steps, &format!("#{n}'s header now names {names}"))?;
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

/// What an edit adds to and removes from an item's `related` and `blocked_by`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Edits {
    pub add_related: Vec<u64>,
    pub remove_related: Vec<u64>,
    pub add_blocked_by: Vec<u64>,
    pub remove_blocked_by: Vec<u64>,
    /// `[extra]` values to set, each under a key the repo config declares.
    pub set_extra: Vec<(String, String)>,
}

/// An edit: the item, its `related` and `blocked_by` after, and whether anything was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edited {
    pub iid: u64,
    pub related: Vec<u64>,
    pub blocked_by: Vec<u64>,
    pub changed: bool,
}

/// Whether `n` is reached by following `blocked_by` from `start`, one `level` per step.
fn blocked_through(tracker: &dyn Tracker, start: u64, n: u64) -> Result<bool, Error> {
    let (mut seen, mut frontier) = (HashSet::from([start]), vec![start]);
    while !frontier.is_empty() {
        let found = tracker.level(&frontier)?;
        let mut next = Vec::new();
        for l in found.values() {
            for (k, b) in header::refs_of(&l.item.desc) {
                if k != "blocked_by" {
                    continue;
                }
                if b == n {
                    return Ok(true);
                }
                if seen.insert(b) {
                    next.push(b);
                }
            }
        }
        frontier = next;
    }
    Ok(false)
}

/// Changes `n`'s `related` and `blocked_by`: the body first, then for each removal the mirror
/// dropped or the blocker synced and the link removed unless another reference explains it, then
/// for each addition the link and the other end synced. Refuses before any write a repo that has
/// not opted in, a header out of form, the item itself, a missing item to add, and a blocker that
/// is blocked, at any depth, by `n`.
pub fn edit_item(
    tracker: &dyn Tracker,
    rules: &Rules,
    n: u64,
    edits: &Edits,
) -> Result<Edited, Error> {
    require_opt_in(rules.opted_in)?;
    let (item, mut h) = read(tracker, rules.extra_keys, n)?;
    let all = [
        &edits.add_related,
        &edits.remove_related,
        &edits.add_blocked_by,
        &edits.remove_blocked_by,
    ];
    if let Some((k, _)) = edits
        .set_extra
        .iter()
        .find(|(k, _)| !rules.extra_keys.contains(k))
    {
        return Err(Error::Refused(format!(
            "`{k}` is not an extra key the repo config declares"
        )));
    }
    if all.iter().any(|ns| ns.contains(&n)) {
        return Err(Error::Refused(format!("#{n} cannot name itself")));
    }
    let added: Vec<u64> = edits
        .add_related
        .iter()
        .chain(&edits.add_blocked_by)
        .copied()
        .collect();
    require_all(tracker, &added)?;
    for &b in sorted(&edits.add_blocked_by).iter() {
        if blocked_through(tracker, b, n)? {
            return Err(Error::Refused(format!(
                "#{b} is blocked by #{n}, at some depth; #{n} blocked by #{b} would make a cycle"
            )));
        }
    }
    let apply = |now: &[u64], add: &[u64], remove: &[u64]| {
        let set: BTreeSet<u64> = now
            .iter()
            .chain(add)
            .copied()
            .filter(|x| !remove.contains(x))
            .collect();
        set.into_iter().collect::<Vec<u64>>()
    };
    let related = apply(&h.related, &edits.add_related, &edits.remove_related);
    let blocked_by = apply(
        &h.blocked_by,
        &edits.add_blocked_by,
        &edits.remove_blocked_by,
    );
    let gone_related: Vec<u64> = h
        .related
        .iter()
        .copied()
        .filter(|x| !related.contains(x))
        .collect();
    let new_related: Vec<u64> = related
        .iter()
        .copied()
        .filter(|x| !h.related.contains(x))
        .collect();
    let gone_blocked: Vec<u64> = h
        .blocked_by
        .iter()
        .copied()
        .filter(|x| !blocked_by.contains(x))
        .collect();
    let new_blocked: Vec<u64> = blocked_by
        .iter()
        .copied()
        .filter(|x| !h.blocked_by.contains(x))
        .collect();
    // a mirror removed on the other end alone would come back on the next sync
    let mirror_only: Vec<u64> = sorted(&edits.remove_related)
        .into_iter()
        .filter(|x| !h.related.contains(x))
        .collect();
    let extra_before = h.extra.clone();
    for (k, v) in &edits.set_extra {
        h.extra.insert(k.clone(), toml::Value::String(v.clone()));
    }
    let changed = !(gone_related.is_empty()
        && new_related.is_empty()
        && gone_blocked.is_empty()
        && new_blocked.is_empty()
        && mirror_only.is_empty()
        && h.extra == extra_before);
    if !changed {
        return Ok(Edited {
            iid: n,
            related,
            blocked_by,
            changed: false,
        });
    }
    h.related = related.clone();
    h.blocked_by = blocked_by.clone();
    h.drop_orphan_notes();
    let body = header::with_header(&item.desc, &h).map_err(Error::Refused)?;
    tracker.set_desc(n, &body)?;
    let extra = rules.extra_keys;
    let mut steps: Steps = Vec::new();
    for m in gone_related.iter().chain(&mirror_only).copied() {
        steps.push((
            format!("drop #{n} from #{m}'s related"),
            Box::new(move || {
                let (it, mut mh) = read(tracker, extra, m)?;
                if !mh.related.contains(&n) {
                    return Ok(());
                }
                mh.related.retain(|x| *x != n);
                mh.drop_orphan_notes();
                let body = header::with_header(&it.desc, &mh).map_err(Error::Refused)?;
                tracker.set_desc(m, &body)
            }),
        ));
        steps.push(unlink_unexplained(tracker, m, n));
    }
    for &m in &gone_blocked {
        steps.push(sync(tracker, extra, m));
        steps.push(unlink_unexplained(tracker, m, n));
    }
    for &m in new_related.iter().chain(&new_blocked) {
        steps.push(link(tracker, m, n));
        steps.push(sync(tracker, extra, m));
    }
    run(steps, &format!("#{n}'s header is written"))?;
    Ok(Edited {
        iid: n,
        related,
        blocked_by,
        changed: true,
    })
}

pub fn edited_json(e: &Edited) -> Value {
    json!({ "version": JSON_VERSION, "item": e.iid, "related": e.related, "blocked_by": e.blocked_by, "changed": e.changed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::FakeTracker;

    /// A body whose header names `n` as the parent.
    fn p(n: u64) -> String {
        format!("+++\nparent = {n}\n+++\n")
    }

    fn refs(parent: Option<u64>, from: Option<u64>) -> Refs {
        Refs {
            parent,
            from,
            ..Refs::default()
        }
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
            &refs(Some(1), None),
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
                "link #1 #2".to_string(),
                "desc #1".to_string()
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
        let n = new_item(&t, &rules(true), &refs(None, None), "Alone", "text", &[]).unwrap();
        assert_eq!(n.link, None);
        assert_eq!(t.item(2).unwrap().desc, "text");
        assert_eq!(t.writes.borrow().len(), 1);
    }

    #[test]
    fn an_empty_body_is_the_header_alone() {
        let t = tracker();
        new_item(&t, &rules(true), &refs(Some(1), None), "x", "", &[]).unwrap();
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
            new_item(&t, &rules(false), &refs(Some(1), None), "x", "y", &[]),
            Err(Error::Refused(_))
        ));
        assert!(t.writes.borrow().is_empty() && t.levels.borrow().is_empty());
    }

    #[test]
    fn a_parent_that_does_not_exist_is_refused_before_writing() {
        let t = tracker();
        assert!(matches!(
            new_item(&t, &rules(true), &refs(Some(99), None), "x", "y", &[]),
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
            &refs(Some(1), None),
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
        let e = new_item(&t, &rules(true), &refs(Some(1), None), "x", "y", &[]).unwrap_err();
        let m = e.to_string();
        assert!(matches!(e, Error::Unfinished(_)), "{m}");
        assert!(
            m.ends_with("left to do: link #1 and #2, then gltree sync 1"),
            "{m}"
        );
    }

    fn tree3() -> FakeTracker {
        // 1 ─ 2 ─ 3, and 4 a root related to 2; every reference linked, every derived list in step
        let mut items = vec![
            Item::new(1, State::Open, "t1", "+++\nchildren = [2]\n+++\n"),
            Item::new(
                2,
                State::Open,
                "t2",
                "+++\nparent = 1\nchildren = [3]\nrelated = [4]\n\n[notes]\nrelated = \"same screen\"\n+++\n\nWaits on the store.\nbody",
            ),
            Item::new(3, State::Open, "t3", &p(2)),
            Item::new(4, State::Open, "t4", "+++\nrelated = [2]\n+++\n\nlone"),
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
        assert_eq!(
            writes,
            [
                "desc #2",
                "link #4 #2",
                "unlink #1 #2",
                "desc #1",
                "desc #4"
            ]
        );
        assert_eq!(t.item(1).unwrap().desc, "");
        assert_eq!(
            t.item(2).unwrap().desc,
            "+++\nparent = 4\nchildren = [3]\nrelated = [4]\n\n[notes]\nrelated = \"same screen\"\n+++\n\nWaits on the store.\nbody"
        );
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_root_moves_under_a_parent_with_the_header_new_writes() {
        let t = tree3();
        move_item(&t, &rules(true), 4, Some(3)).unwrap();
        assert_eq!(
            t.item(4).unwrap().desc,
            "+++\nparent = 3\nrelated = [2]\n+++\n\nlone"
        );
        assert_eq!(*t.writes.borrow(), ["desc #4", "link #3 #4", "desc #3"]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn to_root_keeps_the_rest_of_the_header_and_drops_a_header_left_empty() {
        let t = tree3();
        let m = move_item(&t, &rules(true), 2, None).unwrap();
        assert_eq!((m.from, m.to), (Some(1), None));
        assert_eq!(
            t.item(2).unwrap().desc,
            "+++\nchildren = [3]\nrelated = [4]\n\n[notes]\nrelated = \"same screen\"\n+++\n\nWaits on the store.\nbody"
        );
        assert_eq!(*t.writes.borrow(), ["desc #2", "unlink #1 #2", "desc #1"]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
        move_item(&t, &rules(true), 3, None).unwrap();
        assert_eq!(t.item(3).unwrap().desc, "");
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_note_goes_with_the_parent_it_explained() {
        let t = tree3();
        t.items.borrow_mut()[2].desc =
            "+++\nparent = 2\n\n[notes]\nparent = \"the store's slice\"\n+++\n".into();
        move_item(&t, &rules(true), 3, None).unwrap();
        assert_eq!(t.item(3).unwrap().desc, "");
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn new_then_to_root_gives_back_the_text() {
        let t = tree3();
        let text = "first\r\n\r\nsecond";
        let made = new_item(&t, &rules(true), &refs(Some(1), None), "x", text, &[]).unwrap();
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
        let n = new_item(
            &t,
            &rules(true),
            &refs(Some(4), Some(2)),
            "found",
            "body",
            &[],
        )
        .unwrap();
        assert_eq!((n.link, n.from), (Some((4, 5)), Some(2)));
        assert_eq!(
            t.item(5).unwrap().desc,
            "+++\nparent = 4\nfrom = 2\n+++\n\nbody"
        );
        assert_eq!(
            t.writes.borrow()[1..],
            [
                "link #4 #5".to_string(),
                "link #2 #5".to_string(),
                "desc #4".to_string()
            ]
        );
        let r = new_item(&t, &rules(true), &refs(None, Some(3)), "alone", "", &[]).unwrap();
        assert_eq!(t.item(r.created.iid).unwrap().desc, "+++\nfrom = 3\n+++\n");
    }

    #[test]
    fn from_equal_to_parent_or_missing_is_refused_before_any_write() {
        let t = tree3();
        assert!(matches!(
            new_item(&t, &rules(true), &refs(Some(2), Some(2)), "x", "y", &[]),
            Err(Error::Refused(m)) if m.contains("--parent alone")
        ));
        assert!(matches!(
            new_item(&t, &rules(true), &refs(None, Some(99)), "x", "y", &[]),
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
        let m = new_item(&t, &rules(true), &refs(Some(4), Some(2)), "x", "y", &[])
            .unwrap_err()
            .to_string();
        assert!(
            m.ends_with("left to do: link #4 and #5, then link #2 and #5, then gltree sync 4"),
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
        let m = new_item(&t, &every_parent(), &refs(Some(3), None), "x", "y", &[])
            .unwrap_err()
            .to_string();
        assert!(m.contains("#3 is closed") && m.contains("--from 3"), "{m}");
        let m = move_item(&t, &every_parent(), 4, Some(3))
            .unwrap_err()
            .to_string();
        assert!(m.contains("#3 is closed") && m.contains("`from`"), "{m}");
        assert!(t.writes.borrow().is_empty());
        // roots-only lets a closed slice take a child, but not when its root is closed too
        assert!(new_item(&t, &rules(true), &refs(Some(3), None), "x", "y", &[]).is_ok());
        closed(&t, 1);
        let m = new_item(&t, &rules(true), &refs(Some(3), None), "x", "y", &[])
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
            m.ends_with(
                "left to do: link #4 and #2, then unlink #1 and #2, then gltree sync 1, then gltree sync 4"
            ),
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
        assert!(
            m.ends_with("left to do: unlink #1 and #2, then gltree sync 1, then gltree sync 4"),
            "{m}"
        );
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
            related: vec![],
            blocked_by: vec![4],
        };
        assert_eq!(
            new_json(&n),
            json!({ "version": 2, "created": { "number": 2, "url": "u" }, "link": { "parent": 1, "child": 2 }, "from": null, "related": [], "blocked_by": [4] })
        );
        let root = New {
            created: Created {
                iid: 3,
                url: "v".into(),
            },
            link: None,
            from: Some(9),
            related: vec![5],
            blocked_by: vec![],
        };
        assert_eq!(new_json(&root)["link"], Value::Null);
        assert_eq!(new_json(&root)["from"], 9);
        assert_eq!(new_json(&root)["related"], json!([5]));
    }

    #[test]
    fn json_reports_an_edit() {
        let e = Edited {
            iid: 2,
            related: vec![4],
            blocked_by: vec![],
            changed: true,
        };
        assert_eq!(
            edited_json(&e),
            json!({ "version": 1, "item": 2, "related": [4], "blocked_by": [], "changed": true })
        );
    }

    fn all(n: u64) -> Edits {
        Edits {
            add_related: vec![n],
            ..Edits::default()
        }
    }

    fn links_of(t: &FakeTracker, n: u64) -> Vec<u64> {
        let mut l = t.item(n).unwrap().links.unwrap_or_default();
        l.sort();
        l
    }

    #[test]
    fn new_with_related_and_blocked_by_writes_every_end() {
        let t = tree3();
        let r = Refs {
            parent: Some(1),
            related: vec![4],
            blocked_by: vec![3],
            ..Refs::default()
        };
        let n = new_item(&t, &rules(true), &r, "x", "body", &[]).unwrap();
        assert_eq!((n.related, n.blocked_by), (vec![4], vec![3]));
        assert_eq!(
            t.item(5).unwrap().desc,
            "+++\nparent = 1\nrelated = [4]\nblocked_by = [3]\n+++\n\nbody"
        );
        assert_eq!(
            t.writes.borrow()[1..],
            [
                "link #1 #5",
                "link #3 #5",
                "link #4 #5",
                "desc #1",
                "desc #3",
                "desc #4"
            ]
        );
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_failed_sync_after_new_names_the_syncs_left() {
        let t = tree3();
        t.fail_descs.set(Some(0));
        let r = Refs {
            parent: Some(1),
            blocked_by: vec![3],
            ..Refs::default()
        };
        let m = new_item(&t, &rules(true), &r, "x", "y", &[])
            .unwrap_err()
            .to_string();
        assert!(
            m.ends_with("left to do: gltree sync 1, then gltree sync 3"),
            "{m}"
        );
    }

    #[test]
    fn edit_adds_a_related_item_with_its_link_and_mirror() {
        let t = tree3();
        let e = edit_item(&t, &rules(true), 1, &all(3)).unwrap();
        assert_eq!((e.related, e.changed), (vec![3], true));
        assert_eq!(*t.writes.borrow(), ["desc #1", "link #3 #1", "desc #3"]);
        assert!(t.item(3).unwrap().desc.contains("related = [1]"));
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn edit_removes_a_related_item_on_both_ends_and_its_link() {
        let t = tree3();
        let rm = Edits {
            remove_related: vec![4],
            ..Edits::default()
        };
        edit_item(&t, &rules(true), 2, &rm).unwrap();
        assert_eq!(*t.writes.borrow(), ["desc #2", "desc #4", "unlink #4 #2"]);
        assert!(
            !t.item(2).unwrap().desc.contains("same screen"),
            "the note goes with its key"
        );
        assert_eq!(t.item(4).unwrap().desc, "lone");
        assert_eq!(links_of(&t, 4), Vec::<u64>::new());
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_link_another_reference_explains_is_kept() {
        let t = tree3();
        move_item(&t, &rules(true), 2, Some(4)).unwrap();
        let rm = Edits {
            remove_related: vec![4],
            ..Edits::default()
        };
        edit_item(&t, &rules(true), 2, &rm).unwrap();
        assert_eq!(links_of(&t, 4), vec![2]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn a_mirror_alone_is_removed_from_the_other_end() {
        let t = tree3();
        t.items.borrow_mut()[3].desc = "lone".into();
        let rm = Edits {
            remove_related: vec![2],
            ..Edits::default()
        };
        let e = edit_item(&t, &rules(true), 4, &rm).unwrap();
        assert!(e.changed);
        assert!(!t.item(2).unwrap().desc.contains("related"));
        assert_eq!(links_of(&t, 4), Vec::<u64>::new());
    }

    #[test]
    fn edit_adds_and_removes_a_blocker_with_its_blocks() {
        let t = tree3();
        let add = Edits {
            add_blocked_by: vec![3],
            ..Edits::default()
        };
        edit_item(&t, &rules(true), 4, &add).unwrap();
        assert!(t.item(3).unwrap().desc.contains("blocks = [4]"));
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
        let rm = Edits {
            remove_blocked_by: vec![3],
            ..Edits::default()
        };
        edit_item(&t, &rules(true), 4, &rm).unwrap();
        assert!(!t.item(3).unwrap().desc.contains("blocks"));
        assert_eq!(links_of(&t, 3), vec![2]);
        assert!(clean(&t).is_empty(), "{:?}", clean(&t));
    }

    #[test]
    fn edit_sets_a_declared_extra_value_and_refuses_an_undeclared_one() {
        let t = tree3();
        let declared = ["marker".to_string()];
        let r = Rules {
            extra_keys: &declared,
            ..rules(true)
        };
        let set = Edits {
            set_extra: vec![("marker".into(), "K-1 (등록 2026-10-22)".into())],
            ..Edits::default()
        };
        assert!(edit_item(&t, &r, 4, &set).unwrap().changed);
        assert_eq!(
            t.item(4).unwrap().desc,
            "+++\nrelated = [2]\n\n[extra]\nmarker = \"K-1 (등록 2026-10-22)\"\n+++\n\nlone"
        );
        let writes = t.writes.borrow().len();
        assert!(!edit_item(&t, &r, 4, &set).unwrap().changed);
        let other = Edits {
            set_extra: vec![("other".into(), "x".into())],
            ..Edits::default()
        };
        assert!(matches!(
            edit_item(&t, &r, 4, &other),
            Err(Error::Refused(m)) if m.contains("`other`")
        ));
        assert_eq!(t.writes.borrow().len(), writes);
        let found = crate::check::check(
            &t.all_items().unwrap(),
            &FrontMatter,
            &crate::check::RootsOnly,
            &declared,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_edit_that_changes_nothing_writes_nothing() {
        let t = tree3();
        let e = edit_item(&t, &rules(true), 2, &all(4)).unwrap();
        assert!(!e.changed);
        assert!(t.writes.borrow().is_empty());
    }

    #[test]
    fn edit_refuses_itself_a_missing_item_a_blocker_cycle_and_a_repo_without_config() {
        let t = tree3();
        let add = Edits {
            add_blocked_by: vec![3],
            ..Edits::default()
        };
        edit_item(&t, &rules(true), 4, &add).unwrap();
        let writes = t.writes.borrow().len();
        assert!(matches!(
            edit_item(&t, &rules(true), 2, &all(2)),
            Err(Error::Refused(m)) if m.contains("itself")
        ));
        assert!(matches!(
            edit_item(&t, &rules(true), 2, &all(99)),
            Err(Error::NotFound(99))
        ));
        let back = Edits {
            add_blocked_by: vec![4],
            ..Edits::default()
        };
        assert!(matches!(
            edit_item(&t, &rules(true), 3, &back),
            Err(Error::Refused(m)) if m.contains("cycle")
        ));
        assert!(matches!(
            edit_item(&t, &rules(false), 2, &all(1)),
            Err(Error::Refused(_))
        ));
        assert_eq!(t.writes.borrow().len(), writes);
    }

    #[test]
    fn a_failure_after_the_edit_was_written_names_the_steps_left() {
        let add = Edits {
            add_blocked_by: vec![3],
            ..Edits::default()
        };
        let t = tree3();
        t.fail_links.set(true);
        let m = edit_item(&t, &rules(true), 4, &add)
            .unwrap_err()
            .to_string();
        assert!(
            m.ends_with("left to do: link #3 and #4, then gltree sync 3"),
            "{m}"
        );
        let t = tree3();
        t.fail_descs.set(Some(1));
        let m = edit_item(&t, &rules(true), 4, &add)
            .unwrap_err()
            .to_string();
        assert!(m.ends_with("left to do: gltree sync 3"), "{m}");
    }
}
