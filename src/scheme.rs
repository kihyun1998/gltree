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

#[cfg(test)]
mod tests {
    use super::ParentLine;

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
