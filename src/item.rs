//! An item of a tracker: one GitLab Issue, as gltree sees it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Item {
    pub iid: u64,
    pub title: String,
    pub state: State,
    /// The GitLab work item type, lowercase: `issue`, or a leftover `task`.
    #[serde(rename = "type", default = "issue")]
    pub kind: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub labels: Vec<String>,
    /// The other ends of this item's links; `None` when the source did not record links at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<u64>>,
    /// When it was closed, as the server's UTC timestamp; `None` when open or not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

fn issue() -> String {
    "issue".into()
}

impl Item {
    pub fn new(iid: u64, state: State, title: &str, desc: &str) -> Self {
        Item {
            iid,
            title: title.into(),
            state,
            kind: issue(),
            desc: desc.into(),
            labels: vec![],
            links: None,
            closed_at: None,
            url: None,
        }
    }

    pub fn link_ends(&self) -> &[u64] {
        self.links.as_deref().unwrap_or_default()
    }
}
