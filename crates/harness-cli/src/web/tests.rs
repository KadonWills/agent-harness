//! End-to-end tests against a real server on a free loopback port, using
//! a minimal HTTP/1.1 client so the security headers are exercised as sent.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use super::*;
use crate::git::testing::{repo, write};

struct Reply {
    status: u16,
    headers: String,
    body: String,
}

fn send(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("timeout");
    let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if !body.is_empty() {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    req.push_str("\r\n");
    req.push_str(body);
    stream.write_all(req.as_bytes()).expect("write");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read");
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    Reply {
        status,
        headers: head.to_ascii_lowercase(),
        body: body.to_string(),
    }
}

struct Client {
    server: Server,
    _dir: tempfile::TempDir,
}

impl Client {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = repo(files);
        let server = Server::start(dir.path(), 0).expect("server starts");
        Self { server, _dir: dir }
    }

    fn root(&self) -> &Path {
        self._dir.path()
    }

    fn cookie(&self) -> String {
        format!("{COOKIE}={}", self.server.token)
    }

    fn get(&self, path: &str) -> Reply {
        let cookie = self.cookie();
        send(self.server.port, "GET", path, &[("Cookie", &cookie)], "")
    }

    fn post(&self, path: &str, body: &str) -> Reply {
        let cookie = self.cookie();
        let token = self.server.token.clone();
        send(
            self.server.port,
            "POST",
            path,
            &[("Cookie", &cookie), ("X-Harness-Token", &token)],
            body,
        )
    }
}

#[test]
fn token_login_sets_a_strict_cookie_and_strips_the_url() {
    let c = Client::new(&[("harness.toml", "")]);
    let port = c.server.port;
    let r = send(port, "GET", &format!("/?token={}", c.server.token), &[], "");
    assert_eq!(r.status, 303);
    assert!(r.headers.contains("location: /\r\n"), "{}", r.headers);
    assert!(
        r.headers.contains("httponly; samesite=strict"),
        "{}",
        r.headers
    );

    assert_eq!(send(port, "GET", "/?token=nope", &[], "").status, 401);
    let anon = send(port, "GET", "/history", &[], "");
    assert_eq!(anon.status, 401);
    assert!(anon.body.contains("fix:"), "{}", anon.body);

    let bearer = format!("Bearer {}", c.server.token);
    let r = send(
        port,
        "GET",
        "/api/history",
        &[("Authorization", &bearer)],
        "",
    );
    assert_eq!(r.status, 200);
    assert_eq!(r.body, "[]");
}

#[test]
fn foreign_hosts_and_cross_site_posts_are_refused() {
    let c = Client::new(&[("harness.toml", "")]);
    let port = c.server.port;
    let cookie = c.cookie();
    let rebinding = send(
        port,
        "GET",
        "/api/history",
        &[("Host", "evil.example:80"), ("Cookie", &cookie)],
        "",
    );
    assert_eq!(rebinding.status, 403);
    let localhost = format!("localhost:{port}");
    let ok = send(
        port,
        "GET",
        "/api/history",
        &[("Host", &localhost), ("Cookie", &cookie)],
        "",
    );
    assert_eq!(ok.status, 200);

    // A cookie alone (what a cross-site form would carry) is not enough.
    let no_csrf = send(port, "POST", "/api/run", &[("Cookie", &cookie)], "{}");
    assert_eq!(no_csrf.status, 403);
    let token = c.server.token.clone();
    let cross = send(
        port,
        "POST",
        "/api/run",
        &[
            ("Cookie", &cookie),
            ("X-Harness-Token", &token),
            ("Origin", "http://evil.example"),
        ],
        "{}",
    );
    assert_eq!(cross.status, 403);
}

#[test]
fn every_page_renders() {
    let c = Client::new(&[("harness.toml", "[conflicts]\nbranches = [\"agent/*\"]\n")]);
    for (path, needle) in [
        ("/", "Status"),
        ("/run", "Start run"),
        ("/history", "No recorded runs"),
        ("/findings", "no secrets"),
        ("/conflicts", "need at least two branches"),
        ("/config", "agent/*"),
    ] {
        let r = c.get(path);
        assert_eq!(r.status, 200, "{path}");
        assert!(r.body.contains(needle), "{path}: {}", r.body);
    }
    let missing = c.get("/history/harness_session_nope");
    assert_eq!(missing.status, 404);
    assert_eq!(c.get("/history/..%2F..%2Fetc").status, 404);
}

#[test]
fn config_saves_only_valid_text_and_confirms_loosening() {
    let c = Client::new(&[("harness.toml", "")]);
    let path = c.root().join("harness.toml");

    let r = c.post("/api/config", r#"{"text":"[nope]\n"}"#);
    assert_eq!(r.status, 422);
    assert!(r.body.contains("unknown field"), "{}", r.body);

    let r = c.post(
        "/api/config",
        r#"{"text":"[sandbox]\ntimeout = \"soon\"\n"}"#,
    );
    assert_eq!(r.status, 422);
    assert!(r.body.contains("sandbox.timeout"), "{}", r.body);

    let loosen = r#"{"text":"[sandbox]\nallow_egress = true\n"}"#;
    let r = c.post("/api/config", loosen);
    assert_eq!(r.status, 409);
    assert!(r.body.contains("sandbox.allow_egress"), "{}", r.body);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "");

    let r = c.post(
        "/api/config",
        r#"{"text":"[sandbox]\nallow_egress = true\n","confirm_loosen":true}"#,
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(std::fs::read_to_string(&path)
        .expect("read")
        .contains("allow_egress = true"));

    let r = c.post(
        "/api/config",
        r#"{"text":"[project]\nbase_ref = \"main\"\n"}"#,
    );
    assert_eq!(
        r.status, 200,
        "tightening needs no confirmation: {}",
        r.body
    );
}

#[test]
fn a_run_started_from_the_ui_is_recorded() {
    let c = Client::new(&[("harness.toml", "")]);
    write(c.root(), &[("a.py", "x = 1\n")]);
    let r = c.post("/api/run", r#"{"agent":"claude-code"}"#);
    assert_eq!(r.status, 202, "{}", r.body);

    let deadline = Instant::now() + Duration::from_secs(60);
    let state = loop {
        let r = c.get("/api/run");
        let v: serde_json::Value = serde_json::from_str(&r.body).expect("json");
        if v["running"] == false {
            break v;
        }
        assert!(Instant::now() < deadline, "run never finished");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(state["verdict"], "Pass", "{state}");
    let id = state["session_id"]
        .as_str()
        .expect("session id")
        .to_string();

    let r = c.get(&format!("/history/{id}"));
    assert_eq!(r.status, 200);
    assert!(r.body.contains("claude-code"));
    assert!(r.body.contains("a.py"));
    let r = c.get(&format!("/api/history/{id}"));
    assert!(
        r.body.contains("\"target_agent\": \"claude-code\"")
            || r.body.contains("\"target_agent\":\"claude-code\"")
    );
}

#[test]
fn conflict_fixes_need_the_csrf_header() {
    let c = Client::new(&[("harness.toml", "")]);
    let r = c.post("/api/conflicts/fix", "");
    assert_eq!(r.status, 422, "{}", r.body);
    assert!(r.body.contains("fix:"), "{}", r.body);
    let cookie = c.cookie();
    let r = send(
        c.server.port,
        "POST",
        "/api/conflicts/fix",
        &[("Cookie", &cookie)],
        "",
    );
    assert_eq!(r.status, 403);
}

#[test]
fn stop_shuts_the_server_down() {
    let c = Client::new(&[("harness.toml", "")]);
    let port = c.server.port;
    let cookie = c.cookie();
    c.server.stop().expect("stops");
    // Tests run in parallel, so another test's server may already have been
    // given the freed port. Only a reply that accepts this server's token
    // would mean it is still up.
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return;
    };
    let req = format!(
        "GET /api/history HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\nConnection: close\r\n\r\n"
    );
    let mut raw = String::new();
    let _ = stream.write_all(req.as_bytes());
    let _ = stream.read_to_string(&mut raw);
    assert!(!raw.starts_with("HTTP/1.1 200"), "still serving: {raw}");
}
