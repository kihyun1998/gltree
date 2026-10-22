//! Relation schemes: where a tracker records an item's parent.

use crate::item::Item;

pub trait RelationScheme {
    fn parent_of(&self, item: &Item) -> Option<u64>;
}

/// The parent is the header's `parent` key (ADR-0004).
pub struct FrontMatter;

impl RelationScheme for FrontMatter {
    fn parent_of(&self, item: &Item) -> Option<u64> {
        crate::header::parent_of(&item.desc)
    }
}

/// The first-line format the header replaced: the body's first line `Parent: #N`. Nothing reads
/// a tracker by it any more; it is kept for turning such a tracker into headers.
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

#[cfg(test)]
mod tests {
    use super::{FrontMatter, ParentLine, RelationScheme};
    use crate::item::{Item, State};

    #[test]
    fn the_parent_comes_from_the_header_only() {
        let it = |desc: &str| Item::new(2, State::Open, "t", desc);
        assert_eq!(
            FrontMatter.parent_of(&it("+++\nparent = 7\n+++\n\nx")),
            Some(7)
        );
        assert_eq!(FrontMatter.parent_of(&it("Parent: #7\n\nx")), None);
    }

    #[test]
    fn the_old_line_reads_the_first_line_only() {
        assert_eq!(ParentLine::parse("Parent: #7\n\nx"), Some(7));
        assert_eq!(ParentLine::parse("Parent: #7\r\n\r\nx"), Some(7));
        assert_eq!(ParentLine::parse("Parent: #7  "), Some(7));
        assert_eq!(ParentLine::parse("x\nParent: #7"), None);
        assert_eq!(ParentLine::parse("Parent: #7 and more"), None);
        assert_eq!(ParentLine::parse("Parent: #+7"), None);
        assert_eq!(ParentLine::parse(""), None);
    }
}
