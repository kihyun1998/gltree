//! The tracker's roots, filtered for tools that act on finished features.

use crate::item::{Item, State};
use crate::scheme::{OriginLine, RelationScheme};
use crate::tracker::Error;
use serde_json::{Value, json};

/// The version of the JSON shape `to_json` prints.
pub const JSON_VERSION: u32 = 1;

/// Which roots to keep; every condition must hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    pub state: Option<State>,
    /// Labels a root must carry, every one.
    pub labels: Vec<String>,
    /// Labels a root must not carry; a trailing `*` matches a prefix.
    pub without: Vec<String>,
    /// Inclusive UTC dates, `YYYY-MM-DD`, on the closing date; either one implies closed.
    pub closed_since: Option<String>,
    pub closed_until: Option<String>,
}

/// Whether `label` is matched by `pattern`: equal, or starting with the part before a trailing `*`.
pub fn label_matches(pattern: &str, label: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => label.starts_with(prefix),
        None => label == pattern,
    }
}

/// A `YYYY-MM-DD` date, or the reason it is not one.
pub fn check_date(d: &str) -> Result<(), Error> {
    let b = d.as_bytes();
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && digits(0..4)
        && digits(5..7)
        && digits(8..10)
    {
        Ok(())
    } else {
        Err(Error::Refused(format!(
            "`{d}` is not a date; write YYYY-MM-DD"
        )))
    }
}

/// The roots that pass `filter`, by number.
pub fn roots<'a>(
    items: &'a [Item],
    scheme: &dyn RelationScheme,
    filter: &Filter,
) -> Result<Vec<&'a Item>, Error> {
    for d in filter.closed_since.iter().chain(&filter.closed_until) {
        check_date(d)?;
    }
    let dated = filter.closed_since.is_some() || filter.closed_until.is_some();
    let mut out: Vec<&Item> = Vec::new();
    for it in items.iter().filter(|it| scheme.parent_of(it).is_none()) {
        let state = if dated {
            Some(State::Closed)
        } else {
            filter.state
        };
        if state.is_some_and(|s| s != it.state) {
            continue;
        }
        if !filter.labels.iter().all(|l| it.labels.contains(l)) {
            continue;
        }
        if filter
            .without
            .iter()
            .any(|p| it.labels.iter().any(|l| label_matches(p, l)))
        {
            continue;
        }
        if dated {
            let Some(at) = &it.closed_at else {
                return Err(Error::Refused(format!(
                    "#{} is closed but its closed_at is not recorded; take a fresh snapshot, or read the server",
                    it.iid
                )));
            };
            let day = &at[..at.len().min(10)];
            if filter.closed_since.as_deref().is_some_and(|s| day < s)
                || filter.closed_until.as_deref().is_some_and(|u| day > u)
            {
                continue;
            }
        }
        out.push(it);
    }
    out.sort_by_key(|it| it.iid);
    Ok(out)
}

fn closed_day(it: &Item) -> &str {
    it.closed_at
        .as_deref()
        .map_or("-", |at| &at[..at.len().min(10)])
}

/// `#N [state] closed:<date|-> labels:<a,b> title`
pub fn render(roots: &[&Item]) -> Vec<String> {
    roots
        .iter()
        .map(|it| {
            format!(
                "#{} [{}] closed:{} labels:{} {}",
                it.iid,
                it.state.as_str(),
                closed_day(it),
                it.labels.join(","),
                it.title
            )
        })
        .collect()
}

/// `origin` is the number the item's origin line names under the repo's word, or null.
pub fn to_json(roots: &[&Item], origin: Option<&OriginLine>) -> Value {
    let list: Vec<Value> = roots
        .iter()
        .map(|it| {
            json!({
                "number": it.iid,
                "title": it.title,
                "state": it.state.as_str(),
                "labels": it.labels,
                "closed_at": it.closed_at,
                "first_line": it.desc.split('\n').next().unwrap_or("").trim_end_matches('\r'),
                "url": it.url,
                "origin": origin.and_then(|r| r.origin_of(&it.desc)),
            })
        })
        .collect();
    json!({ "version": JSON_VERSION, "roots": list })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheme::ParentLine;

    fn it(iid: u64, state: State, desc: &str, labels: &[&str], closed: Option<&str>) -> Item {
        let mut i = Item::new(iid, state, &format!("t{iid}"), desc);
        i.labels = labels.iter().map(|s| s.to_string()).collect();
        i.closed_at = closed.map(String::from);
        i
    }

    fn items() -> Vec<Item> {
        vec![
            it(
                1,
                State::Closed,
                "NIT: PTR-1",
                &["SPEC", "nit::registered"],
                Some("2026-10-19T23:59:59Z"),
            ),
            it(
                2,
                State::Closed,
                "",
                &["SPEC"],
                Some("2026-10-21T03:34:56Z"),
            ),
            it(
                3,
                State::Closed,
                "Parent: #2",
                &["bug"],
                Some("2026-10-21T00:00:00Z"),
            ),
            it(4, State::Open, "## Highlights", &["SPEC"], None),
            it(
                5,
                State::Closed,
                "",
                &["chore", "nit"],
                Some("2026-10-25T00:00:00Z"),
            ),
            it(
                6,
                State::Closed,
                "",
                &["xnit::y"],
                Some("2026-10-26T00:00:00Z"),
            ),
        ]
    }

    fn numbers(f: Filter) -> Vec<u64> {
        roots(&items(), &ParentLine, &f)
            .unwrap()
            .iter()
            .map(|i| i.iid)
            .collect()
    }

    #[test]
    fn an_item_with_a_parent_line_is_never_a_root() {
        assert_eq!(numbers(Filter::default()), [1, 2, 4, 5, 6]);
        assert!(
            !numbers(Filter {
                labels: vec!["bug".into()],
                ..Filter::default()
            })
            .contains(&3)
        );
    }

    #[test]
    fn state_keeps_open_or_closed() {
        assert_eq!(
            numbers(Filter {
                state: Some(State::Open),
                ..Filter::default()
            }),
            [4]
        );
        assert_eq!(
            numbers(Filter {
                state: Some(State::Closed),
                ..Filter::default()
            }),
            [1, 2, 5, 6]
        );
    }

    #[test]
    fn labels_must_all_be_present() {
        assert_eq!(
            numbers(Filter {
                labels: vec!["SPEC".into()],
                ..Filter::default()
            }),
            [1, 2, 4]
        );
        assert_eq!(
            numbers(Filter {
                labels: vec!["SPEC".into(), "nit::registered".into()],
                ..Filter::default()
            }),
            [1]
        );
    }

    #[test]
    fn a_without_prefix_drops_any_label_that_starts_with_it_and_nothing_else() {
        assert_eq!(
            numbers(Filter {
                without: vec!["nit::*".into()],
                ..Filter::default()
            }),
            [2, 4, 5, 6]
        );
        assert!(label_matches("nit::*", "nit::skipped"));
        assert!(!label_matches("nit::*", "nit"));
        assert!(!label_matches("nit::*", "xnit::y"));
        assert!(label_matches("bug", "bug") && !label_matches("bug", "bugs"));
    }

    #[test]
    fn filters_combine_with_and() {
        let f = Filter {
            state: Some(State::Closed),
            labels: vec!["SPEC".into()],
            without: vec!["nit::*".into()],
            ..Filter::default()
        };
        assert_eq!(numbers(f), [2]);
    }

    #[test]
    fn the_closed_date_range_is_inclusive_in_utc_and_implies_closed() {
        let f = Filter {
            closed_since: Some("2026-10-19".into()),
            closed_until: Some("2026-10-25".into()),
            ..Filter::default()
        };
        assert_eq!(numbers(f), [1, 2, 5]);
        let since = Filter {
            closed_since: Some("2026-10-21".into()),
            ..Filter::default()
        };
        assert_eq!(numbers(since), [2, 5, 6]);
    }

    #[test]
    fn a_date_filter_over_items_without_a_closing_date_is_an_error() {
        let mut old = items();
        old[1].closed_at = None;
        let f = Filter {
            closed_since: Some("2026-10-19".into()),
            ..Filter::default()
        };
        let e = roots(&old, &ParentLine, &f).unwrap_err().to_string();
        assert!(e.contains("closed_at"), "{e}");
        assert!(roots(&old, &ParentLine, &Filter::default()).is_ok());
    }

    #[test]
    fn a_date_must_be_yyyy_mm_dd() {
        assert!(check_date("2026-10-19").is_ok());
        for bad in ["2026-1-19", "19-10-2026", "2026/10/19", "2026-10-19T00:00"] {
            assert!(check_date(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_first_line_of_a_crlf_body_has_no_cr() {
        let crlf = it(9, State::Closed, "NIT: PTR-9\r\nbody", &[], None);
        assert_eq!(
            to_json(&[&crlf], None)["roots"][0]["first_line"],
            "NIT: PTR-9"
        );
    }

    #[test]
    fn json_names_the_origin_under_the_repos_word() {
        let found = it(9, State::Open, "From: #3 (x)\n\nbody", &[], None);
        let o = OriginLine::new("From");
        assert_eq!(to_json(&[&found], Some(&o))["roots"][0]["origin"], 3);
        assert_eq!(to_json(&[&found], None)["roots"][0]["origin"], Value::Null);
    }

    #[test]
    fn text_and_json_carry_date_labels_and_first_line() {
        let all = items();
        let picked: Vec<&Item> = vec![&all[0], &all[3]];
        assert_eq!(
            render(&picked),
            [
                "#1 [closed] closed:2026-10-19 labels:SPEC,nit::registered t1",
                "#4 [open] closed:- labels:SPEC t4"
            ]
        );
        let v = to_json(&picked, None);
        assert_eq!(v["version"], 1);
        assert_eq!(v["roots"][0]["first_line"], "NIT: PTR-1");
        assert_eq!(v["roots"][0]["closed_at"], "2026-10-19T23:59:59Z");
        assert_eq!(v["roots"][1]["closed_at"], Value::Null);
        assert_eq!(v["roots"][1]["number"], 4);
    }
}
