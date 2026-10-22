//! The header: a TOML block between `+++` lines at the very start of a description (ADR-0004).

use std::collections::BTreeMap;

pub const FENCE: &str = "+++";
/// The reference keys, in the order gltree writes them.
pub const REFS: &[&str] = &[
    "parent",
    "from",
    "children",
    "related",
    "blocked_by",
    "blocks",
];
/// The keys a person writes a note for.
pub const NOTED: &[&str] = &["parent", "from", "related", "blocked_by"];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Header {
    pub parent: Option<u64>,
    pub from: Option<u64>,
    pub children: Vec<u64>,
    pub related: Vec<u64>,
    pub blocked_by: Vec<u64>,
    pub blocks: Vec<u64>,
    /// One reason per reference key, as written.
    pub notes: BTreeMap<String, String>,
    /// Keys the repo config declares, kept as written and never read.
    pub extra: BTreeMap<String, toml::Value>,
}

/// A description cut into its header and the body after the closing fence.
#[derive(Debug, Clone, PartialEq)]
pub struct Split<'a> {
    pub header: Option<Header>,
    pub body: &'a str,
}

impl Header {
    pub fn is_empty(&self) -> bool {
        *self == Header::default()
    }

    /// Drops each note whose key the header no longer holds, so a write that empties a key never
    /// leaves the header out of form.
    pub fn drop_orphan_notes(&mut self) {
        let keep: Vec<String> = self.notes.keys().filter(|k| self.has(k)).cloned().collect();
        self.notes.retain(|k, _| keep.contains(k));
    }

    /// Whether the header holds a value under the reference key `key`.
    pub fn has(&self, key: &str) -> bool {
        match key {
            "parent" => self.parent.is_some(),
            "from" => self.from.is_some(),
            "related" => !self.related.is_empty(),
            "blocked_by" => !self.blocked_by.is_empty(),
            _ => false,
        }
    }

    /// The first reference key, in the written order, that names `iid`.
    pub fn naming(&self, iid: u64) -> Option<&'static str> {
        let lists: [(&'static str, &[u64]); 4] = [
            ("children", &self.children),
            ("related", &self.related),
            ("blocked_by", &self.blocked_by),
            ("blocks", &self.blocks),
        ];
        if self.parent == Some(iid) {
            return Some("parent");
        }
        if self.from == Some(iid) {
            return Some("from");
        }
        lists
            .into_iter()
            .find(|(_, ns)| ns.contains(&iid))
            .map(|(k, _)| k)
    }
}

/// The block's TOML text and the body, or `None` when `desc` does not open with a block.
/// An opening fence with no closing one is an error.
fn fenced(desc: &str) -> Result<Option<(&str, &str)>, String> {
    let Some(rest) = strip_fence_line(desc) else {
        return Ok(None);
    };
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == FENCE {
            return Ok(Some((&rest[..at], &rest[at + line.len()..])));
        }
        at += line.len();
    }
    Err(format!("the header opens with `{FENCE}` but never closes"))
}

fn strip_fence_line(desc: &str) -> Option<&str> {
    let rest = desc.strip_prefix(FENCE)?;
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

/// The header and body of `desc`, refusing anything out of form; `extra` names the keys the
/// repo config declares for `[extra]`.
pub fn parse<'a>(desc: &'a str, extra: &[String]) -> Result<Split<'a>, String> {
    let Some((text, body)) = fenced(desc)? else {
        return Ok(Split {
            header: None,
            body: desc,
        });
    };
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| format!("the header is not valid TOML: {}", e.message()))?;
    let mut h = Header::default();
    for (key, value) in table {
        match key.as_str() {
            "parent" => h.parent = Some(number(&key, &value)?),
            "from" => h.from = Some(number(&key, &value)?),
            "children" => h.children = list(&key, &value)?,
            "related" => h.related = list(&key, &value)?,
            "blocked_by" => h.blocked_by = list(&key, &value)?,
            "blocks" => h.blocks = list(&key, &value)?,
            "notes" => h.notes = notes(&value)?,
            "extra" => h.extra = extras(&value, extra)?,
            _ => return Err(format!("unknown key `{key}`")),
        }
    }
    if h.from.is_some() && h.from == h.parent {
        return Err(
            "`from` names the parent; an item found while working its parent is part of it, so keep `parent` alone"
                .into(),
        );
    }
    if let Some(k) = h.notes.keys().find(|k| !h.has(k)) {
        return Err(format!("`[notes]` has `{k}`, but the header has no `{k}`"));
    }
    Ok(Split {
        header: Some(h),
        body,
    })
}

/// The references a person writes — `parent`, `from`, `related`, `blocked_by` — as far as they can
/// be read, keyed by name: a value out of form is skipped rather than failing the rest, so one
/// stray key never hides the others from the link and state checks.
pub fn refs_of(desc: &str) -> Vec<(&'static str, u64)> {
    let Some(table) = fenced(desc)
        .ok()
        .flatten()
        .and_then(|(text, _)| text.parse::<toml::Table>().ok())
    else {
        return vec![];
    };
    let n = |v: &toml::Value| {
        v.as_integer()
            .and_then(|n| u64::try_from(n).ok())
            .filter(|n| *n > 0)
    };
    let mut out = Vec::new();
    for key in ["parent", "from"] {
        if let Some(x) = table.get(key).and_then(n) {
            out.push((key, x));
        }
    }
    for key in ["related", "blocked_by"] {
        let items = table.get(key).and_then(|v| v.as_array());
        for x in items.into_iter().flatten().filter_map(n) {
            out.push((key, x));
        }
    }
    out
}

/// The keys of the first-line format a body may still open with; a repo adds its own words
/// through `old_head_keys`.
pub const OLD_HEAD_KEYS: &[&str] = &["Parent", "From", "Related", "Blocked by", "Depends on"];

/// The first line of `body` when it is still a first-line head: one of `OLD_HEAD_KEYS` or
/// `more_keys`, a colon, and an issue number with nothing after it but a list, a reason or
/// punctuation — such as `Blocked by: #4`. A sentence that only starts that way is not one, and
/// another key with numbers (`Fixed with: #2`) is prose of the body's own.
pub fn old_head<'a>(body: &'a str, more_keys: &[String]) -> Option<&'a str> {
    let line = body
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .find(|l| !l.trim().is_empty())?;
    let (key, rest) = line.split_once(": #")?;
    if !OLD_HEAD_KEYS.contains(&key) && !more_keys.iter().any(|k| k == key) {
        return None;
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let after = rest[digits..].trim_start();
    let tail_ok = after.is_empty() || after.starts_with([',', '(', ';', '.', '—', '-', '#']);
    (digits > 0 && tail_ok).then_some(line)
}

/// The header and body as far as they can be read, for display: every `[extra]` key is taken,
/// and a header out of form reads as none with the body still cut after the block.
pub fn loose(desc: &str) -> Split<'_> {
    let Ok(Some((text, body))) = fenced(desc) else {
        return Split {
            header: None,
            body: desc,
        };
    };
    let declared: Vec<String> = text
        .parse::<toml::Table>()
        .ok()
        .and_then(|t| {
            t.get("extra")
                .and_then(|e| e.as_table())
                .map(|e| e.keys().cloned().collect())
        })
        .unwrap_or_default();
    Split {
        header: parse(desc, &declared).ok().and_then(|s| s.header),
        body,
    }
}

/// The parent `desc` names, or `None` when it names none or its header cannot be read at all.
pub fn parent_of(desc: &str) -> Option<u64> {
    let (text, _) = fenced(desc).ok()??;
    let table: toml::Table = text.parse().ok()?;
    table
        .get("parent")
        .and_then(|v| v.as_integer())
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| *n > 0)
}

fn number(key: &str, v: &toml::Value) -> Result<u64, String> {
    v.as_integer()
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| format!("`{key}` must be an issue number, such as `{key} = 12`"))
}

fn list(key: &str, v: &toml::Value) -> Result<Vec<u64>, String> {
    let bad = || format!("`{key}` must be a list of issue numbers, such as `{key} = [12, 13]`");
    let items = v.as_array().ok_or_else(bad)?;
    let ns = items
        .iter()
        .map(|x| number(key, x).map_err(|_| bad()))
        .collect::<Result<Vec<_>, _>>()?;
    if ns.is_empty() {
        return Err(format!("`{key}` is empty; leave the key out"));
    }
    if ns.windows(2).any(|w| w[0] >= w[1]) {
        return Err(format!(
            "`{key}` must be in ascending order with no number twice"
        ));
    }
    Ok(ns)
}

fn notes(v: &toml::Value) -> Result<BTreeMap<String, String>, String> {
    let t = v.as_table().ok_or("`[notes]` must be a table")?;
    t.iter()
        .map(|(k, v)| {
            if !NOTED.contains(&k.as_str()) {
                return Err(format!("`[notes]` has `{k}`; known: {}", NOTED.join(", ")));
            }
            let s = v
                .as_str()
                .ok_or_else(|| format!("`notes.{k}` must be a string"))?;
            Ok((k.clone(), s.to_string()))
        })
        .collect()
}

fn extras(v: &toml::Value, declared: &[String]) -> Result<BTreeMap<String, toml::Value>, String> {
    let t = v.as_table().ok_or("`[extra]` must be a table")?;
    t.iter()
        .map(|(k, v)| {
            if !declared.contains(k) {
                return Err(format!(
                    "`[extra]` has `{k}`, which the repo config does not declare"
                ));
            }
            Ok((k.clone(), v.clone()))
        })
        .collect()
}

/// The block for `h`, keys in the fixed order, ending with the closing fence and a newline;
/// empty for an empty header.
pub fn render(h: &Header) -> String {
    if h.is_empty() {
        return String::new();
    }
    let mut out = format!("{FENCE}\n");
    let one = |out: &mut String, k: &str, n: Option<u64>| {
        if let Some(n) = n {
            out.push_str(&format!("{k} = {n}\n"));
        }
    };
    let many = |out: &mut String, k: &str, ns: &[u64]| {
        if !ns.is_empty() {
            let ns: Vec<String> = ns.iter().map(u64::to_string).collect();
            out.push_str(&format!("{k} = [{}]\n", ns.join(", ")));
        }
    };
    one(&mut out, "parent", h.parent);
    one(&mut out, "from", h.from);
    many(&mut out, "children", &h.children);
    many(&mut out, "related", &h.related);
    many(&mut out, "blocked_by", &h.blocked_by);
    many(&mut out, "blocks", &h.blocks);
    let table = |out: &mut String, name: &str, rows: Vec<(&String, String)>| {
        if !rows.is_empty() {
            out.push_str(&format!("\n[{name}]\n"));
            for (k, v) in rows {
                out.push_str(&format!("{k} = {v}\n"));
            }
        }
    };
    let notes = NOTED
        .iter()
        .filter_map(|k| h.notes.get_key_value(*k))
        .map(|(k, v)| (k, toml::Value::String(v.clone()).to_string()))
        .collect();
    table(&mut out, "notes", notes);
    table(
        &mut out,
        "extra",
        h.extra.iter().map(|(k, v)| (k, v.to_string())).collect(),
    );
    out.push_str(FENCE);
    out.push('\n');
    out
}

/// `desc` with its header replaced by `h`, the body after the block untouched. A block added in
/// front of a body gets a blank line between them; dropping the block drops that one blank line.
pub fn with_header(desc: &str, h: &Header) -> Result<String, String> {
    let body = match fenced(desc)? {
        Some((_, body)) => body,
        None if h.is_empty() => return Ok(desc.into()),
        None if desc.is_empty() => "",
        None => return Ok(format!("{}\n{desc}", render(h))),
    };
    if h.is_empty() {
        let body = body
            .strip_prefix("\r\n")
            .or_else(|| body.strip_prefix('\n'))
            .unwrap_or(body);
        return Ok(body.into());
    }
    Ok(format!("{}{body}", render(h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(parent: Option<u64>) -> Header {
        Header {
            parent,
            ..Header::default()
        }
    }

    #[test]
    fn a_block_at_the_start_is_the_header_and_the_rest_is_the_body() {
        let s = parse("+++\nparent = 12\n+++\n\nBody.", &[]).unwrap();
        assert_eq!(s.header, Some(h(Some(12))));
        assert_eq!(s.body, "\nBody.");
        let s = parse("+++\r\nparent = 12\r\n+++\r\n\r\nBody.", &[]).unwrap();
        assert_eq!((s.header.unwrap().parent, s.body), (Some(12), "\r\nBody."));
    }

    #[test]
    fn no_block_or_a_block_anywhere_else_is_no_header() {
        for desc in [
            "",
            "Body.",
            "Parent: #12\n\nBody.",
            "x\n+++\nparent = 1\n+++",
            " +++\nparent = 1\n+++",
        ] {
            let s = parse(desc, &[]).unwrap();
            assert_eq!((s.header, s.body), (None, desc), "{desc:?}");
            assert_eq!(parent_of(desc), None, "{desc:?}");
        }
    }

    #[test]
    fn every_reference_key_and_both_tables_are_read() {
        let desc = "+++\nparent = 1\nfrom = 2\nchildren = [3, 4]\nrelated = [5]\nblocked_by = [6]\nblocks = [7]\n\n[notes]\nfrom = \"found here\"\n\n[extra]\nmarker = \"K-1 (x)\"\n+++\n";
        let got = parse(desc, &["marker".into()]).unwrap().header.unwrap();
        assert_eq!((got.parent, got.from), (Some(1), Some(2)));
        assert_eq!(
            (got.children, got.related, got.blocked_by, got.blocks),
            (vec![3, 4], vec![5], vec![6], vec![7])
        );
        assert_eq!(got.notes["from"], "found here");
        assert_eq!(got.extra["marker"].as_str(), Some("K-1 (x)"));
    }

    #[test]
    fn anything_out_of_form_is_refused_with_a_reason() {
        let cases = [
            ("+++\nparent = 12\n", "never closes"),
            ("+++\nparent = #12\n+++\n", "not valid TOML"),
            ("+++\nparent = \"12\"\n+++\n", "issue number"),
            ("+++\nparent = 0\n+++\n", "issue number"),
            ("+++\nparnet = 12\n+++\n", "unknown key `parnet`"),
            ("+++\nrelated = [3, 2]\n+++\n", "ascending"),
            ("+++\nrelated = [2, 2]\n+++\n", "ascending"),
            ("+++\nrelated = []\n+++\n", "empty"),
            ("+++\nrelated = 2\n+++\n", "list of issue numbers"),
            (
                "+++\n[notes]\nchildren = \"x\"\n+++\n",
                "`[notes]` has `children`",
            ),
            ("+++\n[extra]\nmarker = \"x\"\n+++\n", "does not declare"),
            (
                "+++\nparent = 3\nfrom = 3\n+++\n",
                "`from` names the parent",
            ),
            (
                "+++\nparent = 3\n\n[notes]\nfrom = \"x\"\n+++\n",
                "the header has no `from`",
            ),
        ];
        for (desc, says) in cases {
            let e = parse(desc, &[]).unwrap_err();
            assert!(e.contains(says), "{desc:?}: {e}");
        }
    }

    #[test]
    fn refs_are_read_loosely_by_key_skipping_what_is_out_of_form() {
        let desc = "+++\nparent = 1\nfrom = 2\nrelated = [5, 4]\nblocked_by = [6, \"x\"]\nchildren = [9]\nparnet = 3\n+++\n";
        assert_eq!(
            refs_of(desc),
            [
                ("parent", 1),
                ("from", 2),
                ("related", 5),
                ("related", 4),
                ("blocked_by", 6)
            ]
        );
        assert!(refs_of("Parent: #1").is_empty());
        assert!(refs_of("+++\nparent = #1\n+++\n").is_empty());
    }

    #[test]
    fn an_old_head_is_a_known_key_and_a_number_not_a_sentence() {
        let mine = ["Origin".to_string()];
        for line in [
            "Blocked by: #4",
            "Parent: #12",
            "Blocked by: #2550 (the state), #2551.",
            "From: #8 (found while reproducing)",
            "Related: #3, #4",
            "Depends on: #2495",
            "Origin: #649 (a find)",
        ] {
            assert_eq!(
                old_head(&format!("\n{line}\nbody"), &mine),
                Some(line),
                "{line}"
            );
        }
        for body in [
            "Parent: #1 is quoted here, not a parent line",
            "Mentions Parent: #1 on its first line",
            "## Problem\nParent: #1",
            "Blocked by: none",
            "Fixed with: #2 (the reader) + #3 (the writer)",
            "",
        ] {
            assert_eq!(old_head(body, &mine), None, "{body}");
        }
        assert_eq!(old_head("Origin: #649", &[]), None);
    }

    #[test]
    fn the_parent_is_read_even_from_a_header_check_refuses() {
        assert_eq!(parent_of("+++\nparent = 12\nparnet = 1\n+++\n"), Some(12));
        assert_eq!(parent_of("+++\nparent = #12\n+++\n"), None);
    }

    #[test]
    fn render_writes_the_fixed_order_and_parses_back() {
        let mut x = Header {
            parent: Some(1),
            from: Some(2),
            children: vec![3, 4],
            related: vec![5],
            blocked_by: vec![6],
            blocks: vec![7],
            ..Header::default()
        };
        x.notes
            .insert("blocked_by".into(), "needs \"the store\" first".into());
        x.notes
            .insert("from".into(), "설정 「외관」에서 발견".into());
        x.extra
            .insert("marker".into(), toml::Value::String("K-1".into()));
        let text = render(&x);
        assert_eq!(
            text,
            "+++\nparent = 1\nfrom = 2\nchildren = [3, 4]\nrelated = [5]\nblocked_by = [6]\nblocks = [7]\n\n[notes]\nfrom = \"설정 「외관」에서 발견\"\nblocked_by = 'needs \"the store\" first'\n\n[extra]\nmarker = \"K-1\"\n+++\n"
        );
        assert_eq!(parse(&text, &["marker".into()]).unwrap().header, Some(x));
        assert_eq!(render(&Header::default()), "");
    }

    #[test]
    fn with_header_keeps_the_body_byte_for_byte() {
        let body = "\r\n설정 「외관」\r\nREG: x\r\n";
        let desc = format!("+++\nparent = 1\n+++\n{body}");
        assert_eq!(
            with_header(&desc, &h(Some(4))).unwrap(),
            format!("+++\nparent = 4\n+++\n{body}")
        );
        assert_eq!(
            with_header(&desc, &h(None)).unwrap(),
            "설정 「외관」\r\nREG: x\r\n"
        );
        assert_eq!(
            with_header("text", &h(Some(4))).unwrap(),
            "+++\nparent = 4\n+++\n\ntext"
        );
        assert_eq!(
            with_header("", &h(Some(4))).unwrap(),
            "+++\nparent = 4\n+++\n"
        );
        assert_eq!(with_header("text", &h(None)).unwrap(), "text");
        let back = with_header(&with_header("a\n\nb", &h(Some(4))).unwrap(), &h(None)).unwrap();
        assert_eq!(back, "a\n\nb");
    }
}
