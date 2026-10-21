//! The tracker port: everything the core asks of a tracker.

use crate::item::Item;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// Reading the source failed (a file, a missing login, the network).
    Source(String),
    /// The source was read but its content is not a tracker.
    Malformed(String),
    /// The server answered with an error of its own: a refusal, a rate or complexity limit.
    Server(String),
    NotFound(u64),
    /// gltree will not do it, before touching the tracker: the reason says why.
    Refused(String),
    /// An item was created but a step after it failed; the message says what is left to do.
    Unfinished(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Source(m) => write!(f, "cannot read the tracker: {m}"),
            Error::Malformed(m) => write!(f, "not a tracker snapshot: {m}"),
            Error::Server(m) => write!(f, "the server refused: {m}"),
            Error::NotFound(n) => write!(f, "#{n} is not in the tracker"),
            Error::Refused(m) => write!(f, "{m}"),
            Error::Unfinished(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

/// One item of a level and the items linked to it, bodies included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    pub item: Item,
    pub linked: Vec<Item>,
}

/// An item the tracker created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub iid: u64,
    pub url: String,
}

pub trait Tracker {
    fn all_items(&self) -> Result<Vec<Item>, Error>;
    /// The given items with their linked items, in one request; an absent item is left out.
    fn level(&self, iids: &[u64]) -> Result<HashMap<u64, Linked>, Error>;
    fn create_item(&self, title: &str, desc: &str, labels: &[String]) -> Result<Created, Error>;
    /// A `relates_to` link between two items; an existing one is success.
    fn add_link(&self, a: u64, b: u64) -> Result<(), Error>;
    fn set_desc(&self, iid: u64, desc: &str) -> Result<(), Error>;
    /// Removes the link between two items; no link there is success.
    fn remove_link(&self, a: u64, b: u64) -> Result<(), Error>;
}

/// An in-memory tracker for tests and for callers that already hold the items.
/// Its links are each item's `links`; `levels` records every `level` call and `writes` every
/// write, as text; `fail_links` makes `add_link` fail and `fail_unlinks` `remove_link`.
pub struct FakeTracker {
    pub items: RefCell<Vec<Item>>,
    pub levels: RefCell<Vec<Vec<u64>>>,
    pub writes: RefCell<Vec<String>>,
    pub fail_links: Cell<bool>,
    pub fail_unlinks: Cell<bool>,
}

impl FakeTracker {
    pub fn new(items: Vec<Item>) -> Self {
        FakeTracker {
            items: RefCell::new(items),
            levels: RefCell::new(vec![]),
            writes: RefCell::new(vec![]),
            fail_links: Cell::new(false),
            fail_unlinks: Cell::new(false),
        }
    }

    pub fn item(&self, iid: u64) -> Option<Item> {
        self.items.borrow().iter().find(|it| it.iid == iid).cloned()
    }
}

impl Tracker for FakeTracker {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        Ok(self.items.borrow().clone())
    }

    fn level(&self, iids: &[u64]) -> Result<HashMap<u64, Linked>, Error> {
        let mut asked = iids.to_vec();
        asked.sort();
        self.levels.borrow_mut().push(asked);
        Ok(level_of(&self.items.borrow(), iids))
    }

    fn create_item(&self, title: &str, desc: &str, labels: &[String]) -> Result<Created, Error> {
        let mut items = self.items.borrow_mut();
        let iid = items.iter().map(|it| it.iid).max().unwrap_or(0) + 1;
        let mut it = Item::new(iid, crate::item::State::Open, title, desc);
        it.labels = labels.to_vec();
        it.links = Some(vec![]);
        items.push(it);
        self.writes
            .borrow_mut()
            .push(format!("create #{iid} {title:?} {labels:?}"));
        Ok(Created {
            iid,
            url: format!("fake://{iid}"),
        })
    }

    fn add_link(&self, a: u64, b: u64) -> Result<(), Error> {
        if self.fail_links.get() {
            return Err(Error::Server("link refused".into()));
        }
        for it in self.items.borrow_mut().iter_mut() {
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
        self.writes.borrow_mut().push(format!("link #{a} #{b}"));
        Ok(())
    }

    fn set_desc(&self, iid: u64, desc: &str) -> Result<(), Error> {
        let mut items = self.items.borrow_mut();
        let it = items
            .iter_mut()
            .find(|it| it.iid == iid)
            .ok_or(Error::NotFound(iid))?;
        it.desc = desc.into();
        self.writes.borrow_mut().push(format!("desc #{iid}"));
        Ok(())
    }

    fn remove_link(&self, a: u64, b: u64) -> Result<(), Error> {
        if self.fail_unlinks.get() {
            return Err(Error::Server("unlink refused".into()));
        }
        for it in self.items.borrow_mut().iter_mut() {
            let other = if it.iid == a {
                b
            } else if it.iid == b {
                a
            } else {
                continue;
            };
            if let Some(links) = it.links.as_mut() {
                links.retain(|l| *l != other);
            }
        }
        self.writes.borrow_mut().push(format!("unlink #{a} #{b}"));
        Ok(())
    }
}

/// A level taken from items that carry their own `links`.
pub fn level_of(items: &[Item], iids: &[u64]) -> HashMap<u64, Linked> {
    let by: HashMap<u64, &Item> = items.iter().map(|it| (it.iid, it)).collect();
    iids.iter()
        .filter_map(|n| by.get(n))
        .map(|it| {
            let linked = it
                .link_ends()
                .iter()
                .filter_map(|l| by.get(l))
                .map(|l| (*l).clone())
                .collect();
            (
                it.iid,
                Linked {
                    item: (*it).clone(),
                    linked,
                },
            )
        })
        .collect()
}
