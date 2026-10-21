//! The tracker port: everything the core asks of a tracker.

use crate::item::Item;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    /// Reading the source failed (a file, later the network).
    Source(String),
    /// The source was read but its content is not a tracker.
    Malformed(String),
    NotFound(u64),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Source(m) => write!(f, "cannot read the tracker: {m}"),
            Error::Malformed(m) => write!(f, "not a tracker snapshot: {m}"),
            Error::NotFound(n) => write!(f, "#{n} is not in the tracker"),
        }
    }
}

impl std::error::Error for Error {}

pub trait Tracker {
    fn all_items(&self) -> Result<Vec<Item>, Error>;
}

/// An in-memory tracker for tests and for callers that already hold the items.
pub struct FakeTracker {
    pub items: Vec<Item>,
}

impl Tracker for FakeTracker {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        Ok(self.items.clone())
    }
}
