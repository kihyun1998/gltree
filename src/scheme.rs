//! Relation schemes: where a tracker records an item's parent.

use crate::item::Item;

pub trait RelationScheme {
    fn parent_of(&self, item: &Item) -> Option<u64>;
}

/// The parent is the body's first line `Parent: #N` (ADR-0001).
pub struct ParentLine;

impl ParentLine {
    pub fn parse(desc: &str) -> Option<u64> {
        let first = desc.split('\n').next()?;
        let n = first.strip_prefix("Parent: #")?.trim_end();
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        n.parse().ok()
    }
}

impl RelationScheme for ParentLine {
    fn parent_of(&self, item: &Item) -> Option<u64> {
        Self::parse(&item.desc)
    }
}

/// Where an item was found, apart from what it is part of: `<word>: #N`, optionally followed by
/// whitespace and free text, directly under the Parent line or else the first line. The word is
/// the repo's (`origin_word`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginLine {
    pub word: String,
}

impl OriginLine {
    pub fn new(word: &str) -> Self {
        OriginLine { word: word.into() }
    }

    /// The number one line names, wherever the line sits.
    pub fn parse_line(&self, line: &str) -> Option<u64> {
        let rest = line.strip_prefix(self.word.as_str())?.strip_prefix(": #")?;
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let (n, tail) = rest.split_at(end);
        if n.is_empty() || !(tail.is_empty() || tail.starts_with(char::is_whitespace)) {
            return None;
        }
        n.parse().ok()
    }

    /// The line index the origin line takes in `desc`: 1 under a Parent line, else 0.
    pub fn place(desc: &str) -> usize {
        usize::from(ParentLine::parse(desc).is_some())
    }

    /// The origin `desc` names in its place.
    pub fn origin_of(&self, desc: &str) -> Option<u64> {
        self.parse_line(desc.split('\n').nth(Self::place(desc))?)
    }
}

#[cfg(test)]
mod tests {
    use super::{OriginLine, ParentLine};

    fn from() -> OriginLine {
        OriginLine::new("From")
    }

    #[test]
    fn an_origin_line_is_the_word_and_a_number_then_free_text() {
        let o = from();
        assert_eq!(o.parse_line("From: #7"), Some(7));
        assert_eq!(o.parse_line("From: #7 (found while measuring)"), Some(7));
        assert_eq!(o.parse_line("From: #7\r"), Some(7));
        assert_eq!(o.parse_line("From: #7x"), None);
        assert_eq!(o.parse_line("From: #"), None);
        assert_eq!(o.parse_line("from: #7"), None);
        assert_eq!(o.parse_line("Parent: #7"), None);
        assert_eq!(
            OriginLine::new("출처").parse_line("출처: #649 (후속)"),
            Some(649)
        );
    }

    #[test]
    fn the_origin_sits_under_the_parent_line_or_first() {
        let o = from();
        assert_eq!(o.origin_of("From: #3\n\nx"), Some(3));
        assert_eq!(o.origin_of("Parent: #1\nFrom: #3\n\nx"), Some(3));
        assert_eq!(o.origin_of("Parent: #1\r\nFrom: #3\r\n\r\nx"), Some(3));
        assert_eq!(o.origin_of("Parent: #1\n\nFrom: #3"), None);
        assert_eq!(o.origin_of("x\nFrom: #3"), None);
        assert_eq!(o.origin_of("Parent: #1"), None);
    }

    #[test]
    fn reads_the_first_line_only() {
        assert_eq!(ParentLine::parse("Parent: #7\n\nx"), Some(7));
        assert_eq!(ParentLine::parse("x\nParent: #7"), None);
    }

    #[test]
    fn accepts_a_crlf_body_and_trailing_spaces() {
        assert_eq!(ParentLine::parse("Parent: #7\r\n\r\nx"), Some(7));
        assert_eq!(ParentLine::parse("Parent: #7  "), Some(7));
    }

    #[test]
    fn rejects_anything_else_on_the_line() {
        assert_eq!(ParentLine::parse("Parent: #7 and more"), None);
        assert_eq!(ParentLine::parse("Parent: #"), None);
        assert_eq!(ParentLine::parse("Parent: #+7"), None);
        assert_eq!(ParentLine::parse("parent: #7"), None);
        assert_eq!(ParentLine::parse(""), None);
    }
}
