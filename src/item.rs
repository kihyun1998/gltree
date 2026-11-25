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

/// A comment a person wrote on an item; system notes are not comments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Comment {
    /// The author's username; `None` when the server names none.
    #[serde(default)]
    pub author: Option<String>,
    /// When it was written, as the server's timestamp.
    pub created_at: String,
    pub body: String,
    /// Visible to project members only.
    pub internal: bool,
    /// The body as the server renders it to HTML; `None` when the read did not ask for it.
    #[serde(skip)]
    pub body_html: Option<String>,
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
    /// Oldest first; `None` when the source did not record comments at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<Vec<Comment>>,
    /// The description as the server renders it to HTML; `None` when the read did not ask for it.
    #[serde(skip)]
    pub desc_html: Option<String>,
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
            comments: None,
            desc_html: None,
        }
    }

    pub fn link_ends(&self) -> &[u64] {
        self.links.as_deref().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rendered_html_is_never_written_out_nor_read_back() {
        let mut item = Item::new(1, State::Open, "t", "body");
        item.desc_html = Some("<p>body</p>".into());
        item.comments = Some(vec![Comment {
            author: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            body: "c".into(),
            internal: false,
            body_html: Some("<p>c</p>".into()),
        }]);
        let text = serde_json::to_string(&item).unwrap();
        assert!(!text.contains("html") && !text.contains("<p>"), "{text}");
        let back: Item = serde_json::from_str(
            r#"{"iid":1,"title":"t","state":"open","desc_html":"<p>x</p>","comments":[{"created_at":"x","body":"c","internal":false,"body_html":"<p>c</p>"}]}"#,
        )
        .unwrap();
        assert_eq!(back.desc_html, None);
        assert_eq!(back.comments.unwrap()[0].body_html, None);
    }
}
