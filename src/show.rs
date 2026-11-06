//! One item as `gltree show` prints it: title, state, labels, body and comments.

use crate::item::{Comment, Item};
use serde_json::{Value, json};

pub const JSON_VERSION: u32 = 1;

/// A line from the tracker with its control characters taken out; a tab stays.
fn clean(line: &str) -> String {
    line.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect()
}

/// Text from the tracker, every line indented, so it cannot read as one of `render`'s own lines.
fn body_lines(text: &str) -> impl Iterator<Item = String> {
    text.lines().map(|l| match clean(l) {
        l if l.is_empty() => l,
        l => format!("  {l}"),
    })
}

/// The text of an item: gltree's own lines at the margin, what people wrote indented under them.
pub fn render(item: &Item) -> Vec<String> {
    let mut lines = vec![format!(
        "#{} [{}] {}",
        item.iid,
        item.state.as_str(),
        clean(&item.title)
    )];
    if !item.labels.is_empty() {
        lines.push(format!("labels: {}", clean(&item.labels.join(", "))));
    }
    if !item.desc.is_empty() {
        lines.push(String::new());
        lines.extend(body_lines(&item.desc));
    }
    lines.push(String::new());
    match item.comments.as_deref() {
        None => lines.push("comments not recorded; take a fresh snapshot".into()),
        Some([]) => lines.push("no comments".into()),
        Some(comments) => {
            let noun = if comments.len() == 1 {
                "comment"
            } else {
                "comments"
            };
            lines.push(format!("{} {noun}", comments.len()));
            for c in comments {
                lines.extend([String::new(), comment_head(c)]);
                lines.extend(body_lines(&c.body));
            }
        }
    }
    lines
}

fn comment_head(c: &Comment) -> String {
    let author = clean(c.author.as_deref().unwrap_or("unknown"));
    let internal = if c.internal { "  [internal]" } else { "" };
    format!("{author}  {}{internal}", clean(&c.created_at))
}

pub fn to_json(item: &Item) -> Value {
    json!({
        "version": JSON_VERSION,
        "number": item.iid,
        "title": item.title,
        "state": item.state.as_str(),
        "labels": item.labels,
        "body": item.desc,
        "comments": item.comments,
    })
}
