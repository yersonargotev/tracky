#![cfg(unix)]

use rusqlite::Connection;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tracky::storage::{apply_migrations, TRACKY_APPLICATION_ID, TRACKY_SCHEMA_GENERATION};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);

fn tracky() -> &'static str {
    env!("CARGO_BIN_EXE_tracky")
}

fn fixture_database(root: &Path) -> PathBuf {
    let database = root.join("tracky.sqlite");
    let connection = Connection::open(&database).expect("create fixture database");
    apply_migrations(&connection).expect("migrate fixture database");
    connection
        .execute_batch(include_str!("fixtures/dashboard/seeds/finance.sql"))
        .expect("seed dashboard fixture");
    drop(connection);
    database
}

fn sandboxed_command(home: &Path) -> Command {
    let mut command = Command::new(tracky());
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("TMPDIR", home.join("tmp"));
    fs::create_dir_all(home.join("tmp")).expect("create sandbox temp directory");
    command
}

fn find_url(line: &str) -> Option<String> {
    let start = line.find("http://127.0.0.1:")?;
    Some(
        line[start..]
            .split_whitespace()
            .next()
            .expect("URL after prefix")
            .trim_end_matches(['.', ',', ')', ']'])
            .to_string(),
    )
}

struct RunningDashboard {
    child: Child,
    url: String,
}

impl RunningDashboard {
    fn start(home: &Path, database: &Path) -> Self {
        let mut child = sandboxed_command(home)
            .args([
                "dashboard",
                "--db",
                database.to_str().expect("UTF-8 database path"),
                "--start-date",
                "2026-01-01",
                "--end-date",
                "2026-03-31",
                "--currency",
                "COP",
                "--no-open",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn dashboard");
        let stdout = child.stdout.take().expect("capture dashboard stdout");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if let Some(url) = find_url(&line) {
                            let _ = sender.send(Ok(url));
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        return;
                    }
                }
            }
            let _ = sender.send(Err("dashboard exited before printing its ready URL".into()));
        });
        let ready = receiver.recv_timeout(PROCESS_TIMEOUT).unwrap_or_else(|_| {
            let status = child.try_wait().expect("query dashboard status");
            panic!("dashboard did not become ready within {PROCESS_TIMEOUT:?}; status={status:?}")
        });
        let url = match ready {
            Ok(url) => url,
            Err(error) => {
                let _ = child.wait();
                let mut stderr = String::new();
                child
                    .stderr
                    .take()
                    .expect("capture dashboard stderr")
                    .read_to_string(&mut stderr)
                    .expect("read dashboard stderr");
                panic!("dashboard readiness failed: {error}; stderr={stderr}");
            }
        };
        let (host, path) = url_parts(&url);
        assert!(
            host.starts_with("127.0.0.1:"),
            "listener must use literal loopback"
        );
        let capability = path
            .strip_prefix("/c/")
            .and_then(|path| path.strip_suffix('/'))
            .expect("capability-bearing dashboard URL");
        assert_eq!(capability.len(), 64, "capability must contain 256 bits");
        assert!(
            capability
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "capability must be canonical lowercase hexadecimal"
        );
        Self { child, url }
    }

    fn stop(&mut self) {
        self.stop_with_signal("-TERM");
    }

    fn stop_with_signal(&mut self, signal: &str) {
        if self
            .child
            .try_wait()
            .expect("query dashboard status")
            .is_some()
        {
            return;
        }
        let status = Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()
            .expect("send termination signal");
        assert!(status.success(), "kill must accept dashboard PID");

        let deadline = Instant::now() + PROCESS_TIMEOUT;
        while Instant::now() < deadline {
            if self.child.try_wait().expect("wait for dashboard").is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        panic!("dashboard did not exit within {PROCESS_TIMEOUT:?} after {signal}");
    }

    fn stderr(&mut self) -> String {
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .expect("dashboard stderr is available once")
            .read_to_string(&mut stderr)
            .expect("read dashboard stderr");
        stderr
    }
}

impl Drop for RunningDashboard {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status();
            let _ = self.child.wait();
        }
    }
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: String,
}

fn url_parts(url: &str) -> (&str, &str) {
    let rest = url.strip_prefix("http://").expect("loopback HTTP URL");
    rest.split_once('/').map_or((rest, "/"), |(host, path)| {
        (host, &url[url.len() - path.len() - 1..])
    })
}

fn request(url: &str, method: &str, path: &str, headers: &[(&str, &str)]) -> HttpResponse {
    let (host, _) = url_parts(url);
    let mut stream = TcpStream::connect(host).expect("connect to dashboard listener");
    stream
        .set_read_timeout(Some(PROCESS_TIMEOUT))
        .expect("set HTTP read timeout");
    write!(stream, "{method} {path} HTTP/1.1\r\n").expect("write request line");
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        write!(stream, "Host: {host}\r\n").expect("write Host header");
    }
    write!(stream, "Connection: close\r\n").expect("write connection header");
    for (name, value) in headers {
        write!(stream, "{name}: {value}\r\n").expect("write request header");
    }
    write!(stream, "\r\n").expect("finish request");
    stream.flush().expect("flush request");

    read_response(stream)
}

fn request_with_undecodable_fetch_site(url: &str, path: &str) -> HttpResponse {
    let (host, _) = url_parts(url);
    let mut stream = TcpStream::connect(host).expect("connect to dashboard listener");
    stream
        .set_read_timeout(Some(PROCESS_TIMEOUT))
        .expect("set HTTP read timeout");
    write!(stream, "GET {path} HTTP/1.1\r\nHost: {host}\r\n").unwrap();
    stream
        .write_all(b"Sec-Fetch-Site: \x80\r\nConnection: close\r\n\r\n")
        .unwrap();
    stream.flush().unwrap();
    read_response(stream)
}

fn read_response(mut stream: TcpStream) -> HttpResponse {
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read HTTP response");
    let (head, body) = raw.split_once("\r\n\r\n").expect("HTTP response framing");
    let mut lines = head.lines();
    let status = lines
        .next()
        .expect("HTTP status line")
        .split_whitespace()
        .nth(1)
        .expect("HTTP status code")
        .parse()
        .expect("numeric HTTP status");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    HttpResponse {
        status,
        headers,
        body: body.to_string(),
    }
}

fn assert_defensive_headers(response: &HttpResponse) {
    for header in [
        "content-security-policy",
        "x-frame-options",
        "referrer-policy",
        "x-content-type-options",
        "cache-control",
    ] {
        assert!(
            response.headers.contains_key(header),
            "{header} missing from {response:?}"
        );
    }
    assert_eq!(response.headers["x-frame-options"], "DENY");
    assert_eq!(response.headers["referrer-policy"], "no-referrer");
    assert_eq!(response.headers["x-content-type-options"], "nosniff");
    assert!(response.headers["cache-control"].contains("no-store"));
    assert!(!response.headers.contains_key("access-control-allow-origin"));
}

fn database_artifact_bytes(database: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let root = database.parent().expect("database parent");
    let database_name = database
        .file_name()
        .expect("database filename")
        .to_string_lossy();
    fs::read_dir(root)
        .expect("read fixture directory")
        .filter_map(|entry| {
            let path = entry.expect("read directory entry").path();
            if !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(database_name.as_ref()))
            {
                return None;
            }
            let relative = path
                .strip_prefix(root)
                .expect("fixture child")
                .to_path_buf();
            let bytes = fs::read(&path).expect("read fixture artifact");
            Some((relative, bytes))
        })
        .collect()
}

#[test]
fn dashboard_is_hidden_from_general_help_but_has_explicit_help() {
    let root = tempfile::tempdir().expect("sandbox");
    let top = sandboxed_command(root.path())
        .arg("--help")
        .output()
        .expect("run top-level help");
    assert!(top.status.success());
    assert!(!String::from_utf8_lossy(&top.stdout).contains("dashboard"));

    let dashboard = sandboxed_command(root.path())
        .args(["dashboard", "--help"])
        .output()
        .expect("run hidden dashboard help");
    assert!(dashboard.status.success());
    assert!(String::from_utf8_lossy(&dashboard.stdout).contains("--no-open"));
}

#[test]
fn database_upgrade_marks_tracky_legacy_database_and_refuses_unrelated_sqlite() {
    let root = tempfile::tempdir().expect("sandbox");
    let legacy = root.path().join("legacy.sqlite");
    let connection = Connection::open(&legacy).expect("create legacy database");
    apply_migrations(&connection).expect("create recognizable Tracky schema");
    connection
        .execute_batch("PRAGMA application_id = 0; PRAGMA user_version = 1;")
        .expect("restore legacy markers");
    drop(connection);

    let upgraded = sandboxed_command(root.path())
        .args(["database", "upgrade", "--db"])
        .arg(&legacy)
        .output()
        .expect("upgrade legacy Tracky database");
    assert!(
        upgraded.status.success(),
        "upgrade failed: {}",
        String::from_utf8_lossy(&upgraded.stderr)
    );
    let connection = Connection::open(&legacy).expect("inspect upgraded database");
    let application_id: i64 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .expect("read application_id");
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("read user_version");
    assert_ne!(application_id, 0, "upgrade must mark Tracky identity");
    assert_eq!(user_version, 2, "dashboard-compatible generation");

    let unrelated = root.path().join("unrelated.sqlite");
    Connection::open(&unrelated)
        .expect("create unrelated database")
        .execute("CREATE TABLE private_secrets(value TEXT)", [])
        .expect("create unrelated schema");
    let before = fs::read(&unrelated).expect("read unrelated database");
    let refused = sandboxed_command(root.path())
        .args(["database", "upgrade", "--db"])
        .arg(&unrelated)
        .output()
        .expect("attempt unrelated upgrade");
    assert!(!refused.status.success());
    assert_eq!(fs::read(&unrelated).unwrap(), before);
    let diagnostic = String::from_utf8_lossy(&refused.stderr);
    assert!(!diagnostic.contains(root.path().to_string_lossy().as_ref()));
    assert!(!diagnostic.contains("private_secrets"));

    let connection = Connection::open(&unrelated).expect("reopen unrelated database");
    connection
        .pragma_update(None, "application_id", TRACKY_APPLICATION_ID)
        .unwrap();
    connection
        .pragma_update(None, "user_version", TRACKY_SCHEMA_GENERATION)
        .unwrap();
    drop(connection);
    let spoofed_before = fs::read(&unrelated).unwrap();
    let spoofed = sandboxed_command(root.path())
        .args(["database", "upgrade", "--db"])
        .arg(&unrelated)
        .output()
        .expect("attempt upgrade with spoofed markers");
    assert!(!spoofed.status.success());
    assert_eq!(fs::read(&unrelated).unwrap(), spoofed_before);

    let shaped = root.path().join("legacy-shaped-unrelated.sqlite");
    Connection::open(&shaped)
        .unwrap()
        .execute_batch(
            "CREATE TABLE institutions(id TEXT);
             CREATE TABLE accounts(id TEXT);
             CREATE TABLE source_documents(id TEXT);
             CREATE TABLE import_batches(id TEXT);
             CREATE TABLE candidate_transactions(id TEXT);
             CREATE TABLE provenance(id TEXT);
             CREATE TABLE canonical_transactions(id TEXT);
             CREATE TABLE transaction_fingerprints(id TEXT);
             CREATE TABLE transaction_duplicate_markers(id TEXT);
             PRAGMA user_version = 1;",
        )
        .unwrap();
    let shaped_before = fs::read(&shaped).unwrap();
    let refused = sandboxed_command(root.path())
        .args(["database", "upgrade", "--db"])
        .arg(&shaped)
        .output()
        .expect("attempt legacy-shaped unrelated upgrade");
    assert!(!refused.status.success());
    assert_eq!(fs::read(&shaped).unwrap(), shaped_before);
}

#[test]
fn dashboard_uses_its_single_pre_listener_snapshot_until_explicit_refresh_exists() {
    let root = tempfile::tempdir().expect("sandbox");
    let database = fixture_database(root.path());
    let mut dashboard = RunningDashboard::start(root.path(), &database);
    Connection::open(&database)
        .unwrap()
        .execute(
            "INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind, income_source_id, income_kind) VALUES ('post-start-income', 'cop-checking', '2026-03-01', 'Post-start income', 999999, 'COP', 'income', 'salary', 'salary')",
            [],
        )
        .unwrap();

    let path = url_parts(&dashboard.url).1.to_string();
    let response = request(&dashboard.url, "GET", &path, &[]);
    assert_eq!(response.status, 200);
    assert!(!response.body.contains("999999"));
    dashboard.stop();
}

#[test]
fn invalid_dashboard_arguments_fail_before_printing_or_creating_a_listener() {
    let root = tempfile::tempdir().expect("sandbox");
    let database = fixture_database(root.path());
    for arguments in [
        vec!["--start-date", "2026-03-31", "--end-date", "2026-01-01"],
        vec!["--currency", "ZZZ"],
    ] {
        let started = Instant::now();
        let output = sandboxed_command(root.path())
            .args(["dashboard", "--db"])
            .arg(&database)
            .args(arguments)
            .arg("--no-open")
            .output()
            .expect("run invalid dashboard invocation");
        assert!(!output.status.success());
        assert!(started.elapsed() < PROCESS_TIMEOUT);
        assert!(!String::from_utf8_lossy(&output.stdout).contains("http://"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("http://"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
    }

    let missing = root.path().join("does-not-exist.sqlite");
    let output = sandboxed_command(root.path())
        .args(["dashboard", "--db"])
        .arg(&missing)
        .arg("--no-open")
        .output()
        .expect("run dashboard against a missing database");
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("http://"));
    assert!(
        !missing.exists(),
        "dashboard must not create a missing database"
    );
}

#[test]
fn capability_get_returns_exact_semantic_html_without_mutating_database() {
    let root = tempfile::tempdir().expect("sandbox");
    let database = fixture_database(root.path());
    let before = database_artifact_bytes(&database);
    let mut dashboard = RunningDashboard::start(root.path(), &database);
    let (_, path) = url_parts(&dashboard.url);
    let response = request(
        &dashboard.url,
        "GET",
        path,
        &[("Sec-Fetch-Site", "none"), ("Sec-Fetch-Mode", "navigate")],
    );
    assert_eq!(response.status, 200, "{response:?}");
    assert!(response.headers["content-type"].starts_with("text/html"));
    assert_defensive_headers(&response);
    for semantic in ["<main", "<h1", "<table", "<th", "<tbody"] {
        assert!(response.body.contains(semantic), "missing {semantic}");
    }
    for exact_value in ["500000", "170000", "330000", "100000"] {
        assert!(
            response.body.contains(exact_value),
            "missing exact value {exact_value}"
        );
    }
    assert!(!response.body.contains("http://"));
    assert!(!response.body.contains("https://"));
    let (host, _) = url_parts(&dashboard.url);
    let address = host.to_string();
    dashboard.stop();
    assert!(
        TcpStream::connect(address).is_err(),
        "listener must close when the foreground process exits"
    );
    assert_eq!(database_artifact_bytes(&database), before);
}

#[test]
fn capability_v1_resources_return_the_complete_snapshot_and_canonical_drill_down() {
    let root = tempfile::tempdir().expect("sandbox");
    let database = fixture_database(root.path());
    let before = database_artifact_bytes(&database);
    let mut dashboard = RunningDashboard::start(root.path(), &database);
    let (_, path) = url_parts(&dashboard.url);

    let aggregate = request(
        &dashboard.url,
        "GET",
        &format!("{path}api/v1/dashboard"),
        &[
            ("Sec-Fetch-Site", "same-origin"),
            ("Sec-Fetch-Mode", "same-origin"),
        ],
    );
    assert_eq!(aggregate.status, 200, "{aggregate:?}");
    assert!(aggregate.headers["content-type"].starts_with("application/json"));
    assert_defensive_headers(&aggregate);
    let aggregate: serde_json::Value = serde_json::from_str(&aggregate.body).unwrap();
    assert_eq!(aggregate["schema_version"], "tracky.dashboard.v1");
    assert_eq!(aggregate["summary"]["income_minor"], "500000");
    assert!(aggregate["investments"].is_object());
    assert!(aggregate["alerts"].is_array());

    let usd = request(
        &dashboard.url,
        "GET",
        &format!("{path}api/v1/dashboard?currency=USD"),
        &[("Sec-Fetch-Site", "same-origin")],
    );
    assert_eq!(usd.status, 200, "{usd:?}");
    let usd: serde_json::Value = serde_json::from_str(&usd.body).unwrap();
    assert_eq!(usd["summary"]["currency"], "USD");
    assert_eq!(usd["summary"]["income_minor"], "10000");
    assert!(usd["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .all(|account| account["currency"] == "USD"));

    let filtered = request(
        &dashboard.url,
        "GET",
        &format!("{path}api/v1/dashboard?account=cop-checking&category=food"),
        &[("Sec-Fetch-Site", "same-origin")],
    );
    assert_eq!(filtered.status, 200, "{filtered:?}");
    let filtered: serde_json::Value = serde_json::from_str(&filtered.body).unwrap();
    assert_eq!(filtered["summary"]["consumption_expense_minor"], "100000");
    assert_eq!(filtered["summary"]["investment_contribution_minor"], "0");
    assert_eq!(
        filtered["filters"]["account_ids"],
        serde_json::json!(["cop-checking"])
    );

    let drill = request(
        &dashboard.url,
        "GET",
        &format!("{path}api/v1/transactions?metric=income&limit=1"),
        &[
            ("Sec-Fetch-Site", "same-origin"),
            ("Sec-Fetch-Mode", "same-origin"),
        ],
    );
    assert_eq!(drill.status, 200, "{drill:?}");
    assert!(drill.headers["content-type"].starts_with("application/json"));
    let drill: serde_json::Value = serde_json::from_str(&drill.body).unwrap();
    assert_eq!(drill["schema_version"], "tracky.dashboard.v1");
    assert_eq!(drill["metric"], "income");
    assert_eq!(
        drill["rows"][0]["canonical_transaction_id"],
        "cop-income-jan"
    );

    let invalid = request(
        &dashboard.url,
        "GET",
        &format!("{path}api/v1/transactions?metric=income&metric=activity"),
        &[("Sec-Fetch-Site", "same-origin")],
    );
    assert_eq!(invalid.status, 400);
    assert!(!invalid.body.contains(database.to_string_lossy().as_ref()));
    let invalid_aggregate = request(
        &dashboard.url,
        "GET",
        &format!("{path}api/v1/dashboard?metric=income"),
        &[("Sec-Fetch-Site", "same-origin")],
    );
    assert_eq!(invalid_aggregate.status, 400);

    dashboard.stop();
    assert_eq!(database_artifact_bytes(&database), before);
}

#[test]
fn dashboard_rejects_adversarial_http_without_leaking_data_or_capability() {
    let root = tempfile::tempdir().expect("sandbox");
    let database = fixture_database(root.path());
    let decoy = "DO-NOT-LEAK-SYNTHETIC-SECRET";
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE canonical_transactions SET description = ?1 WHERE id = 'cop-income-jan'",
            [decoy],
        )
        .unwrap();
    let mut dashboard = RunningDashboard::start(root.path(), &database);
    let path = url_parts(&dashboard.url).1.to_string();
    let wrong_capability = format!("{path}x");
    let unknown_v1 = format!("{path}api/v1/private");
    let cases = [
        ("GET", wrong_capability.as_str(), vec![]),
        ("GET", unknown_v1.as_str(), vec![]),
        ("POST", path.as_str(), vec![]),
        ("GET", "/", vec![]),
        ("GET", "/../tracky.sqlite", vec![]),
        ("GET", path.as_str(), vec![("Sec-Fetch-Site", "cross-site")]),
        ("GET", path.as_str(), vec![("Host", "attacker.invalid")]),
    ];
    for (method, requested_path, headers) in cases {
        let response = request(&dashboard.url, method, requested_path, &headers);
        assert!(
            (400..500).contains(&response.status),
            "request should fail closed: {response:?}"
        );
        assert_defensive_headers(&response);
        assert!(!response.body.contains(decoy));
        assert!(!response.body.contains(&path));
        assert!(!response.body.contains(database.to_string_lossy().as_ref()));
    }
    let malformed = request_with_undecodable_fetch_site(&dashboard.url, &path);
    assert!((400..500).contains(&malformed.status));
    assert_defensive_headers(&malformed);
    assert!(!malformed.body.contains(decoy));
    dashboard.stop();
    let stderr = dashboard.stderr();
    assert!(!stderr.contains(decoy));
    assert!(!stderr.contains(&path));
    assert!(!stderr.contains(database.to_string_lossy().as_ref()));
}

#[test]
fn concurrent_dashboards_use_independent_ports_and_capabilities_and_terminate() {
    let first_root = tempfile::tempdir().expect("first sandbox");
    let second_root = tempfile::tempdir().expect("second sandbox");
    let first_database = fixture_database(first_root.path());
    let second_database = fixture_database(second_root.path());
    let mut first = RunningDashboard::start(first_root.path(), &first_database);
    let mut second = RunningDashboard::start(second_root.path(), &second_database);
    assert_ne!(first.url, second.url);
    let (first_host, first_path) = url_parts(&first.url);
    let (second_host, second_path) = url_parts(&second.url);
    assert_ne!(
        first_host, second_host,
        "ephemeral listeners must be independent"
    );
    assert_ne!(first_path, second_path, "capabilities must be independent");

    let crossed = request(&second.url, "GET", first_path, &[]);
    assert!((400..500).contains(&crossed.status));
    assert_defensive_headers(&crossed);
    let (host, path) = url_parts(&first.url);
    let mut held = TcpStream::connect(host).expect("open held dashboard connection");
    write!(held, "GET {path} HTTP/1.1\r\nHost: {host}\r\n").unwrap();
    held.flush().unwrap();
    let shutdown_started = Instant::now();
    first.stop();
    assert!(shutdown_started.elapsed() < Duration::from_secs(3));
    drop(held);
    second.stop_with_signal("-INT");
}
