use crate::dashboard_finance::{
    read_finance, DashboardResponse, FinanceFilterRequest, UnavailableCurrencyError,
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
use axum::Router;
use chrono::{Datelike, Months, NaiveDate, Utc};
use rusqlite::{Connection, DatabaseName, OpenFlags};
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
    let snapshot = Arc::new(load_snapshot(
        &options.db,
        &start_date,
        &end_date,
        currency.as_deref(),
    )?);

    let capability_bytes = rand::random::<[u8; 32]>();
    let capability = capability_bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
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
        let state = AppState { snapshot, host };
        let app = Router::new()
            .route(&format!("{prefix}/"), get(dashboard_page))
            .route(&format!("{prefix}/app.css"), get(stylesheet))
            .route(&format!("{prefix}/app.js"), get(javascript))
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

fn load_snapshot(
    path: &Path,
    start: &str,
    end: &str,
    requested: Option<&str>,
) -> Result<DashboardResponse> {
    let mut connection = open_dashboard_database(path)?;
    let transaction = connection
        .transaction()
        .map_err(|_| anyhow::anyhow!("dashboard snapshot could not be started"))?;
    let snapshot = read_finance(
        &transaction,
        FinanceFilterRequest {
            start_date: start.to_string(),
            end_date: end.to_string(),
            currency: requested.map(str::to_string),
            ..FinanceFilterRequest::default()
        },
    )
    .map_err(|error| {
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

async fn dashboard_page(State(state): State<AppState>) -> Response {
    Html(render_html(&state.snapshot)).into_response()
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
        &["navigate", "no-cors", "same-origin"],
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

fn render_html(snapshot: &DashboardResponse) -> String {
    let stylesheet = "app.css";
    let script = "app.js";
    let Some(currency) = snapshot.filters.currency.as_deref() else {
        return format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Tracky Monthly ledger</title><link rel=\"stylesheet\" href=\"{stylesheet}\"></head><body><main><h1>Monthly ledger</h1><p class=\"scope\">{} through {}</p><section class=\"empty\"><h2>No currency activity</h2><p>This is a valid empty ledger. Add canonical activity with the Tracky CLI.</p></section></main><script src=\"{script}\"></script></body></html>",
            snapshot.filters.start_date, snapshot.filters.end_date
        );
    };
    let amount =
        |value: &str| format!("<strong data-minor=\"{value}\">{value} {currency}</strong>");
    let rows = snapshot
        .monthly
        .iter()
        .map(|month| format!(
            "<tr><th scope=\"row\">{}</th><td data-minor=\"{}\">{} {currency}</td><td data-minor=\"{}\">{} {currency}</td><td data-minor=\"{}\">{} {currency}</td><td data-minor=\"{}\">{} {currency}</td></tr>",
            month.month,
            month.measures.income_minor,
            month.measures.income_minor,
            month.measures.consumption_expense_minor,
            month.measures.consumption_expense_minor,
            month.measures.net_cash_flow_minor,
            month.measures.net_cash_flow_minor,
            month.measures.investment_contribution_minor,
            month.measures.investment_contribution_minor
        ))
        .collect::<String>();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Tracky Monthly ledger</title><link rel=\"stylesheet\" href=\"{stylesheet}\"></head><body><main><header><h1>Monthly ledger</h1><p class=\"scope\">{} through {} · {currency}</p></header><ul class=\"summary\"><li>Income{}</li><li>Consumption expense{}</li><li>Savings / net cash flow{}</li><li>Investment contributions{}</li></ul><section aria-labelledby=\"monthly-heading\"><h2 id=\"monthly-heading\">Monthly activity</h2><table><caption>Exact monthly amounts in minor units</caption><thead><tr><th scope=\"col\">Month</th><th scope=\"col\">Income</th><th scope=\"col\">Consumption expense</th><th scope=\"col\">Savings</th><th scope=\"col\">Investment contributions</th></tr></thead><tbody>{rows}</tbody></table></section><p>Use the Tracky CLI to review or correct canonical data.</p></main><script src=\"{script}\"></script></body></html>",
        snapshot.filters.start_date,
        snapshot.filters.end_date,
        amount(&snapshot.summary.income_minor),
        amount(&snapshot.summary.consumption_expense_minor),
        amount(&snapshot.summary.net_cash_flow_minor),
        amount(&snapshot.summary.investment_contribution_minor),
    )
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
