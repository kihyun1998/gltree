//! The tracker port: everything the core asks of a tracker.

use crate::item::Item;
use std::cell::RefCell;
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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Source(m) => write!(f, "cannot read the tracker: {m}"),
            Error::Malformed(m) => write!(f, "not a tracker snapshot: {m}"),
            Error::Server(m) => write!(f, "the server refused: {m}"),
            Error::NotFound(n) => write!(f, "#{n} is not in the tracker"),
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

pub trait Tracker {
    fn all_items(&self) -> Result<Vec<Item>, Error>;
    /// The given items with their linked items, in one request; an absent item is left out.
    fn level(&self, iids: &[u64]) -> Result<HashMap<u64, Linked>, Error>;
}

/// An in-memory tracker for tests and for callers that already hold the items.
/// Its links are each item's `links`; `levels` records every `level` call.
pub struct FakeTracker {
    pub items: Vec<Item>,
    pub levels: RefCell<Vec<Vec<u64>>>,
}

impl FakeTracker {
    pub fn new(items: Vec<Item>) -> Self {
        FakeTracker {
            items,
            levels: RefCell::new(vec![]),
        }
    }
}

impl Tracker for FakeTracker {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        Ok(self.items.clone())
    }

    fn level(&self, iids: &[u64]) -> Result<HashMap<u64, Linked>, Error> {
        let mut asked = iids.to_vec();
        asked.sort();
        self.levels.borrow_mut().push(asked);
        Ok(level_of(&self.items, iids))
    }
}

/// A level taken from items that carry their own `links`.
pub fn level_of(items: &[Item], iids: &[u64]) -> HashMap<u64, Linked> {
    let by: HashMap<u64, &Item> = items.iter().map(|it| (it.iid, it)).collect();
    iids.iter()
        .filter_map(|n| by.get(n))
        .map(|it| {
            let linked = it
                .links
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
