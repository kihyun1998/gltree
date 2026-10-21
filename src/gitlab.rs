//! The tracker on a GitLab server, over its GraphQL API.

use crate::item::{Item, State};
use crate::login::Login;
use crate::tracker::{Error, Linked, Tracker};
use serde_json::{Value, json};
use std::collections::HashMap;
use ureq::Agent;
use ureq::tls::{PemItem, RootCerts, TlsConfig, TlsProvider, parse_pem};

/// Item numbers per level request; with 100 linked items each it stays under GitLab's complexity cap.
pub const LEVEL_BATCH: usize = 100;

pub struct GitLab {
    agent: Agent,
    login: Login,
    project: String,
}

impl GitLab {
    pub fn new(login: Login, project: &str) -> Result<GitLab, Error> {
        let mut config = Agent::config_builder().http_status_as_error(false);
        if let Some(path) = &login.ca_cert {
            let pem = std::fs::read(path)
                .map_err(|e| Error::Source(format!("CA certificate {}: {e}", path.display())))?;
            let certs: Vec<_> = parse_pem(&pem)
                .filter_map(|item| match item {
                    Ok(PemItem::Certificate(c)) => Some(c),
                    _ => None,
                })
                .collect();
            if certs.is_empty() {
                return Err(Error::Source(format!(
                    "no PEM certificate in {}",
                    path.display()
                )));
            }
            config = config.tls_config(
                TlsConfig::builder()
                    .provider(TlsProvider::NativeTls)
                    .root_certs(RootCerts::new_with_certs(&certs))
                    .build(),
            );
        }
        Ok(GitLab {
            agent: config.build().into(),
            login,
            project: project.into(),
        })
    }

    fn graphql(&self, query: &str) -> Result<Value, Error> {
        let url = format!("{}/api/graphql", self.login.api_base);
        let body =
            serde_json::to_vec(&json!({ "query": query })).expect("a JSON object serialises");
        let mut resp = self
            .agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.login.token))
            .header("Content-Type", "application/json")
            .send(&body[..])
            .map_err(|e| Error::Source(format!("{url}: {e}")))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_vec()
            .map_err(|e| Error::Source(format!("{url}: {e}")))?;
        answer(status, &bytes)
    }
}

impl Tracker for GitLab {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        Err(Error::Source(
            "reading every item from the server is not built yet; name a root, or pass --snapshot"
                .into(),
        ))
    }

    fn level(&self, iids: &[u64]) -> Result<HashMap<u64, Linked>, Error> {
        let mut out = HashMap::new();
        for chunk in iids.chunks(LEVEL_BATCH) {
            out.extend(parse_level(
                &self.graphql(&level_query(&self.project, chunk))?,
            )?);
        }
        Ok(out)
    }
}

/// The level query, with the item numbers written into it: glab-style array variables do not survive every client.
pub fn level_query(project: &str, iids: &[u64]) -> String {
    let iids = iids
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "query{{project(fullPath:{project}){{workItems(iids:[{iids}],first:{LEVEL_BATCH}){{nodes{{iid title state widgets{{\
         ... on WorkItemWidgetDescription{{description}} \
         ... on WorkItemWidgetLinkedItems{{linkedItems(first:100){{pageInfo{{hasNextPage}} nodes{{workItem{{\
         iid title state widgets{{... on WorkItemWidgetDescription{{description}}}}}}}}}}}}}}}}}}}}}}",
        project = Value::String(project.into())
    )
}

/// A GraphQL answer's `data`, or the server's own error.
pub fn answer(status: u16, body: &[u8]) -> Result<Value, Error> {
    let v: Value = serde_json::from_slice(body).map_err(|_| {
        Error::Server(format!(
            "HTTP {status}: {}",
            String::from_utf8_lossy(&body[..body.len().min(200)])
        ))
    })?;
    if let Some(errs) = v
        .get("errors")
        .and_then(Value::as_array)
        .filter(|e| !e.is_empty())
    {
        let msgs: Vec<&str> = errs
            .iter()
            .filter_map(|e| e.get("message").and_then(Value::as_str))
            .collect();
        return Err(Error::Server(msgs.join("; ")));
    }
    if !(200..300).contains(&status) {
        let msg = v
            .get("message")
            .or(v.get("error"))
            .map(Value::to_string)
            .unwrap_or_default();
        return Err(Error::Server(format!("HTTP {status} {msg}")));
    }
    v.get("data")
        .cloned()
        .ok_or_else(|| Error::Server("an answer without data".into()))
}

/// The items of a level answer, each with its linked items.
pub fn parse_level(data: &Value) -> Result<HashMap<u64, Linked>, Error> {
    let nodes = data
        .pointer("/project/workItems/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            Error::Source("the project is not on the server, or this token cannot see it".into())
        })?;
    let mut out = HashMap::new();
    for n in nodes {
        let item = brief(n)?;
        let links = n["widgets"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|w| w.get("linkedItems"))
            .cloned()
            .unwrap_or(json!({ "nodes": [] }));
        if links
            .pointer("/pageInfo/hasNextPage")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(Error::Source(format!(
                "#{} has more than 100 links; draw it from a snapshot instead",
                item.iid
            )));
        }
        let linked = links["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| brief(&x["workItem"]))
            .collect::<Result<_, _>>()?;
        out.insert(item.iid, Linked { item, linked });
    }
    Ok(out)
}

fn brief(n: &Value) -> Result<Item, Error> {
    let bad = || Error::Malformed(format!("a work item without iid/title/state: {n}"));
    let iid = n["iid"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(bad)?;
    let title = n["title"].as_str().ok_or_else(bad)?;
    let state = match n["state"].as_str() {
        Some("OPEN") => State::Open,
        Some("CLOSED") => State::Closed,
        _ => return Err(bad()),
    };
    let desc = n["widgets"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|w| w.get("description").and_then(Value::as_str))
        .unwrap_or("");
    Ok(Item::new(iid, state, title, desc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wi(iid: u64, state: &str, desc: &str, links: Option<(Vec<Value>, bool)>) -> Value {
        let mut widgets = vec![json!({ "description": desc })];
        if let Some((nodes, more)) = links {
            widgets.push(
                json!({ "linkedItems": { "pageInfo": { "hasNextPage": more }, "nodes": nodes } }),
            );
        }
        json!({ "iid": iid.to_string(), "title": format!("t{iid}"), "state": state, "widgets": widgets })
    }

    fn level_answer(nodes: Vec<Value>) -> Value {
        json!({ "project": { "workItems": { "nodes": nodes } } })
    }

    #[test]
    fn the_query_names_the_project_and_writes_the_item_numbers_in() {
        let q = level_query("team/app", &[3, 12]);
        assert!(q.contains(r#"project(fullPath:"team/app")"#), "{q}");
        assert!(q.contains(r#"workItems(iids:["3","12"],first:100)"#), "{q}");
        assert!(
            q.contains("linkedItems(first:100){pageInfo{hasNextPage}"),
            "{q}"
        );
        assert_eq!(q.matches('{').count(), q.matches('}').count(), "{q}");
    }

    #[test]
    fn a_level_answer_gives_each_item_with_its_linked_items_and_bodies() {
        let child = json!({ "workItem": wi(5, "CLOSED", "Parent: #3\n\nx", None) });
        let got = parse_level(&level_answer(vec![wi(
            3,
            "OPEN",
            "",
            Some((vec![child], false)),
        )]))
        .unwrap();
        let l = &got[&3];
        assert_eq!((l.item.iid, l.item.state), (3, State::Open));
        assert_eq!(
            l.linked,
            vec![Item::new(5, State::Closed, "t5", "Parent: #3\n\nx")]
        );
    }

    #[test]
    fn more_links_than_one_page_is_an_error_not_a_short_tree() {
        let r = parse_level(&level_answer(vec![wi(3, "OPEN", "", Some((vec![], true)))]));
        assert!(matches!(r, Err(Error::Source(m)) if m.contains("#3")));
    }

    #[test]
    fn an_unknown_project_is_reported() {
        assert!(matches!(
            parse_level(&json!({ "project": null })),
            Err(Error::Source(_))
        ));
    }

    #[test]
    fn graphql_errors_and_refusals_are_server_errors() {
        let complexity = br#"{"errors":[{"message":"Query has complexity of 577, which exceeds max complexity of 250"}]}"#;
        assert!(
            matches!(answer(200, complexity), Err(Error::Server(m)) if m.contains("complexity"))
        );
        assert!(
            matches!(answer(401, br#"{"message":"401 Unauthorized"}"#), Err(Error::Server(m)) if m.contains("401"))
        );
        assert!(matches!(answer(429, b"Retry later"), Err(Error::Server(m)) if m.contains("429")));
        assert_eq!(
            answer(200, br#"{"data":{"x":1}}"#).unwrap(),
            json!({ "x": 1 })
        );
    }
}
