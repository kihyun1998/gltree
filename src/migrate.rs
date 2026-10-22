//! Turning a tracker written in the first-line format into headers (ADR-0004): the old head's
//! key lines become the header, the lists derived on the other ends are filled, and every
//! reference gets its link. The only reader of the old format left.

use crate::derived;
use crate::header::{self, Header};
use crate::item::Item;
use crate::tracker::{Error, Tracker};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The version of the JSON shape `to_json` prints.
pub const JSON_VERSION: u32 = 1;

/// The words a repo's old head used for `from` and `related`; `Parent`, `Blocked by` and
/// `Depends on` are the format's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    pub from: Vec<String>,
    pub related: Vec<String>,
}

impl Default for Words {
    fn default() -> Self {
        Words {
            from: vec!["From".into()],
            related: vec!["Related".into()],
        }
    }
}

/// One item the migration changes: the old head's lines, the description before and after,
/// and what moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub iid: u64,
    pub old_head: Vec<String>,
    pub before: String,
    pub after: String,
    pub kinds: Vec<&'static str>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub changes: Vec<Change>,
    /// Links the references need and the tracker lacks, each pair once, lower number first.
    pub links: Vec<(u64, u64)>,
}

/// The issue numbers a value opens with, and whether anything else follows them.
fn numbers(value: &str) -> Option<(Vec<u64>, bool)> {
    let mut ns = Vec::new();
    let mut rest = value.trim();
    while let Some(r) = rest.strip_prefix('#') {
        let d = r.chars().take_while(char::is_ascii_digit).count();
        if d == 0 {
            break;
        }
        ns.push(r[..d].parse().ok()?);
        rest = r[d..].trim_start();
        match rest.strip_prefix(',') {
            Some(r) if r.trim_start().starts_with('#') => rest = r.trim_start(),
            _ => break,
        }
    }
    (!ns.is_empty()).then(|| (ns, !rest.trim().is_empty()))
}

/// What one head line holds, if it is a line the old format had.
enum Line {
    Ref(&'static str, Vec<u64>, Option<String>),
    Extra(String, String),
}

fn read_line(line: &str, words: &Words, extra_keys: &[String]) -> Option<Line> {
    let (key, value) = line.split_once(": ")?;
    let extra = key.to_lowercase().replace(' ', "_");
    if extra_keys.contains(&extra) {
        return Some(Line::Extra(extra, value.trim().to_string()));
    }
    let name = match key {
        "Parent" => "parent",
        "Blocked by" | "Depends on" => "blocked_by",
        k if words.from.iter().any(|w| w == k) => "from",
        k if words.related.iter().any(|w| w == k) => "related",
        _ => return None,
    };
    let (ns, more) = numbers(value)?;
    if name == "parent" && (more || ns.len() > 1) {
        return None;
    }
    if name == "from" && ns.len() > 1 {
        return None;
    }
    let note = more.then(|| value.trim().to_string());
    Some(Line::Ref(name, ns, note))
}

/// The header an old head gives, and the byte where the body after it starts; `None` for a
/// description whose first line is not a head line — a header's opening `+++` included.
pub fn read_head<'a>(
    desc: &'a str,
    words: &Words,
    extra_keys: &[String],
) -> Option<(Header, Vec<String>, &'a str)> {
    let (mut h, mut lines, mut end, mut at) = (Header::default(), Vec::new(), None, 0);
    let mut lists: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
    for raw in desc.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\r', '\n']);
        at += raw.len();
        if line.trim().is_empty() {
            continue;
        }
        let Some(read) = read_line(line, words, extra_keys) else {
            break;
        };
        match read {
            Line::Ref(key, ns, note) => {
                match key {
                    "parent" if h.parent.is_none() => h.parent = Some(ns[0]),
                    "from" if h.from.is_none() => h.from = Some(ns[0]),
                    "related" | "blocked_by" => lists.entry(key).or_default().extend(ns),
                    _ => {}
                }
                if let Some(n) = note {
                    let slot = h.notes.entry(key.to_string()).or_default();
                    if !slot.is_empty() {
                        slot.push_str("; ");
                    }
                    slot.push_str(&n);
                }
            }
            Line::Extra(key, value) => {
                h.extra.insert(key, toml::Value::String(value));
            }
        }
        lines.push(line.to_string());
        end = Some(at);
    }
    let end = end?;
    h.related = lists
        .remove("related")
        .map(|s| s.into_iter().collect())
        .unwrap_or_default();
    h.blocked_by = lists
        .remove("blocked_by")
        .map(|s| s.into_iter().collect())
        .unwrap_or_default();
    if h.from.is_some() && h.from == h.parent {
        if let Some(n) = h.notes.remove("from") {
            h.notes.entry("parent".into()).or_insert(n);
        }
        h.from = None;
    }
    let body = desc[end..].trim_start_matches(['\r', '\n']);
    Some((h, lines, body))
}

/// The migration of `items`: each old head turned into a header, then every derived list filled
/// over the whole set, then the links the references lack. An item already in form is left
/// alone, so planning a migrated tracker gives an empty plan.
pub fn plan(items: &[Item], words: &Words, extra_keys: &[String]) -> Result<Plan, Error> {
    let mut heads: HashMap<u64, Vec<String>> = HashMap::new();
    let mut migrated: Vec<Item> = items.to_vec();
    for it in &mut migrated {
        if let Some((h, lines, body)) = read_head(&it.desc, words, extra_keys) {
            let text = header::with_header(body, &h).map_err(Error::Malformed)?;
            heads.insert(it.iid, lines);
            it.desc = text;
        }
    }
    let naming = derived::naming_index(&migrated);
    let finals: Vec<(u64, String)> = migrated
        .iter()
        .map(|it| {
            let Ok(split) = header::parse(&it.desc, extra_keys) else {
                return (it.iid, it.desc.clone());
            };
            let h = split.header.unwrap_or_default();
            let e = derived::expected(it.iid, naming.get(&it.iid).into_iter().flatten().copied());
            let r = derived::rebuilt(&h, &e);
            (
                it.iid,
                header::with_header(&it.desc, &r).unwrap_or_else(|_| it.desc.clone()),
            )
        })
        .collect();
    let before: HashMap<u64, &Item> = items.iter().map(|it| (it.iid, it)).collect();
    let mut plan = Plan::default();
    let mut pairs: BTreeSet<(u64, u64)> = BTreeSet::new();
    for (iid, after) in &finals {
        let old = before[iid];
        for (_, m) in header::refs_of(after) {
            let linked = old.links.as_ref().is_some_and(|l| l.contains(&m));
            let known = before.contains_key(&m);
            if !linked && known && old.links.is_some() {
                pairs.insert((m.min(*iid), m.max(*iid)));
            }
        }
        if *after == old.desc {
            continue;
        }
        let head = heads.remove(iid).unwrap_or_default();
        let mut kinds = Vec::new();
        if let Ok(header::Split {
            header: Some(h), ..
        }) = header::parse(after, extra_keys)
        {
            let from_head: BTreeSet<&str> = head
                .iter()
                .filter_map(|l| read_line(l, words, extra_keys))
                .map(|l| match l {
                    Line::Ref(k, ..) => k,
                    Line::Extra(..) => "extra",
                })
                .collect();
            kinds.extend(from_head);
            if !h.notes.is_empty() && !head.is_empty() {
                kinds.push("notes");
            }
            if !h.children.is_empty() || !h.blocks.is_empty() || head.is_empty() {
                kinds.push("derived");
            }
        }
        plan.changes.push(Change {
            iid: *iid,
            old_head: head,
            before: old.desc.clone(),
            after: after.clone(),
            kinds,
        });
    }
    plan.changes.sort_by_key(|c| c.iid);
    plan.links = pairs.into_iter().collect();
    Ok(plan)
}

/// `items` with the plan's descriptions and links applied, as the tracker will read after `apply`.
pub fn applied(items: &[Item], plan: &Plan) -> Vec<Item> {
    let after: HashMap<u64, &str> = plan
        .changes
        .iter()
        .map(|c| (c.iid, c.after.as_str()))
        .collect();
    let mut out = items.to_vec();
    for it in &mut out {
        if let Some(a) = after.get(&it.iid) {
            it.desc = (*a).to_string();
        }
        for &(a, b) in &plan.links {
            let other = if it.iid == a {
                b
            } else if it.iid == b {
                a
            } else {
                continue;
            };
            let links = it.links.get_or_insert_with(Vec::new);
            if !links.contains(&other) {
                links.push(other);
            }
        }
    }
    out
}

/// Writes the plan: every description, then every link. A failure names what is left.
pub fn apply(tracker: &dyn Tracker, plan: &Plan) -> Result<(), Error> {
    for (i, c) in plan.changes.iter().enumerate() {
        tracker.set_desc(c.iid, &c.after).map_err(|e| {
            Error::Unfinished(format!(
                "{i} of {} items written, then #{} failed ({e}); run migrate --apply again",
                plan.changes.len(),
                c.iid
            ))
        })?;
    }
    for (i, &(a, b)) in plan.links.iter().enumerate() {
        tracker.add_link(a, b).map_err(|e| {
            Error::Unfinished(format!(
                "every item written and {i} of {} links made, then #{a} and #{b} failed ({e}); run migrate --apply again",
                plan.links.len()
            ))
        })?;
    }
    Ok(())
}

/// Counts by kind of change: items per kind, then items and links in all.
pub fn counts(plan: &Plan) -> BTreeMap<&'static str, usize> {
    let mut m = BTreeMap::new();
    for c in &plan.changes {
        for k in &c.kinds {
            *m.entry(*k).or_insert(0) += 1;
        }
    }
    m.insert("items", plan.changes.len());
    m.insert("links", plan.links.len());
    m
}

/// Each change as its old head (`- `) and its new block (`+ `), then the links and the counts.
pub fn render(plan: &Plan, applied: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for c in &plan.changes {
        lines.push(format!("#{} ({})", c.iid, c.kinds.join(", ")));
        for l in &c.old_head {
            lines.push(format!("  - {l}"));
        }
        let block = header::render(&header::loose(&c.after).header.unwrap_or_default());
        for l in block.lines() {
            lines.push(format!("  + {l}"));
        }
    }
    for (a, b) in &plan.links {
        lines.push(format!("link #{a} #{b}"));
    }
    let c = counts(plan);
    let list: Vec<String> = c.iter().map(|(k, v)| format!("{k} {v}")).collect();
    let verb = if applied { "written" } else { "dry run" };
    lines.push(format!("{verb}: {}", list.join(", ")));
    lines
}

pub fn to_json(plan: &Plan, applied: bool) -> Value {
    let changes: Vec<Value> = plan
        .changes
        .iter()
        .map(|c| json!({ "item": c.iid, "kinds": c.kinds, "old_head": c.old_head, "after": c.after }))
        .collect();
    let links: Vec<Value> = plan.links.iter().map(|(a, b)| json!([a, b])).collect();
    json!({ "version": JSON_VERSION, "applied": applied, "counts": counts(plan), "changes": changes, "links": links })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::State::Open;

    fn item(iid: u64, desc: &str, links: &[u64]) -> Item {
        let mut it = Item::new(iid, Open, &format!("t{iid}"), desc);
        it.links = Some(links.to_vec());
        it
    }

    fn words() -> Words {
        Words {
            from: vec!["From".into(), "출처".into()],
            related: vec!["Related".into(), "관련".into()],
        }
    }

    fn head(desc: &str) -> Option<(Header, Vec<String>, &str)> {
        read_head(desc, &words(), &["marker".into()])
    }

    #[test]
    fn a_parent_line_and_a_blank_line_become_a_header() {
        let (h, lines, body) = head("Parent: #12\n\nBody.\nMore.").unwrap();
        assert_eq!(
            (h.parent, lines, body),
            (Some(12), vec!["Parent: #12".to_string()], "Body.\nMore.")
        );
    }

    #[test]
    fn key_lines_a_blank_line_split_off_the_top_join_the_head() {
        let (h, lines, body) =
            head("marker: K-1 (x)\r\n\r\nParent: #3\r\n\r\nBlocked by: #4, #2 (the store)\r\nBody")
                .unwrap();
        assert_eq!(h.parent, Some(3));
        assert_eq!(h.blocked_by, [2, 4]);
        assert_eq!(h.notes["blocked_by"], "#4, #2 (the store)");
        assert_eq!(h.extra["marker"].as_str(), Some("K-1 (x)"));
        assert_eq!(lines.len(), 3);
        assert_eq!(body, "Body");
    }

    #[test]
    fn the_repos_words_and_depends_on_are_read() {
        let (h, ..) =
            head("출처: #649 (후속 — #649 의 일부가 아님)\n관련: #7, #8\nDepends on: #9\n\nx")
                .unwrap();
        assert_eq!(
            (h.from, h.related.clone(), h.blocked_by.clone()),
            (Some(649), vec![7, 8], vec![9])
        );
        assert_eq!(h.notes["from"], "#649 (후속 — #649 의 일부가 아님)");
        assert!(!h.notes.contains_key("related"));
    }

    #[test]
    fn a_line_that_names_no_number_ends_the_head_and_stays_in_the_body() {
        let (h, _, body) = head("Parent: #1\n\nBlocked by: none — landed\nBody").unwrap();
        assert_eq!((h.parent, h.blocked_by.clone()), (Some(1), vec![]));
        assert_eq!(body, "Blocked by: none — landed\nBody");
        let (_, _, body) = head("Parent: #1\n\nParent: `../PRD.md` · decision\nx").unwrap();
        assert_eq!(body, "Parent: `../PRD.md` · decision\nx");
        assert!(head("Body first\nParent: #1").is_none());
        assert!(head("+++\nparent = 1\n+++\n").is_none());
        assert!(head("Parent: #1 is quoted").is_none());
    }

    #[test]
    fn from_equal_to_parent_keeps_the_parent_and_its_note() {
        let (h, ..) = head("Parent: #5\nFrom: #5 (found here)\n\nx").unwrap();
        assert_eq!((h.parent, h.from), (Some(5), None));
        assert_eq!(h.notes["parent"], "#5 (found here)");
    }

    fn tracker() -> Vec<Item> {
        vec![
            item(1, "", &[2, 3]),
            item(2, "Parent: #1\n\nBlocked by: #4\nbody two", &[1]),
            item(3, "marker: K-3\n\nParent: #1\n\n## Body\r\ntext", &[1]),
            item(4, "From: #2 (seen there)\n\nfour", &[2]),
        ]
    }

    #[test]
    fn the_plan_moves_every_head_fills_every_derived_list_and_links_every_reference() {
        let items = tracker();
        let p = plan(&items, &words(), &["marker".into()]).unwrap();
        let after: Vec<(u64, &str)> = p
            .changes
            .iter()
            .map(|c| (c.iid, c.after.as_str()))
            .collect();
        assert_eq!(
            after,
            [
                (1, "+++\nchildren = [2, 3]\n+++\n"),
                (2, "+++\nparent = 1\nblocked_by = [4]\n+++\n\nbody two"),
                (
                    3,
                    "+++\nparent = 1\n\n[extra]\nmarker = \"K-3\"\n+++\n\n## Body\r\ntext"
                ),
                (
                    4,
                    "+++\nfrom = 2\nblocks = [2]\n\n[notes]\nfrom = \"#2 (seen there)\"\n+++\n\nfour"
                ),
            ]
        );
        assert_eq!(p.links, [(2, 4)]);
        let checked = crate::check::check(
            &applied(&items, &p),
            &crate::scheme::FrontMatter,
            &crate::check::RootsOnly,
            &["marker".into()],
        );
        assert!(checked.is_empty(), "{checked:?}");
        let again = plan(&applied(&items, &p), &words(), &["marker".into()]).unwrap();
        assert_eq!(again, Plan::default());
    }

    #[test]
    fn the_body_after_the_head_is_kept_byte_for_byte() {
        let items = tracker();
        let p = plan(&items, &words(), &["marker".into()]).unwrap();
        let three = p.changes.iter().find(|c| c.iid == 3).unwrap();
        assert!(three.after.ends_with("\n## Body\r\ntext"));
        assert_eq!(three.old_head, ["marker: K-3", "Parent: #1"]);
    }

    #[test]
    fn counts_name_each_kind_and_apply_writes_then_links() {
        let items = tracker();
        let p = plan(&items, &words(), &["marker".into()]).unwrap();
        let c = counts(&p);
        assert_eq!(
            (
                c["items"],
                c["links"],
                c["parent"],
                c["extra"],
                c["from"],
                c["blocked_by"]
            ),
            (4, 1, 2, 1, 1, 1)
        );
        let t = crate::tracker::FakeTracker::new(items);
        apply(&t, &p).unwrap();
        assert_eq!(
            *t.writes.borrow(),
            ["desc #1", "desc #2", "desc #3", "desc #4", "link #2 #4"]
        );
        t.fail_links.set(true);
        let e = apply(&t, &p).unwrap_err().to_string();
        assert!(e.contains("0 of 1 links") && e.contains("again"), "{e}");
    }
}
