//! An item of a tracker: one GitLab Issue, as gltree sees it.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Open,
    Closed,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Open => "open",
            State::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Item {
    pub iid: u64,
    pub title: String,
    pub state: State,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub labels: Vec<String>,
    /// The other ends of this item's links; empty when the source did not record them.
    #[serde(default)]
    pub links: Vec<u64>,
}

impl Item {
    pub fn new(iid: u64, state: State, title: &str, desc: &str) -> Self {
        Item {
            iid,
            title: title.into(),
            state,
            desc: desc.into(),
            labels: vec![],
            links: vec![],
        }
    }
}
