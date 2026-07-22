use crate::dashboard_finance::{
    read_drill_down, read_finance, DashboardResponse, DrillMetric, DrillRequest,
    FinanceFilterRequest, UnavailableCurrencyError,
};
use crate::storage::{
    dashboard_schema_is_compatible, TRACKY_APPLICATION_ID, TRACKY_SCHEMA_GENERATION,
};
use anyhow::{bail, Context, Result};
use axum::extract::{Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{Datelike, Months, NaiveDate, Utc};
use rusqlite::backup::Backup;
use rusqlite::{Connection, DatabaseName, OpenFlags, Transaction};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

const CSS: &str = include_str!("dashboard_assets/dashboard.css");
const JAVASCRIPT: &str = include_str!("dashboard_assets/dashboard.js");
pub(crate) const INCOMPATIBLE_SNAPSHOT_EXIT_CODE: i32 = 42;

#[derive(Debug)]
pub(crate) struct IncompatibleDashboardSchemaError;

impl std::fmt::Display for IncompatibleDashboardSchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("unsupported Tracky schema; run `tracky database upgrade --db <PATH>`")
    }
}

impl std::error::Error for IncompatibleDashboardSchemaError {}

#[derive(Debug)]
pub struct DashboardOptions {
    pub db: PathBuf,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub currency: Option<String>,
    pub no_open: bool,
}

#[derive(Clone)]
struct AppState {
    snapshot: Arc<Mutex<DashboardSnapshot>>,
    db: PathBuf,
    startup_request: FinanceFilterRequest,
    host: String,
}

struct DashboardSnapshot {
    connection: Connection,
    startup_response: DashboardResponse,
    _directory: TempDir,
}

pub fn serve<W: Write>(options: DashboardOptions, mut stdout: W) -> Result<i32> {
    let (start_date, end_date) =
        validated_dates(options.start_date.as_deref(), options.end_date.as_deref())?;
    let currency = options
        .currency
        .map(|value| validate_currency(&value))
        .transpose()?;

    // Initial compatibility and snapshot failures are fatal before a listener exists.
    let startup_request = FinanceFilterRequest {
        start_date,
        end_date,
        currency,
        ..FinanceFilterRequest::default()
    };
    let snapshot = Arc::new(Mutex::new(load_snapshot(
        &options.db,
        startup_request.clone(),
    )?));

    let capability_bytes = rand::random::<[u8; 32]>();
    let capability = capability_bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .context("starting dashboard runtime")?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .context("binding dashboard loopback listener")?;
        let address = listener.local_addr().context("reading dashboard address")?;
        let host = format!("127.0.0.1:{}", address.port());
        let prefix = format!("/c/{capability}");
        let url = format!("http://{host}{prefix}/");
        let state = AppState {
            snapshot,
            db: options.db.clone(),
            startup_request,
            host,
        };
        let app = Router::new()
            .route(&format!("{prefix}/"), get(dashboard_page))
            .route(&format!("{prefix}/app.css"), get(stylesheet))
            .route(&format!("{prefix}/app.js"), get(javascript))
            .route(&format!("{prefix}/api/v1/dashboard"), get(dashboard_api))
            .route(
                &format!("{prefix}/api/v1/dashboard/refresh"),
                get(refresh_api),
            )
            .route(
                &format!("{prefix}/api/v1/transactions"),
                get(transactions_api),
            )
            .fallback(not_found)
            .layer(middleware::from_fn_with_state(
                state.clone(),
                secure_request,
            ))
            .with_state(state);

        let (shutdown_sender, shutdown_receiver) = tokio::sync::oneshot::channel();
        let mut server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_receiver.await;
                })
                .await
        });
        tokio::net::TcpStream::connect(address)
            .await
            .context("checking dashboard readiness")?;
        writeln!(stdout, "Dashboard ready: {url}")?;
        stdout.flush()?;
        if !options.no_open {
            open_browser(&url);
        }
        shutdown_signal().await;
        let _ = shutdown_sender.send(());
        match tokio::time::timeout(Duration::from_secs(2), &mut server).await {
            Ok(result) => result
                .context("joining dashboard server")?
                .context("serving dashboard")?,
            Err(_) => {
                server.abort();
                let _ = server.await;
            }
        }
        Ok(0)
    })
}

fn validated_dates(start: Option<&str>, end: Option<&str>) -> Result<(String, String)> {
    let today = Utc::now().date_naive();
    let end = match end {
        Some(value) => parse_date(value, "end date")?,
        None => today,
    };
    let start = match start {
        Some(value) => parse_date(value, "start date")?,
        None => NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
            .expect("current month is valid")
            .checked_sub_months(Months::new(11))
            .expect("dashboard default date is representable"),
    };
    if start > end {
        bail!("start date must be on or before end date");
    }
    Ok((start.to_string(), end.to_string()))
}

fn parse_date(value: &str, label: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("{label} must use YYYY-MM-DD"))
}

fn validate_currency(value: &str) -> Result<String> {
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
        bail!("currency must be a three-letter uppercase ISO code");
    }
    Ok(value.to_string())
}

fn open_dashboard_database(path: &Path) -> Result<Connection> {
    let absolute = path
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("dashboard database could not be opened"))?;
    let path = absolute
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("dashboard database could not be opened"))?;
    let mut uri = String::from("file:");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(char::from(byte));
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri.push_str("?immutable=1");
    let connection = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| anyhow::anyhow!("dashboard database could not be opened"))?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|_| anyhow::anyhow!("dashboard database could not be secured"))?;
    if !connection
        .is_readonly(DatabaseName::Main)
        .map_err(|_| anyhow::anyhow!("dashboard database mode could not be verified"))?
    {
        bail!("dashboard database is not read-only");
    }
    validate_markers(&connection)?;
    if !dashboard_schema_is_compatible(&connection).map_err(|_| IncompatibleDashboardSchemaError)? {
        return Err(IncompatibleDashboardSchemaError.into());
    }
    Ok(connection)
}

fn markers(connection: &Connection) -> Result<(i64, i64)> {
    let application_id = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    let generation = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    Ok((application_id, generation))
}

fn validate_markers(connection: &Connection) -> Result<()> {
    let (application_id, generation) = markers(connection)
        .map_err(|_| anyhow::anyhow!("Tracky schema identity could not be verified"))?;
    if application_id != TRACKY_APPLICATION_ID || generation != TRACKY_SCHEMA_GENERATION {
        return Err(IncompatibleDashboardSchemaError.into());
    }
    Ok(())
}

fn read_dashboard(
    connection: &mut Connection,
    request: FinanceFilterRequest,
) -> Result<DashboardResponse> {
    read_transaction(connection, |transaction| {
        read_finance(transaction, request).map_err(|error| {
            if error.is::<UnavailableCurrencyError>() {
                error
            } else {
                anyhow::anyhow!("dashboard data could not be read")
            }
        })
    })
}

fn read_drill(
    connection: &mut Connection,
    request: DrillRequest,
) -> Result<crate::dashboard_finance::DrillResponse> {
    read_transaction(connection, |transaction| {
        read_drill_down(transaction, request)
            .map_err(|_| anyhow::anyhow!("dashboard data could not be read"))
    })
}

fn read_transaction<T>(
    connection: &mut Connection,
    read: impl FnOnce(&Transaction<'_>) -> Result<T>,
) -> Result<T> {
    let transaction = connection
        .transaction()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
    let response = read(&transaction)?;
    transaction
        .commit()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be completed"))?;
    Ok(response)
}

fn load_snapshot(path: &Path, startup_request: FinanceFilterRequest) -> Result<DashboardSnapshot> {
    let snapshot_directory =
        TempDir::new().map_err(|_| anyhow::anyhow!("dashboard snapshot could not be created"))?;
    let snapshot_file = snapshot_directory.path().join("snapshot.sqlite");
    let executable = std::env::current_exe()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot helper is unavailable"))?;
    let status = Command::new(executable)
        .arg("__dashboard-snapshot")
        .arg("--source")
        .arg(path)
        .arg("--destination")
        .arg(&snapshot_file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
    if status.code() == Some(INCOMPATIBLE_SNAPSHOT_EXIT_CODE) {
        return Err(IncompatibleDashboardSchemaError.into());
    }
    if !status.success() {
        bail!("dashboard snapshot could not be completed");
    }
    let mut connection = Connection::open_with_flags(
        &snapshot_file,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be reopened"))?;
    connection
        .pragma_update(None, "cache_size", -2048)
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be bounded"))?;
    connection
        .pragma_update(None, "temp_store", "FILE")
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be bounded"))?;
    connection
        .pragma_update(None, "query_only", true)
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be secured"))?;
    let startup_response = read_dashboard(&mut connection, startup_request)?;
    Ok(DashboardSnapshot {
        connection,
        startup_response,
        _directory: snapshot_directory,
    })
}

pub(crate) fn write_snapshot(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        bail!("dashboard snapshot destination must not exist");
    }
    let source = open_dashboard_database(source)?;
    let mut destination = Connection::open(destination)
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
    {
        let backup = Backup::new(&source, &mut destination)
            .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
        backup
            .run_to_completion(i32::MAX, Duration::ZERO, None)
            .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be completed"))?;
    }
    Ok(())
}

async fn dashboard_api(State(state): State<AppState>, request: Request) -> Response {
    let filters = match dashboard_filters(request.uri().query(), &state.startup_request) {
        Ok(filters) => filters,
        Err(()) => return api_error(StatusCode::BAD_REQUEST, "invalid_dashboard_request"),
    };
    let snapshot = state.snapshot.clone();
    match tokio::task::spawn_blocking(move || {
        let mut snapshot = snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("dashboard snapshot unavailable"))?;
        read_dashboard(&mut snapshot.connection, filters)
    })
    .await
    {
        Ok(Ok(response)) => Json(response).into_response(),
        _ => api_error(StatusCode::BAD_REQUEST, "dashboard_read_failed"),
    }
}

async fn refresh_api(State(state): State<AppState>, request: Request) -> Response {
    let filters = match dashboard_filters(request.uri().query(), &state.startup_request) {
        Ok(filters) => filters,
        Err(()) => return api_error(StatusCode::BAD_REQUEST, "invalid_dashboard_request"),
    };
    let path = state.db.clone();
    let startup_request = state.startup_request.clone();
    let replacement = tokio::task::spawn_blocking(move || -> Result<_> {
        let mut snapshot = load_snapshot(&path, startup_request)?;
        let response = read_dashboard(&mut snapshot.connection, filters)?;
        Ok((snapshot, response))
    })
    .await;
    match replacement {
        Ok(Ok((replacement, response))) => match state.snapshot.lock() {
            Ok(mut snapshot) => {
                *snapshot = replacement;
                Json(response).into_response()
            }
            Err(_) => refresh_error(),
        },
        Ok(Err(error)) if error.is::<IncompatibleDashboardSchemaError>() => {
            incompatible_schema_error()
        }
        _ => refresh_error(),
    }
}

async fn transactions_api(State(state): State<AppState>, request: Request) -> Response {
    let query = match query_pairs(request.uri().query()) {
        Ok(query) => query,
        Err(()) => return api_error(StatusCode::BAD_REQUEST, "invalid_drill_request"),
    };
    let filters = match filters_from_pairs(&query, &state.startup_request) {
        Ok(filters) => filters,
        Err(()) => return api_error(StatusCode::BAD_REQUEST, "invalid_drill_request"),
    };
    let metric = match one(&query, "metric").and_then(parse_metric) {
        Some(metric) => metric,
        None => return api_error(StatusCode::BAD_REQUEST, "invalid_drill_request"),
    };
    let month = match one(&query, "month") {
        Some(month)
            if month.len() == 7
                && month.as_bytes()[4] == b'-'
                && month[..4].bytes().all(|byte| byte.is_ascii_digit())
                && month[5..]
                    .parse::<u8>()
                    .is_ok_and(|month| (1..=12).contains(&month)) =>
        {
            Some(month.to_string())
        }
        Some(_) => return api_error(StatusCode::BAD_REQUEST, "invalid_drill_request"),
        None => None,
    };
    let limit = match one(&query, "limit") {
        Some(limit) => match limit.parse::<usize>() {
            Ok(limit) if (1..=100).contains(&limit) => limit,
            _ => return api_error(StatusCode::BAD_REQUEST, "invalid_drill_request"),
        },
        None => 50,
    };
    let drill = DrillRequest {
        filters,
        metric,
        month,
        cursor: one(&query, "cursor").map(str::to_string),
        limit,
    };
    let snapshot = state.snapshot.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut snapshot = snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("dashboard snapshot unavailable"))?;
        read_drill(&mut snapshot.connection, drill)
    })
    .await;
    match result {
        Ok(Ok(response)) => Json(response).into_response(),
        _ => api_error(StatusCode::BAD_REQUEST, "dashboard_read_failed"),
    }
}

type QueryPairs = BTreeMap<String, Vec<String>>;

fn dashboard_filters(
    query: Option<&str>,
    defaults: &FinanceFilterRequest,
) -> std::result::Result<FinanceFilterRequest, ()> {
    let query = query_pairs(query)?;
    if query.keys().any(|key| {
        !matches!(
            key.as_str(),
            "start" | "end" | "currency" | "account" | "category"
        )
    }) {
        return Err(());
    }
    filters_from_pairs(&query, defaults)
}

fn filters_from_pairs(
    query: &QueryPairs,
    defaults: &FinanceFilterRequest,
) -> std::result::Result<FinanceFilterRequest, ()> {
    const ALLOWED: &[&str] = &[
        "start", "end", "currency", "account", "category", "metric", "month", "cursor", "limit",
    ];
    if query.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(());
    }
    Ok(FinanceFilterRequest {
        start_date: one(query, "start")
            .unwrap_or(&defaults.start_date)
            .to_string(),
        end_date: one(query, "end").unwrap_or(&defaults.end_date).to_string(),
        currency: one(query, "currency")
            .map(str::to_string)
            .or_else(|| defaults.currency.clone()),
        account_ids: query
            .get("account")
            .cloned()
            .or_else(|| defaults.account_ids.clone()),
        category_ids: query
            .get("category")
            .cloned()
            .or_else(|| defaults.category_ids.clone()),
    })
}

fn query_pairs(query: Option<&str>) -> std::result::Result<QueryPairs, ()> {
    let mut pairs = BTreeMap::new();
    for pair in query
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
    {
        let (key, value) = pair.split_once('=').ok_or(())?;
        let key = decode_query_component(key)?;
        let value = decode_query_component(value)?;
        pairs.entry(key).or_insert_with(Vec::new).push(value);
    }
    if pairs
        .iter()
        .any(|(key, values)| !matches!(key.as_str(), "account" | "category") && values.len() != 1)
    {
        return Err(());
    }
    Ok(pairs)
}

fn decode_query_component(value: &str) -> std::result::Result<String, ()> {
    let mut bytes = Vec::with_capacity(value.len());
    let source = value.as_bytes();
    let mut index = 0;
    while index < source.len() {
        match source[index] {
            b'%' if index + 2 < source.len() => {
                let high = hex_value(source[index + 1]).ok_or(())?;
                let low = hex_value(source[index + 2]).ok_or(())?;
                bytes.push(high * 16 + low);
                index += 3;
            }
            b'%' => return Err(()),
            b'+' => {
                bytes.push(b' ');
                index += 1;
            }
            byte if byte.is_ascii() => {
                bytes.push(byte);
                index += 1;
            }
            _ => return Err(()),
        }
    }
    String::from_utf8(bytes).map_err(|_| ())
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn one<'a>(query: &'a QueryPairs, key: &str) -> Option<&'a str> {
    query.get(key)?.first().map(String::as_str)
}

fn parse_metric(metric: &str) -> Option<DrillMetric> {
    match metric {
        "activity" => Some(DrillMetric::Activity),
        "income" => Some(DrillMetric::Income),
        "consumption_expense" => Some(DrillMetric::ConsumptionExpense),
        "net_cash_flow" => Some(DrillMetric::NetCashFlow),
        "investment_contribution" => Some(DrillMetric::InvestmentContribution),
        _ => None,
    }
}

fn api_error(status: StatusCode, code: &'static str) -> Response {
    (
        status,
        Json(serde_json::json!({"ok": false, "errors": [{"code": code}]})),
    )
        .into_response()
}

fn refresh_error() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({
            "ok": false,
            "state": "stale",
            "errors": [{"code": "dashboard_refresh_failed"}]
        })),
    )
        .into_response()
}

fn incompatible_schema_error() -> Response {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({
            "ok": false,
            "state": "incompatible_schema",
            "errors": [{"code": "dashboard_schema_incompatible"}]
        })),
    )
        .into_response()
}

async fn dashboard_page(State(state): State<AppState>) -> Response {
    match state.snapshot.lock() {
        Ok(snapshot) => {
            Html(crate::dashboard_view::render(&snapshot.startup_response)).into_response()
        }
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, "Dashboard unavailable").into_response(),
    }
}

async fn stylesheet() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], CSS).into_response()
}

async fn javascript() -> Response {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        JAVASCRIPT,
    )
        .into_response()
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

async fn secure_request(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let valid_host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.host);
    let valid_fetch_site = valid_optional_header(
        request.headers(),
        "sec-fetch-site",
        &["none", "same-origin"],
    );
    let valid_fetch_mode = valid_optional_header(
        request.headers(),
        "sec-fetch-mode",
        &["navigate", "no-cors", "same-origin", "cors"],
    );
    let mut response =
        if request.method() == Method::GET && valid_host && valid_fetch_site && valid_fetch_mode {
            next.run(request).await
        } else {
            (StatusCode::NOT_FOUND, "Not found").into_response()
        };
    add_security_headers(response.headers_mut());
    response
}

fn valid_optional_header(
    headers: &axum::http::HeaderMap,
    name: &'static str,
    allowed: &[&str],
) -> bool {
    match headers.get(name) {
        None => true,
        Some(value) => value.to_str().is_ok_and(|value| allowed.contains(&value)),
    }
}

fn add_security_headers(headers: &mut axum::http::HeaderMap) {
    const VALUES: &[(&str, &str)] = &[
        ("cache-control", "no-store"),
        ("content-security-policy", "default-src 'none'; style-src 'self'; script-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"),
        ("cross-origin-resource-policy", "same-origin"),
        ("permissions-policy", "camera=(), microphone=(), geolocation=()"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
    ];
    for (name, value) in VALUES {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn open_browser(url: &str) {
    let _ = webbrowser::open(url);
}
