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
use rusqlite::{Connection, DatabaseName, OpenFlags};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const CSS: &str = include_str!("dashboard_assets/dashboard.css");
const JAVASCRIPT: &str = include_str!("dashboard_assets/dashboard.js");

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
    snapshot: Arc<DashboardResponse>,
    db: PathBuf,
    startup_request: FinanceFilterRequest,
    host: String,
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
    let snapshot = Arc::new(load_snapshot(&options.db, startup_request.clone())?);

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
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
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
    if !dashboard_schema_is_compatible(&connection)
        .map_err(|_| anyhow::anyhow!("dashboard schema capabilities could not be verified"))?
    {
        bail!("unsupported Tracky schema; run `tracky database upgrade --db <PATH>`");
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
        bail!("unsupported Tracky schema; run `tracky database upgrade --db <PATH>`");
    }
    Ok(())
}

fn load_snapshot(path: &Path, request: FinanceFilterRequest) -> Result<DashboardResponse> {
    let mut connection = open_dashboard_database(path)?;
    let transaction = connection
        .transaction()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
    let snapshot = read_finance(&transaction, request).map_err(|error| {
        if error.is::<UnavailableCurrencyError>() {
            error
        } else {
            anyhow::anyhow!("dashboard data could not be read")
        }
    })?;
    transaction
        .commit()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be completed"))?;
    Ok(snapshot)
}

async fn dashboard_api(State(state): State<AppState>, request: Request) -> Response {
    let query = match query_pairs(request.uri().query()) {
        Ok(query)
            if query.keys().all(|key| {
                matches!(
                    key.as_str(),
                    "start" | "end" | "currency" | "account" | "category"
                )
            }) =>
        {
            query
        }
        _ => return api_error(StatusCode::BAD_REQUEST, "invalid_dashboard_request"),
    };
    let filters = match filters_from_pairs(&query, &state.startup_request) {
        Ok(filters) => filters,
        Err(()) => return api_error(StatusCode::BAD_REQUEST, "invalid_dashboard_request"),
    };
    if filters == state.startup_request {
        return Json((*state.snapshot).clone()).into_response();
    }
    let path = state.db.clone();
    match tokio::task::spawn_blocking(move || load_snapshot(&path, filters)).await {
        Ok(Ok(response)) => Json(response).into_response(),
        _ => api_error(StatusCode::BAD_REQUEST, "dashboard_read_failed"),
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
    let path = state.db.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<_> {
        let mut connection = open_dashboard_database(&path)?;
        let transaction = connection
            .transaction()
            .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
        let response = read_drill_down(&transaction, drill)
            .map_err(|_| anyhow::anyhow!("dashboard data could not be read"))?;
        transaction
            .commit()
            .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be completed"))?;
        Ok(response)
    })
    .await;
    match result {
        Ok(Ok(response)) => Json(response).into_response(),
        _ => api_error(StatusCode::BAD_REQUEST, "dashboard_read_failed"),
    }
}

type QueryPairs = BTreeMap<String, Vec<String>>;

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

async fn dashboard_page(State(state): State<AppState>) -> Response {
    Html(crate::dashboard_view::render(&state.snapshot)).into_response()
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
