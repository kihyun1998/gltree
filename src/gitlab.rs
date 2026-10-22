//! The tracker on a GitLab server, over its GraphQL API.

use crate::item::{Item, State};
use crate::login::Login;
use crate::tracker::{Created, Error, Linked, Tracker};
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
        let (status, bytes) = self.call(Method::Post, &url, Some(&json!({ "query": query })))?;
        answer(status, &bytes)
    }

    /// A REST call under this project, e.g. `issues`; the status and the body.
    fn rest(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Vec<u8>), Error> {
        let url = format!(
            "{}/api/v4/projects/{}/{path}",
            self.login.api_base,
            encode_path(&self.project)
        );
        self.call(method, &url, body)
    }

    fn call(
        &self,
        method: Method,
        url: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Vec<u8>), Error> {
        let auth = format!("Bearer {}", self.login.token);
        let json = body
            .map(|b| serde_json::to_vec(b).expect("a JSON value serialises"))
            .unwrap_or_default();
        let sent = match method {
            Method::Get => self.agent.get(url).header("Authorization", &auth).call(),
            Method::Delete => self.agent.delete(url).header("Authorization", &auth).call(),
            Method::Post => self
                .agent
                .post(url)
                .header("Authorization", &auth)
                .header("Content-Type", "application/json")
                .send(&json[..]),
            Method::Put => self
                .agent
                .put(url)
                .header("Authorization", &auth)
                .header("Content-Type", "application/json")
                .send(&json[..]),
        };
        let mut resp = sent.map_err(|e| Error::Source(format!("{url}: {e}")))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_vec()
            .map_err(|e| Error::Source(format!("{url}: {e}")))?;
        Ok((status, bytes))
    }
}

impl Tracker for GitLab {
    fn all_items(&self) -> Result<Vec<Item>, Error> {
        let (mut items, mut after) = (Vec::new(), None);
        loop {
            let (page, next) =
                parse_page(&self.graphql(&page_query(&self.project, after.as_deref()))?)?;
            items.extend(page);
            match next {
                Some(cursor) => after = Some(cursor),
                None => return Ok(items),
            }
        }
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

    fn create_item(&self, title: &str, desc: &str, labels: &[String]) -> Result<Created, Error> {
        let body = json!({ "title": title, "description": desc, "labels": labels.join(",") });
        let (status, bytes) = self.rest(Method::Post, "issues", Some(&body))?;
        created(status, &bytes)
    }

    fn add_link(&self, a: u64, b: u64) -> Result<(), Error> {
        let body = json!({ "target_project_id": self.project, "target_issue_iid": b, "link_type": "relates_to" });
        let (status, bytes) = self.rest(Method::Post, &format!("issues/{a}/links"), Some(&body))?;
        linked(status, &bytes)
    }

    fn set_desc(&self, iid: u64, desc: &str) -> Result<(), Error> {
        let (status, bytes) = self.rest(
            Method::Put,
            &format!("issues/{iid}"),
            Some(&json!({ "description": desc })),
        )?;
        created(status, &bytes).map(|_| ())
    }

    fn remove_link(&self, a: u64, b: u64) -> Result<(), Error> {
        let (status, bytes) = self.rest(Method::Get, &format!("issues/{a}/links"), None)?;
        let Some(id) = link_id(status, &bytes, b)? else {
            return Ok(());
        };
        let (status, bytes) = self.rest(Method::Delete, &format!("issues/{a}/links/{id}"), None)?;
        linked(status, &bytes)
    }
}

#[derive(Clone, Copy)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
}

/// The id of the link from an item to `other`, from its links answer; `None` when there is none.
pub fn link_id(status: u16, body: &[u8], other: u64) -> Result<Option<u64>, Error> {
    if !(200..300).contains(&status) {
        return Err(Error::Server(format!("HTTP {status} reading links")));
    }
    let v: Value =
        serde_json::from_slice(body).map_err(|e| Error::Server(format!("a links answer: {e}")))?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .find(|l| l["iid"].as_u64() == Some(other))
        .and_then(|l| l["issue_link_id"].as_u64()))
}

/// A project path as one URL segment: `group/app` → `group%2Fapp`.
pub fn encode_path(path: &str) -> String {
    path.bytes()
        .map(|c| match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (c as char).to_string()
            }
            _ => format!("%{c:02X}"),
        })
        .collect()
}

/// The item a create answered with, or the server's refusal.
pub fn created(status: u16, body: &[u8]) -> Result<Created, Error> {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(Error::Server(format!(
            "HTTP {status} {}",
            v.get("message").map(Value::to_string).unwrap_or_default()
        )));
    }
    let iid = v["iid"]
        .as_u64()
        .ok_or_else(|| Error::Server(format!("a created issue without iid: {v}")))?;
    Ok(Created {
        iid,
        url: v["web_url"].as_str().unwrap_or_default().into(),
    })
}

/// A link answer: created, or already there (409), or refused.
pub fn linked(status: u16, body: &[u8]) -> Result<(), Error> {
    if (200..300).contains(&status) || status == 409 {
        return Ok(());
    }
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    Err(Error::Server(format!(
        "HTTP {status} {}",
        v.get("message").map(Value::to_string).unwrap_or_default()
    )))
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

/// One page of every work item, after `after` (a cursor from the previous page).
pub fn page_query(project: &str, after: Option<&str>) -> String {
    let after = after
        .map(|a| format!(",after:{}", Value::String(a.into())))
        .unwrap_or_default();
    format!(
        "query{{project(fullPath:{project}){{workItems(first:100{after}){{pageInfo{{hasNextPage endCursor}}          nodes{{iid title state closedAt webUrl workItemType{{name}} widgets{{         ... on WorkItemWidgetDescription{{description}}          ... on WorkItemWidgetLabels{{labels{{nodes{{title}}}}}}          ... on WorkItemWidgetLinkedItems{{linkedItems(first:100){{pageInfo{{hasNextPage}} nodes{{workItem{{iid}}}}}}}}}}}}}}}}}}",
        project = Value::String(project.into())
    )
}

/// The items of a page with their type, labels and link ends, and the cursor of the next page.
pub fn parse_page(data: &Value) -> Result<(Vec<Item>, Option<String>), Error> {
    let wi = data.pointer("/project/workItems").ok_or_else(|| {
        Error::Source("the project is not on the server, or this token cannot see it".into())
    })?;
    let mut items = Vec::new();
    for n in wi["nodes"].as_array().into_iter().flatten() {
        let mut item = brief(n)?;
        item.kind = n
            .pointer("/workItemType/name")
            .and_then(Value::as_str)
            .unwrap_or("Issue")
            .to_lowercase();
        item.closed_at = n["closedAt"].as_str().map(String::from);
        item.url = n["webUrl"].as_str().map(String::from);
        let widgets: Vec<&Value> = n["widgets"].as_array().into_iter().flatten().collect();
        item.labels = widgets
            .iter()
            .filter_map(|w| w.pointer("/labels/nodes").and_then(Value::as_array))
            .flatten()
            .filter_map(|l| l["title"].as_str().map(String::from))
            .collect();
        let links = widgets.iter().find_map(|w| w.get("linkedItems"));
        if links
            .and_then(|l| l.pointer("/pageInfo/hasNextPage"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Err(Error::Source(format!(
                "#{} has more than 100 links; check cannot see them all",
                item.iid
            )));
        }
        item.links = Some(
            links
                .and_then(|l| l["nodes"].as_array())
                .into_iter()
                .flatten()
                .filter_map(|x| {
                    x.pointer("/workItem/iid")
                        .and_then(Value::as_str)?
                        .parse()
                        .ok()
                })
                .collect(),
        );
        items.push(item);
    }
    let next = (wi.pointer("/pageInfo/hasNextPage").and_then(Value::as_bool) == Some(true))
        .then(|| {
            wi.pointer("/pageInfo/endCursor")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .flatten();
    Ok((items, next))
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
    fn a_page_gives_type_labels_and_link_ends_and_the_next_cursor() {
        let node = json!({
            "iid": "7", "title": "t7", "state": "OPEN", "workItemType": { "name": "Task" },
            "widgets": [
                { "description": "Parent: #3" },
                { "labels": { "nodes": [{ "title": "SPEC" }, { "title": "bug" }] } },
                { "linkedItems": { "pageInfo": { "hasNextPage": false }, "nodes": [{ "workItem": { "iid": "3" } }] } }
            ]
        });
        let data = json!({ "project": { "workItems": {
            "pageInfo": { "hasNextPage": true, "endCursor": "abc" }, "nodes": [node] } } });
        let (items, next) = parse_page(&data).unwrap();
        assert_eq!(next.as_deref(), Some("abc"));
        let it = &items[0];
        assert_eq!((it.kind.as_str(), it.desc.as_str()), ("task", "Parent: #3"));
        assert_eq!(
            (it.labels.clone(), it.links.clone()),
            (vec!["SPEC".to_string(), "bug".to_string()], Some(vec![3]))
        );
    }

    #[test]
    fn the_last_page_has_no_cursor_and_an_item_without_links_records_none_linked() {
        let data = json!({ "project": { "workItems": {
            "pageInfo": { "hasNextPage": false, "endCursor": "zzz" },
            "nodes": [{ "iid": "1", "title": "t1", "state": "CLOSED", "workItemType": { "name": "Issue" },
                        "closedAt": "2026-10-21T03:34:56Z", "webUrl": "https://gitlab.example.com/team/app/-/issues/1",
                        "widgets": [{ "description": "" }] }] } } });
        let (items, next) = parse_page(&data).unwrap();
        assert_eq!(next, None);
        assert_eq!(
            (items[0].kind.as_str(), items[0].links.clone()),
            ("issue", Some(vec![]))
        );
        assert_eq!(items[0].closed_at.as_deref(), Some("2026-10-21T03:34:56Z"));
        assert_eq!(
            items[0].url.as_deref(),
            Some("https://gitlab.example.com/team/app/-/issues/1")
        );
    }

    #[test]
    fn an_open_item_has_no_closed_at() {
        let data = json!({ "project": { "workItems": { "pageInfo": { "hasNextPage": false }, "nodes": [{
            "iid": "2", "title": "t2", "state": "OPEN", "closedAt": null, "workItemType": { "name": "Issue" },
            "widgets": [] }] } } });
        assert_eq!(parse_page(&data).unwrap().0[0].closed_at, None);
    }

    #[test]
    fn a_page_with_more_links_than_one_page_is_an_error() {
        let data = json!({ "project": { "workItems": { "pageInfo": { "hasNextPage": false }, "nodes": [{
            "iid": "1", "title": "t1", "state": "OPEN", "workItemType": { "name": "Issue" },
            "widgets": [{ "linkedItems": { "pageInfo": { "hasNextPage": true }, "nodes": [] } }] }] } } });
        assert!(parse_page(&data).is_err());
    }

    #[test]
    fn the_page_query_carries_the_cursor_only_after_the_first_page() {
        assert!(!page_query("team/app", None).contains("after:"));
        let q = page_query("team/app", Some("ab\"c"));
        assert!(q.contains(r#"after:"ab\"c""#), "{q}");
        assert_eq!(q.matches('{').count(), q.matches('}').count(), "{q}");
    }

    #[test]
    fn a_project_path_is_one_url_segment() {
        assert_eq!(encode_path("team/sub/app.v2"), "team%2Fsub%2Fapp.v2");
    }

    #[test]
    fn a_create_answer_gives_the_number_and_url_or_the_refusal() {
        let ok = br#"{"iid": 42, "web_url": "https://gitlab.example.com/team/app/-/issues/42"}"#;
        assert_eq!(created(201, ok).unwrap().iid, 42);
        assert!(
            matches!(created(403, br#"{"message":"403 Forbidden"}"#), Err(Error::Server(m)) if m.starts_with("HTTP 403"))
        );
        // a refusal is told by its status, whatever the body carries
        let refused_with_iid = br#"{"iid": 5, "message": "title is too long"}"#;
        assert!(
            matches!(created(400, refused_with_iid), Err(Error::Server(m)) if m.starts_with("HTTP 400"))
        );
        assert!(created(201, b"{}").is_err());
    }

    #[test]
    fn the_link_to_an_item_is_found_by_its_number() {
        let body = br#"[{"iid": 7, "issue_link_id": 31}, {"iid": 9, "issue_link_id": 32}]"#;
        assert_eq!(link_id(200, body, 9).unwrap(), Some(32));
        assert_eq!(link_id(200, body, 8).unwrap(), None);
        assert!(link_id(404, b"{}", 9).is_err());
    }

    #[test]
    fn a_link_that_already_exists_is_success() {
        assert!(linked(201, b"{}").is_ok());
        assert!(linked(409, br#"{"message":"Issue(s) already assigned"}"#).is_ok());
        assert!(matches!(linked(422, b"{}"), Err(Error::Server(m)) if m.contains("422")));
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
