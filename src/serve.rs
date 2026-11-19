//! The local viewer: a page and a small JSON API over the tracker, on 127.0.0.1 only.

use crate::config::RepoConfig;
use crate::tracker::{Error, Tracker};
use crate::tree;
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// The version of the JSON shapes this module answers with, besides `/api/tree`'s, which is `tree`'s.
pub const JSON_VERSION: u32 = 1;
pub const DEFAULT_PORT: u16 = 7420;
const PAGE: &str = include_str!("serve.html");
const HEAD_LIMIT: usize = 16 * 1024;
const BODY_LIMIT: u64 = 64 * 1024;

/// Opens the tracker for one read; called once per request that reads it.
pub type Open = Box<dyn Fn() -> Result<Box<dyn Tracker>, Error> + Send + Sync>;

pub struct App {
    open: Open,
    walk: bool,
    config: RepoConfig,
    port: u16,
    families: Mutex<Value>,
    reading: Mutex<()>,
}

#[derive(Debug, PartialEq, Eq)]
struct Request<'a> {
    method: &'a str,
    path: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
    site: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
struct Head<'a> {
    method: &'a str,
    path: &'a str,
    host: Option<&'a str>,
    origin: Option<&'a str>,
    site: Option<&'a str>,
    length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    TooLarge,
    BadGateway,
}

impl Status {
    fn code(self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::BadRequest => 400,
            Status::Forbidden => 403,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::TooLarge => 413,
            Status::BadGateway => 502,
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::BadRequest => "Bad Request",
            Status::Forbidden => "Forbidden",
            Status::NotFound => "Not Found",
            Status::MethodNotAllowed => "Method Not Allowed",
            Status::TooLarge => "Content Too Large",
            Status::BadGateway => "Bad Gateway",
        }
    }
}

#[derive(Debug)]
struct Response {
    status: Status,
    content_type: &'static str,
    body: Vec<u8>,
    allow: Option<&'static str>,
}

impl App {
    /// An app with nothing read yet; `refresh` makes the first read.
    pub fn new(open: Open, walk: bool, config: RepoConfig, port: u16) -> App {
        App {
            open,
            walk,
            config,
            port,
            families: Mutex::new(Value::Null),
            reading: Mutex::new(()),
        }
    }

    /// Reads every family again and keeps the answer `/api/families` gives.
    pub fn refresh(&self) -> Result<Value, Error> {
        let _one_at_a_time = self.reading.lock().unwrap_or_else(|p| p.into_inner());
        let tracker = (self.open)()?;
        let nodes = tree::tree(tracker.as_ref(), self.config.scheme().as_ref(), None)?;
        let v = json!({
            "version": JSON_VERSION,
            "read_at": utc(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()),
            "badges": self.config.badges,
            "tree": tree::to_json(&nodes),
        });
        *self.families() = v.clone();
        Ok(v)
    }

    fn families(&self) -> std::sync::MutexGuard<'_, Value> {
        self.families.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn tree_of(&self, n: u64) -> Result<Value, Error> {
        let tracker = (self.open)()?;
        let scheme = self.config.scheme();
        let nodes = if self.walk {
            tree::live_tree(tracker.as_ref(), scheme.as_ref(), n)?
        } else {
            tree::tree(tracker.as_ref(), scheme.as_ref(), Some(n))?
        };
        Ok(tree::to_json(&nodes))
    }

    fn ours(&self, authority: &str) -> bool {
        let (name, port) = match authority.rsplit_once(':') {
            Some((name, port)) => (name, port.parse().ok()),
            None => (authority, Some(80)),
        };
        port == Some(self.port)
            && ["127.0.0.1", "localhost"]
                .iter()
                .any(|n| n.eq_ignore_ascii_case(name))
    }

    fn handle(&self, req: &Request) -> Response {
        if !req.host.is_some_and(|h| self.ours(h)) {
            return error(
                Status::Forbidden,
                "this server answers only to 127.0.0.1 and localhost",
            );
        }
        let from_us = req.origin.is_none_or(|o| {
            o.strip_prefix("http://")
                .is_some_and(|authority| self.ours(authority))
        });
        let ours_site = req.site.is_none_or(|s| s == "same-origin" || s == "none");
        if req.path.starts_with("/api/") && !ours_site {
            return error(
                Status::Forbidden,
                "the API answers only this server's own page",
            );
        }
        let get = req.method == "GET" || req.method == "HEAD";
        let post = req.method == "POST";
        const READ: &str = "GET, HEAD";
        match req.path {
            "/" if get => Response {
                status: Status::Ok,
                content_type: "text/html; charset=utf-8",
                body: PAGE.as_bytes().to_vec(),
                allow: None,
            },
            "/api/families" if get => ok(&self.families()),
            "/api/refresh" if post && !from_us => error(
                Status::Forbidden,
                "a refresh is taken only from this server's own page",
            ),
            "/api/refresh" if post => match self.refresh() {
                Ok(v) => ok(&v),
                Err(e) => error(Status::BadGateway, &e.to_string()),
            },
            "/" | "/api/families" => method_not_allowed(READ),
            "/api/refresh" => method_not_allowed("POST"),
            path => match path.strip_prefix("/api/tree/") {
                None => error(Status::NotFound, "no such page"),
                Some(_) if !get => method_not_allowed(READ),
                Some(n) => match n.parse() {
                    Err(_) => error(Status::NotFound, "not an item number"),
                    Ok(n) => match self.tree_of(n) {
                        Ok(v) => ok(&v),
                        Err(e @ Error::NotFound(_)) => error(Status::NotFound, &e.to_string()),
                        Err(e) => error(Status::BadGateway, &e.to_string()),
                    },
                },
            },
        }
    }
}

fn json_response(status: Status, v: &Value) -> Response {
    Response {
        status,
        content_type: "application/json",
        body: serde_json::to_vec(v).expect("a JSON value serialises"),
        allow: None,
    }
}

fn ok(v: &Value) -> Response {
    json_response(Status::Ok, v)
}

fn error(status: Status, message: &str) -> Response {
    json_response(
        status,
        &json!({ "version": JSON_VERSION, "error": message }),
    )
}

fn method_not_allowed(allow: &'static str) -> Response {
    Response {
        allow: Some(allow),
        ..error(Status::MethodNotAllowed, "method not allowed")
    }
}

fn utc(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// A listener on 127.0.0.1; port 0 takes a free one.
pub fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
}

/// Answers every connection on its own thread, until the process ends.
pub fn serve(listener: TcpListener, app: Arc<App>) -> io::Result<()> {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        let app = Arc::clone(&app);
        std::thread::spawn(move || {
            let _ = answer(stream, &app);
        });
    }
    Ok(())
}

fn answer(stream: TcpStream, app: &App) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let before = head.len();
        reader
            .by_ref()
            .take((HEAD_LIMIT - head.len()) as u64)
            .read_until(b'\n', &mut head)?;
        if head.len() == before || head.len() >= HEAD_LIMIT {
            return write(
                &stream,
                &error(Status::BadRequest, "a request head gltree cannot read"),
                false,
            );
        }
    }
    let head = String::from_utf8_lossy(&head);
    let Ok(h) = parse_head(&head) else {
        return write(
            &stream,
            &error(Status::BadRequest, "a request head gltree cannot read"),
            false,
        );
    };
    if h.length > BODY_LIMIT {
        return write(
            &stream,
            &error(Status::TooLarge, "a request body gltree does not take"),
            false,
        );
    }
    io::copy(&mut reader.by_ref().take(h.length), &mut io::sink())?;
    let res = app.handle(&Request {
        method: h.method,
        path: h.path,
        host: h.host,
        origin: h.origin,
        site: h.site,
    });
    write(&stream, &res, h.method == "HEAD")
}

fn parse_head(head: &str) -> Result<Head<'_>, ()> {
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (method, target) = (
        first.next().unwrap_or_default(),
        first.next().unwrap_or_default(),
    );
    let mut h = Head {
        method,
        path: target.split(['?', '#']).next().unwrap_or_default(),
        host: None,
        origin: None,
        site: None,
        length: 0,
    };
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "host" if h.host.is_some() => return Err(()),
            "host" => h.host = Some(value),
            "origin" => h.origin = Some(value),
            "sec-fetch-site" => h.site = Some(value),
            "content-length" => h.length = value.parse().unwrap_or(u64::MAX),
            _ => {}
        }
    }
    Ok(h)
}

fn write(mut stream: &TcpStream, res: &Response, head_only: bool) -> io::Result<()> {
    let allow = res
        .allow
        .map_or(String::new(), |a| format!("Allow: {a}\r\n"));
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{allow}Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\nConnection: close\r\n\r\n",
        res.status.code(),
        res.status.reason(),
        res.content_type,
        res.body.len()
    );
    stream.write_all(head.as_bytes())?;
    if !head_only {
        stream.write_all(&res.body)?;
    }
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Badge;
    use crate::item::{Item, State};
    use crate::tracker::FakeTracker;

    fn items() -> Vec<Item> {
        let mut root = Item::new(1, State::Open, "root", "+++\nchildren = [2]\n+++\n");
        root.labels = vec!["SPEC".into(), "reg::done".into()];
        root.links = Some(vec![2]);
        let mut child = Item::new(2, State::Closed, "child", "+++\nparent = 1\n+++\n");
        child.links = Some(vec![1]);
        let finished = Item::new(3, State::Closed, "finished", "");
        vec![root, child, finished]
    }

    fn app(held: Arc<Mutex<Vec<Item>>>) -> App {
        let mut config = RepoConfig::defaults();
        config.badges = vec![Badge {
            prefix: "reg::".into(),
            name: "Registration".into(),
            extra: None,
            missing: Some("undecided".into()),
        }];
        let reads = Arc::clone(&held);
        let open: Open = Box::new(move || {
            Ok(Box::new(FakeTracker::new(reads.lock().unwrap().clone())) as Box<dyn Tracker>)
        });
        App::new(open, false, config, 7420)
    }

    fn get<'a>(method: &'a str, path: &'a str) -> Request<'a> {
        Request {
            method,
            path,
            host: Some("127.0.0.1:7420"),
            origin: None,
            site: None,
        }
    }

    fn body(r: &Response) -> Value {
        serde_json::from_slice(&r.body).unwrap()
    }

    fn numbers(v: &Value) -> Vec<u64> {
        v["tree"]["roots"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["number"].as_u64().unwrap())
            .collect()
    }

    #[test]
    fn families_carry_every_family_the_read_time_and_the_badges() {
        let a = app(Arc::new(Mutex::new(items())));
        a.refresh().unwrap();
        let r = a.handle(&get("GET", "/api/families"));
        assert_eq!((r.status.code(), r.content_type), (200, "application/json"));
        let v = body(&r);
        assert_eq!(v["version"], 1);
        assert_eq!(v["tree"]["version"], 3);
        assert_eq!(numbers(&v), [1, 3]);
        assert_eq!(
            v["tree"]["roots"][0]["labels"],
            json!(["SPEC", "reg::done"])
        );
        assert_eq!(
            v["badges"],
            json!([{"prefix": "reg::", "name": "Registration", "extra": null, "missing": "undecided"}])
        );
        let at = v["read_at"].as_str().unwrap();
        assert!(
            at.len() == 20 && at.ends_with('Z') && at.as_bytes()[10] == b'T',
            "{at}"
        );
    }

    #[test]
    fn families_are_the_last_read_until_a_refresh_reads_again() {
        let held = Arc::new(Mutex::new(items()));
        let a = app(Arc::clone(&held));
        a.refresh().unwrap();
        held.lock()
            .unwrap()
            .push(Item::new(9, State::Open, "new", ""));
        assert_eq!(
            numbers(&body(&a.handle(&get("GET", "/api/families")))),
            [1, 3]
        );
        let r = a.handle(&get("POST", "/api/refresh"));
        assert_eq!(r.status.code(), 200);
        assert_eq!(numbers(&body(&r)), [1, 3, 9]);
        assert_eq!(
            numbers(&body(&a.handle(&get("GET", "/api/families")))),
            [1, 3, 9]
        );
    }

    #[test]
    fn a_tree_is_read_fresh_and_an_unknown_number_is_404() {
        let held = Arc::new(Mutex::new(items()));
        let a = app(Arc::clone(&held));
        held.lock().unwrap()[1].title = "renamed".into();
        let r = a.handle(&get("GET", "/api/tree/1"));
        assert_eq!(r.status.code(), 200);
        let v = body(&r);
        assert_eq!(v["version"], 3);
        assert_eq!(v["roots"][0]["children"][0]["title"], "renamed");
        for path in ["/api/tree/42", "/api/tree/x", "/api/tree/"] {
            let r = a.handle(&get("GET", path));
            assert_eq!(r.status.code(), 404, "{path}");
            assert!(body(&r)["error"].is_string(), "{path}");
        }
    }

    #[test]
    fn a_failed_read_is_a_502_with_the_reason() {
        let open: Open = Box::new(|| Err(Error::Source("no login for host".into())));
        let a = App::new(open, true, RepoConfig::defaults(), 7420);
        for (method, path) in [("POST", "/api/refresh"), ("GET", "/api/tree/1")] {
            let r = a.handle(&get(method, path));
            assert_eq!(r.status.code(), 502);
            assert!(
                body(&r)["error"]
                    .as_str()
                    .unwrap()
                    .contains("no login for host")
            );
        }
    }

    #[test]
    fn another_host_is_refused_before_anything_is_read() {
        let a = app(Arc::new(Mutex::new(items())));
        a.refresh().unwrap();
        for host in [
            None,
            Some("evil.example:7420"),
            Some("127.0.0.1:7421"),
            Some("127.0.0.1"),
        ] {
            let r = a.handle(&Request {
                host,
                ..get("GET", "/api/families")
            });
            assert_eq!(r.status.code(), 403, "{host:?}");
        }
        let r = a.handle(&Request {
            host: Some("localhost:7420"),
            ..get("GET", "/api/families")
        });
        assert_eq!(r.status.code(), 200);
    }

    #[test]
    fn a_refresh_from_another_origin_is_refused_and_reads_nothing() {
        let held = Arc::new(Mutex::new(items()));
        let a = app(Arc::clone(&held));
        a.refresh().unwrap();
        held.lock()
            .unwrap()
            .push(Item::new(9, State::Open, "new", ""));
        let r = a.handle(&Request {
            origin: Some("https://evil.example"),
            ..get("POST", "/api/refresh")
        });
        assert_eq!(r.status.code(), 403);
        assert_eq!(
            numbers(&body(&a.handle(&get("GET", "/api/families")))),
            [1, 3]
        );
        let r = a.handle(&Request {
            origin: Some("http://127.0.0.1:7420"),
            ..get("POST", "/api/refresh")
        });
        assert_eq!(r.status.code(), 200);
    }

    #[test]
    fn the_page_is_html_and_fetches_nothing_from_outside() {
        let r = app(Arc::new(Mutex::new(items()))).handle(&get("GET", "/"));
        assert_eq!(
            (r.status.code(), r.content_type),
            (200, "text/html; charset=utf-8")
        );
        let page = String::from_utf8(r.body).unwrap();
        assert!(page.starts_with("<!doctype html>"));
        assert!(!page.contains("://"), "the page names an outside address");
    }

    #[test]
    fn a_wrong_method_is_405_and_an_unknown_path_404() {
        let a = app(Arc::new(Mutex::new(items())));
        for (method, path, status) in [
            ("POST", "/", 405),
            ("GET", "/api/refresh", 405),
            ("DELETE", "/api/families", 405),
            ("POST", "/api/tree/1", 405),
            ("GET", "/api/other", 404),
            ("GET", "/index.html", 404),
        ] {
            assert_eq!(
                a.handle(&get(method, path)).status.code(),
                status,
                "{method} {path}"
            );
        }
    }

    #[test]
    fn against_the_server_a_tree_walks_from_its_root_instead_of_reading_every_item() {
        let at = |walk: bool| {
            let open: Open = Box::new(|| {
                let fake = FakeTracker::new(items());
                fake.fail_all_items.set(true);
                Ok(Box::new(fake) as Box<dyn Tracker>)
            });
            App::new(open, walk, RepoConfig::defaults(), 7420).handle(&get("GET", "/api/tree/1"))
        };
        let walked = at(true);
        assert_eq!(walked.status, Status::Ok);
        assert_eq!(body(&walked)["roots"][0]["children"][0]["number"], 2);
        assert_eq!(at(false).status, Status::BadGateway);
    }

    #[test]
    fn on_port_80_a_host_without_a_port_is_ours_and_case_does_not_matter() {
        let open: Open = Box::new(|| Err(Error::Source("unused".into())));
        let a = App::new(open, false, RepoConfig::defaults(), 80);
        for host in ["127.0.0.1", "localhost", "127.0.0.1:80", "LOCALHOST"] {
            let r = a.handle(&Request {
                host: Some(host),
                ..get("GET", "/")
            });
            assert_eq!(r.status, Status::Ok, "{host}");
        }
        let b = app(Arc::new(Mutex::new(items())));
        let r = b.handle(&Request {
            host: Some("LocalHost:7420"),
            ..get("GET", "/")
        });
        assert_eq!(r.status, Status::Ok);
    }

    #[test]
    fn a_cross_site_call_to_the_api_is_refused_before_anything_is_read_but_the_page_opens() {
        let open: Open = Box::new(|| panic!("read on a cross-site call"));
        let a = App::new(open, true, RepoConfig::defaults(), 7420);
        for site in ["cross-site", "same-site"] {
            for (method, path) in [
                ("GET", "/api/tree/1"),
                ("GET", "/api/families"),
                ("POST", "/api/refresh"),
            ] {
                let r = a.handle(&Request {
                    site: Some(site),
                    ..get(method, path)
                });
                assert_eq!(r.status, Status::Forbidden, "{site} {method} {path}");
            }
        }
        let r = a.handle(&Request {
            site: Some("cross-site"),
            ..get("GET", "/")
        });
        assert_eq!(r.status, Status::Ok);
        let r = a.handle(&Request {
            site: Some("same-origin"),
            ..get("GET", "/api/families")
        });
        assert_eq!(r.status, Status::Ok);
    }

    #[test]
    fn a_405_names_the_allowed_methods_and_head_answers_like_get() {
        let a = app(Arc::new(Mutex::new(items())));
        a.refresh().unwrap();
        for (method, path, allow) in [
            ("POST", "/", "GET, HEAD"),
            ("GET", "/api/refresh", "POST"),
            ("POST", "/api/tree/1", "GET, HEAD"),
        ] {
            let r = a.handle(&get(method, path));
            assert_eq!(
                (r.status, r.allow),
                (Status::MethodNotAllowed, Some(allow)),
                "{method} {path}"
            );
        }
        for path in ["/", "/api/families", "/api/tree/1"] {
            assert_eq!(a.handle(&get("HEAD", path)).status, Status::Ok, "{path}");
        }
        assert_eq!(a.handle(&get("GET", "/")).allow, None);
    }

    #[test]
    fn the_head_is_read_into_its_fields_and_a_second_host_is_refused() {
        let h = parse_head(
            "POST /api/refresh?x=1 HTTP/1.1\r\nHost: 127.0.0.1:7420\r\norigin: http://127.0.0.1:7420\r\nSec-Fetch-Site: same-origin\r\nContent-Length: 3\r\n\r\n",
        )
        .unwrap();
        assert_eq!(
            (h.method, h.path, h.host, h.origin, h.site, h.length),
            (
                "POST",
                "/api/refresh",
                Some("127.0.0.1:7420"),
                Some("http://127.0.0.1:7420"),
                Some("same-origin"),
                3
            )
        );
        assert!(
            parse_head("GET / HTTP/1.1\r\nHost: evil.example\r\nHost: 127.0.0.1:7420\r\n\r\n")
                .is_err()
        );
    }

    #[test]
    fn a_refresh_that_finishes_late_does_not_put_back_an_older_read() {
        let calls = Arc::new(Mutex::new(0u32));
        let counted = Arc::clone(&calls);
        let open: Open = Box::new(move || {
            let n = {
                let mut c = counted.lock().unwrap();
                *c += 1;
                *c
            };
            let mut held = items();
            if n == 1 {
                std::thread::sleep(Duration::from_millis(300));
            } else {
                held.push(Item::new(9, State::Open, "new", ""));
            }
            Ok(Box::new(crate::tracker::FakeTracker::new(held)) as Box<dyn Tracker>)
        });
        let a = Arc::new(App::new(open, false, RepoConfig::defaults(), 7420));
        let first = {
            let a = Arc::clone(&a);
            std::thread::spawn(move || a.refresh().unwrap())
        };
        std::thread::sleep(Duration::from_millis(50));
        a.refresh().unwrap();
        first.join().unwrap();
        assert_eq!(
            numbers(&body(&a.handle(&get("GET", "/api/families")))),
            [1, 3, 9]
        );
    }

    #[test]
    fn utc_writes_the_calendar_date() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(utc(1_798_761_599), "2026-12-31T23:59:59Z");
    }
}
