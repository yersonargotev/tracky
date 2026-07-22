use crate::investment_reports;
use anyhow::{bail, Result};
use chrono::{Datelike, Months, NaiveDate, Utc};
use rusqlite::{params, Connection, Transaction};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const DASHBOARD_SCHEMA_VERSION: &str = "tracky.dashboard.v1";

#[derive(Debug)]
pub(crate) struct UnavailableCurrencyError;

impl std::fmt::Display for UnavailableCurrencyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("requested currency is not available in this date range")
    }
}

impl std::error::Error for UnavailableCurrencyError {}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FinanceFilterRequest {
    pub start_date: String,
    pub end_date: String,
    pub currency: Option<String>,
    pub account_ids: Option<Vec<String>>,
    pub category_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DashboardResponse {
    pub schema_version: &'static str,
    pub ok: bool,
    pub read_at: String,
    pub state: &'static str,
    pub filters: ResolvedFilters,
    pub dimensions: Dimensions,
    pub summary: Measures,
    pub monthly: Vec<MonthlyMeasures>,
    pub categories: Vec<CategoryBreakdown>,
    pub accounts: Vec<AccountBreakdown>,
    pub investments: InvestmentSection,
    pub alerts: Vec<DashboardAlert>,
    pub errors: Vec<DashboardError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ResolvedFilters {
    pub start_date: String,
    pub end_date: String,
    pub currency: Option<String>,
    pub account_ids: Vec<String>,
    pub category_ids: Vec<String>,
    pub category_scope: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct Dimensions {
    pub currencies: Vec<String>,
    pub accounts: Vec<AccountDimension>,
    pub categories: Vec<CategoryDimension>,
    pub instruments: Vec<InstrumentDimension>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AccountDimension {
    pub id: String,
    pub name: String,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CategoryDimension {
    pub id: String,
    pub name: String,
    pub currencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct InstrumentDimension {
    pub id: String,
    pub name: String,
    pub instrument_type: String,
    pub denomination_currency: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(crate) struct Measures {
    pub currency: Option<String>,
    pub income_minor: String,
    pub consumption_expense_minor: String,
    pub net_cash_flow_minor: String,
    pub investment_contribution_minor: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct MonthlyMeasures {
    pub month: String,
    #[serde(flatten)]
    pub measures: Measures,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CategoryBreakdown {
    pub category_id: String,
    pub category_name: String,
    pub currency: String,
    pub amount_minor: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AccountBreakdown {
    pub account_id: String,
    pub account_name: String,
    pub row_count: usize,
    #[serde(flatten)]
    pub measures: Measures,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct InvestmentSection {
    pub state: &'static str,
    pub pending_allocation_minor: String,
    pub flows: Vec<InvestmentFlow>,
    pub closing_positions: Vec<ClosingPosition>,
}

impl Default for InvestmentSection {
    fn default() -> Self {
        Self {
            state: "empty",
            pending_allocation_minor: "0".into(),
            flows: Vec::new(),
            closing_positions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct InvestmentFlow {
    pub account_id: String,
    pub currency: String,
    pub amount_minor: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ClosingPosition {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub instrument_type: Option<String>,
    pub quantity: Option<String>,
    pub historical_cost_minor: Option<String>,
    pub cost_currency: Option<String>,
    pub observed_value_minor: Option<String>,
    pub valuation_currency: Option<String>,
    pub effective_date: Option<String>,
    pub observed_at: Option<String>,
    pub age_days: Option<i64>,
    pub freshness: String,
    pub reconciliation_status: String,
    pub quantity_difference: Option<String>,
    pub value_difference_minor: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DashboardAlert {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub account_id: Option<String>,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub effective_date: Option<String>,
    pub observed_at: Option<String>,
    pub age_days: Option<i64>,
    pub pending_amount_minor: Option<String>,
    pub quantity_difference: Option<String>,
    pub value_difference_minor: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DashboardError {
    pub code: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrillMetric {
    Activity,
    Income,
    ConsumptionExpense,
    NetCashFlow,
    InvestmentContribution,
}

impl DrillMetric {
    fn name(self) -> &'static str {
        match self {
            Self::Activity => "activity",
            Self::Income => "income",
            Self::ConsumptionExpense => "consumption_expense",
            Self::NetCashFlow => "net_cash_flow",
            Self::InvestmentContribution => "investment_contribution",
        }
    }

    fn includes_canonical(self, kind: &str, amount: i64, category_filtered: bool) -> bool {
        match self {
            Self::Activity => !category_filtered || kind != "expense",
            Self::Income => kind == "income" && amount > 0,
            Self::ConsumptionExpense => kind == "expense" && !category_filtered,
            Self::NetCashFlow => {
                matches!(kind, "income" | "expense") && (!category_filtered || kind != "expense")
            }
            Self::InvestmentContribution => kind == "investment_contribution",
        }
    }

    fn includes_expense_lines(self) -> bool {
        matches!(
            self,
            Self::Activity | Self::ConsumptionExpense | Self::NetCashFlow
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DrillRequest {
    pub filters: FinanceFilterRequest,
    pub metric: DrillMetric,
    pub month: Option<String>,
    pub cursor: Option<String>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DrillResponse {
    pub schema_version: &'static str,
    pub ok: bool,
    pub filters: ResolvedFilters,
    pub metric: &'static str,
    pub rows: Vec<DrillRow>,
    pub next_cursor: Option<String>,
    pub errors: Vec<DashboardError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct DrillRow {
    pub row_type: &'static str,
    pub id: String,
    pub canonical_transaction_id: String,
    pub posted_date: String,
    pub account_id: Option<String>,
    pub description: String,
    pub transaction_kind: String,
    pub category_id: Option<String>,
    pub amount_minor: String,
    pub currency: String,
}

#[derive(Debug, Clone)]
struct TransactionRow {
    id: String,
    account_id: Option<String>,
    posted_date: String,
    description: String,
    amount_minor: i64,
    currency: String,
    kind: String,
}

#[derive(Debug, Clone)]
struct ExpenseLineRow {
    id: String,
    transaction_id: String,
    account_id: Option<String>,
    posted_date: String,
    description: String,
    category_id: String,
    category_name: String,
    amount_minor: i64,
    currency: String,
}

#[derive(Debug, Clone, Copy, Default)]
struct NumericMeasures {
    income: i64,
    expense: i64,
    contribution: i64,
}

struct FinanceSections {
    summary: Measures,
    monthly: Vec<MonthlyMeasures>,
    categories: Vec<CategoryBreakdown>,
    accounts: Vec<AccountBreakdown>,
}

impl NumericMeasures {
    fn add_income(&mut self, amount: i64) -> Result<()> {
        self.income = self
            .income
            .checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
        Ok(())
    }

    fn add_contribution(&mut self, amount: i64) -> Result<()> {
        self.contribution = self
            .contribution
            .checked_add(magnitude(amount)?)
            .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
        Ok(())
    }

    fn add_expense_magnitude(&mut self, amount: i64) -> Result<()> {
        self.expense = self
            .expense
            .checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
        Ok(())
    }

    fn transport(self, currency: Option<&str>) -> Result<Measures> {
        let net = self
            .income
            .checked_sub(self.expense)
            .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
        Ok(Measures {
            currency: currency.map(str::to_string),
            income_minor: self.income.to_string(),
            consumption_expense_minor: self.expense.to_string(),
            net_cash_flow_minor: net.to_string(),
            investment_contribution_minor: self.contribution.to_string(),
        })
    }
}

pub(crate) fn read_finance(
    connection: &Transaction<'_>,
    request: FinanceFilterRequest,
) -> Result<DashboardResponse> {
    validate_range(&request.start_date, &request.end_date)?;
    let transactions = load_transactions(connection, &request.start_date, &request.end_date)?;
    let lines = load_expense_lines(connection, &request.start_date, &request.end_date)?;
    let dimensions = load_dimensions(connection, &transactions, &lines)?;
    let filters = resolve_filters(&request, &dimensions)?;
    let explicit_empty = request.account_ids.as_ref().is_some_and(Vec::is_empty)
        || request.category_ids.as_ref().is_some_and(Vec::is_empty);
    let incompatible_empty = request
        .account_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty() && filters.account_ids.is_empty())
        || request
            .category_ids
            .as_ref()
            .is_some_and(|ids| !ids.is_empty() && filters.category_ids.is_empty());
    let filter_empty = filters.currency.is_some() && (explicit_empty || incompatible_empty);
    let state = if filters.currency.is_none() {
        "empty"
    } else if filter_empty {
        "filter_empty"
    } else {
        "ready"
    };
    let sections = if state == "ready" {
        aggregate(&transactions, &lines, &filters, &dimensions)?
    } else {
        FinanceSections {
            summary: NumericMeasures::default().transport(filters.currency.as_deref())?,
            monthly: if state == "filter_empty" {
                zero_months(&filters)?
            } else {
                Vec::new()
            },
            categories: Vec::new(),
            accounts: Vec::new(),
        }
    };
    let (investments, alerts) = if state == "ready" {
        project_investments(connection, &filters)?
    } else {
        (InvestmentSection::default(), Vec::new())
    };
    Ok(DashboardResponse {
        schema_version: DASHBOARD_SCHEMA_VERSION,
        ok: true,
        read_at: Utc::now().to_rfc3339(),
        state,
        filters,
        dimensions,
        summary: sections.summary,
        monthly: sections.monthly,
        categories: sections.categories,
        accounts: sections.accounts,
        investments,
        alerts,
        errors: Vec::new(),
    })
}

fn project_investments(
    connection: &Connection,
    filters: &ResolvedFilters,
) -> Result<(InvestmentSection, Vec<DashboardAlert>)> {
    let report = investment_reports::report_filtered(
        connection,
        &filters.start_date,
        &filters.end_date,
        filters.currency.as_deref(),
        &filters.account_ids,
    )?;
    if !report.ok {
        bail!("dashboard investment report could not be read");
    }

    let mut flows = BTreeMap::<(String, String), i64>::new();
    for acquisition in report.acquisitions_and_reinvestment.by_instrument {
        let value = flows
            .entry((acquisition.account_id, acquisition.currency))
            .or_default();
        *value = value
            .checked_add(acquisition.amount_minor)
            .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
    }
    let flows: Vec<InvestmentFlow> = flows
        .into_iter()
        .map(|((account_id, currency), amount_minor)| InvestmentFlow {
            account_id,
            currency,
            amount_minor: amount_minor.to_string(),
        })
        .collect();

    let end = NaiveDate::parse_from_str(&filters.end_date, "%Y-%m-%d")?;
    let mut alerts = BTreeMap::<String, DashboardAlert>::new();
    let mut closing_positions = Vec::new();
    for position in report.closing_positions {
        let instrument_key = position.instrument_id.as_deref().unwrap_or("cash");
        let age_days = position
            .observed_at
            .as_deref()
            .and_then(|observed| observed.get(..10))
            .and_then(|observed| NaiveDate::parse_from_str(observed, "%Y-%m-%d").ok())
            .map(|observed| end.signed_duration_since(observed).num_days());
        let underlying_status = if position.reconciliation_status == "stale" {
            position
                .original_status
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            position.reconciliation_status.clone()
        };
        let quantity_difference = position.current_quantity_difference.clone();
        let value_difference_minor = position.current_value_difference_minor;

        if position.freshness == "stale" {
            let currency = position_alert_currency(&position);
            insert_alert(
                &mut alerts,
                DashboardAlert {
                    id: format!("stale-valuation:{}:{instrument_key}", position.account_id),
                    kind: "stale_valuation".into(),
                    severity: "warning".into(),
                    account_id: Some(position.account_id.clone()),
                    instrument_id: position.instrument_id.clone(),
                    currency,
                    effective_date: position.effective_date.clone(),
                    observed_at: position.observed_at.clone(),
                    age_days,
                    pending_amount_minor: None,
                    quantity_difference: None,
                    value_difference_minor: None,
                },
            );
        }
        if matches!(
            underlying_status.as_str(),
            "no_snapshot" | "missing_snapshot_position"
        ) {
            insert_position_alert(
                &mut alerts,
                "missing-snapshot-position",
                "missing_snapshot_position",
                &position,
                age_days,
                None,
                None,
            );
        }
        if position.observed_value_minor.is_none() {
            insert_position_alert(
                &mut alerts,
                "missing-valuation",
                "missing_valuation",
                &position,
                age_days,
                None,
                None,
            );
        }
        if !matches!(
            underlying_status.as_str(),
            "matched" | "no_snapshot" | "missing_snapshot_position" | "valuation_unavailable"
        ) {
            let currency = position_alert_currency(&position);
            let id = format!(
                "reconciliation-difference:{}:{instrument_key}:{}",
                position.account_id, currency
            );
            insert_alert(
                &mut alerts,
                DashboardAlert {
                    id,
                    kind: "reconciliation_difference".into(),
                    severity: "warning".into(),
                    account_id: Some(position.account_id.clone()),
                    instrument_id: position.instrument_id.clone(),
                    currency,
                    effective_date: position.effective_date.clone(),
                    observed_at: position.observed_at.clone(),
                    age_days,
                    pending_amount_minor: None,
                    quantity_difference: quantity_difference.clone(),
                    value_difference_minor: value_difference_minor.map(|value| value.to_string()),
                },
            );
        }

        closing_positions.push(ClosingPosition {
            account_id: position.account_id,
            instrument_id: position.instrument_id,
            instrument_type: position.instrument_type,
            quantity: position.quantity,
            historical_cost_minor: position
                .historical_cost_minor
                .map(|value| value.to_string()),
            cost_currency: position.cost_currency,
            observed_value_minor: position.observed_value_minor.map(|value| value.to_string()),
            valuation_currency: position.valuation_currency,
            effective_date: position.effective_date,
            observed_at: position.observed_at,
            age_days,
            freshness: position.freshness,
            reconciliation_status: underlying_status,
            quantity_difference,
            value_difference_minor: value_difference_minor.map(|value| value.to_string()),
        });
    }

    let mut pending_allocation_minor = 0_i64;
    for allocation in report.pending_and_reconciliation.allocations {
        pending_allocation_minor = pending_allocation_minor
            .checked_add(allocation.unallocated_minor)
            .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
        let account_id = connection
            .query_row(
                "SELECT account_id FROM canonical_transactions WHERE id=?1",
                [&allocation.contribution_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .unwrap_or(None);
        insert_alert(
            &mut alerts,
            DashboardAlert {
                id: format!("pending-allocation:{}", allocation.contribution_id),
                kind: "pending_allocation".into(),
                severity: "warning".into(),
                account_id,
                instrument_id: None,
                currency: allocation.currency,
                effective_date: None,
                observed_at: None,
                age_days: None,
                pending_amount_minor: Some(allocation.unallocated_minor.to_string()),
                quantity_difference: None,
                value_difference_minor: None,
            },
        );
    }
    for event in report.pending_and_reconciliation.provider_events {
        insert_alert(
            &mut alerts,
            DashboardAlert {
                id: format!("pending-provider-event:{}", event.event_id),
                kind: "pending_provider_event".into(),
                severity: "warning".into(),
                account_id: None,
                instrument_id: None,
                currency: event.currency,
                effective_date: Some(event.effective_date),
                observed_at: None,
                age_days: None,
                pending_amount_minor: None,
                quantity_difference: None,
                value_difference_minor: None,
            },
        );
    }

    let state = if closing_positions
        .iter()
        .any(|position| position.freshness == "stale")
    {
        "stale"
    } else if closing_positions
        .iter()
        .any(|position| position.observed_value_minor.is_none())
        || (closing_positions.is_empty() && pending_allocation_minor > 0)
    {
        "unavailable"
    } else if closing_positions.is_empty() && flows.is_empty() {
        "empty"
    } else {
        "ready"
    };
    Ok((
        InvestmentSection {
            state,
            pending_allocation_minor: pending_allocation_minor.to_string(),
            flows,
            closing_positions,
        },
        alerts.into_values().collect(),
    ))
}

fn insert_position_alert(
    alerts: &mut BTreeMap<String, DashboardAlert>,
    id_kind: &str,
    kind: &str,
    position: &investment_reports::ClosingPosition,
    age_days: Option<i64>,
    quantity_difference: Option<String>,
    value_difference_minor: Option<String>,
) {
    let instrument_key = position.instrument_id.as_deref().unwrap_or("cash");
    insert_alert(
        alerts,
        DashboardAlert {
            id: format!("{id_kind}:{}:{instrument_key}", position.account_id),
            kind: kind.into(),
            severity: "warning".into(),
            account_id: Some(position.account_id.clone()),
            instrument_id: position.instrument_id.clone(),
            currency: position_alert_currency(position),
            effective_date: position.effective_date.clone(),
            observed_at: position.observed_at.clone(),
            age_days,
            pending_amount_minor: None,
            quantity_difference,
            value_difference_minor,
        },
    );
}

fn position_alert_currency(position: &investment_reports::ClosingPosition) -> String {
    position
        .cost_currency
        .clone()
        .or_else(|| position.valuation_currency.clone())
        .unwrap_or_default()
}

fn insert_alert(alerts: &mut BTreeMap<String, DashboardAlert>, alert: DashboardAlert) {
    alerts.insert(alert.id.clone(), alert);
}

pub(crate) fn read_drill_down(
    connection: &Transaction<'_>,
    request: DrillRequest,
) -> Result<DrillResponse> {
    if !(1..=100).contains(&request.limit) {
        bail!("dashboard drill-down limit must be between 1 and 100");
    }
    let dashboard = read_finance(connection, request.filters)?;
    let metric_name = request.metric.name();
    if dashboard.state != "ready" {
        return Ok(DrillResponse {
            schema_version: DASHBOARD_SCHEMA_VERSION,
            ok: true,
            filters: dashboard.filters,
            metric: metric_name,
            rows: Vec::new(),
            next_cursor: None,
            errors: Vec::new(),
        });
    }
    let filters = dashboard.filters;
    let mut transactions = load_transactions(connection, &filters.start_date, &filters.end_date)?;
    transactions.retain(|row| {
        row.currency == filters.currency.as_deref().unwrap_or_default()
            && row
                .account_id
                .as_ref()
                .is_some_and(|id| filters.account_ids.contains(id))
            && request
                .month
                .as_ref()
                .is_none_or(|month| row.posted_date.starts_with(month))
    });
    let category_filter_applied = filters.category_scope == "expense_only";
    let mut rows = Vec::new();
    for transaction in transactions {
        let include_canonical = request.metric.includes_canonical(
            &transaction.kind,
            transaction.amount_minor,
            category_filter_applied,
        );
        if include_canonical {
            rows.push(DrillRow {
                row_type: "canonical_transaction",
                id: transaction.id.clone(),
                canonical_transaction_id: transaction.id,
                posted_date: transaction.posted_date,
                account_id: transaction.account_id,
                description: transaction.description,
                transaction_kind: transaction.kind,
                category_id: None,
                amount_minor: transaction.amount_minor.to_string(),
                currency: transaction.currency,
            });
        }
    }
    if category_filter_applied && request.metric.includes_expense_lines() {
        for line in load_expense_lines(connection, &filters.start_date, &filters.end_date)? {
            if line.currency == filters.currency.as_deref().unwrap_or_default()
                && line
                    .account_id
                    .as_ref()
                    .is_some_and(|id| filters.account_ids.contains(id))
                && filters.category_ids.contains(&line.category_id)
                && request
                    .month
                    .as_ref()
                    .is_none_or(|month| line.posted_date.starts_with(month))
            {
                rows.push(DrillRow {
                    row_type: "expense_line",
                    id: line.id,
                    canonical_transaction_id: line.transaction_id,
                    posted_date: line.posted_date,
                    account_id: line.account_id,
                    description: line.description,
                    transaction_kind: "expense".to_string(),
                    category_id: Some(line.category_id),
                    amount_minor: line.amount_minor.to_string(),
                    currency: line.currency,
                });
            }
        }
    }
    rows.sort_by(|left, right| (&left.posted_date, &left.id).cmp(&(&right.posted_date, &right.id)));
    if let Some(cursor) = request.cursor.as_deref() {
        let (date, id) = parse_cursor(cursor)?;
        rows.retain(|row| (row.posted_date.as_str(), row.id.as_str()) > (date, id));
    }
    let has_more = rows.len() > request.limit;
    rows.truncate(request.limit);
    let next_cursor = has_more.then(|| {
        let last = rows.last().expect("a non-empty page has a last row");
        format!("{}|{}", last.posted_date, last.id)
    });
    Ok(DrillResponse {
        schema_version: DASHBOARD_SCHEMA_VERSION,
        ok: true,
        filters,
        metric: metric_name,
        rows,
        next_cursor,
        errors: Vec::new(),
    })
}

fn aggregate(
    transactions: &[TransactionRow],
    lines: &[ExpenseLineRow],
    filters: &ResolvedFilters,
    dimensions: &Dimensions,
) -> Result<FinanceSections> {
    let currency = filters.currency.as_deref().expect("ready has currency");
    let category_filter_applied = filters.category_scope == "expense_only";
    let selected_accounts: BTreeSet<&str> =
        filters.account_ids.iter().map(String::as_str).collect();
    let selected_categories: BTreeSet<&str> =
        filters.category_ids.iter().map(String::as_str).collect();
    let mut expense_by_transaction: BTreeMap<&str, i64> = BTreeMap::new();
    let mut category_totals: BTreeMap<&str, i64> = BTreeMap::new();
    for line in lines.iter().filter(|line| {
        line.currency == currency
            && line
                .account_id
                .as_deref()
                .is_some_and(|id| selected_accounts.contains(id))
            && (!category_filter_applied || selected_categories.contains(line.category_id.as_str()))
    }) {
        let amount = magnitude(line.amount_minor)?;
        checked_add_map(
            &mut expense_by_transaction,
            line.transaction_id.as_str(),
            amount,
        )?;
        checked_add_map(&mut category_totals, line.category_id.as_str(), amount)?;
    }
    let mut summary = NumericMeasures::default();
    let mut months: BTreeMap<String, NumericMeasures> =
        month_buckets(&filters.start_date, &filters.end_date)?;
    let mut account_measures: BTreeMap<&str, NumericMeasures> = BTreeMap::new();
    let mut account_rows: BTreeMap<&str, usize> = BTreeMap::new();
    for transaction in transactions.iter().filter(|row| {
        row.currency == currency
            && row
                .account_id
                .as_deref()
                .is_some_and(|id| selected_accounts.contains(id))
    }) {
        let account = transaction
            .account_id
            .as_deref()
            .expect("selected account exists");
        let month = &transaction.posted_date[..7];
        let monthly = months.get_mut(month).expect("date range created its month");
        let account_total = account_measures.entry(account).or_default();
        match transaction.kind.as_str() {
            "income" if transaction.amount_minor > 0 => {
                summary.add_income(transaction.amount_minor)?;
                monthly.add_income(transaction.amount_minor)?;
                account_total.add_income(transaction.amount_minor)?;
                *account_rows.entry(account).or_default() += 1;
            }
            "expense" => {
                let amount = if category_filter_applied {
                    expense_by_transaction
                        .get(transaction.id.as_str())
                        .copied()
                        .unwrap_or(0)
                } else {
                    magnitude(transaction.amount_minor)?
                };
                summary.add_expense_magnitude(amount)?;
                monthly.add_expense_magnitude(amount)?;
                account_total.add_expense_magnitude(amount)?;
                if amount != 0 {
                    *account_rows.entry(account).or_default() += 1;
                }
            }
            "investment_contribution" => {
                summary.add_contribution(transaction.amount_minor)?;
                monthly.add_contribution(transaction.amount_minor)?;
                account_total.add_contribution(transaction.amount_minor)?;
                *account_rows.entry(account).or_default() += 1;
            }
            _ => {}
        }
    }
    let monthly = months
        .into_iter()
        .map(|(month, numeric)| {
            Ok(MonthlyMeasures {
                month,
                measures: numeric.transport(Some(currency))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let category_names: BTreeMap<&str, &str> = dimensions
        .categories
        .iter()
        .map(|category| (category.id.as_str(), category.name.as_str()))
        .collect();
    let categories = category_totals
        .into_iter()
        .map(|(id, amount)| CategoryBreakdown {
            category_id: id.to_string(),
            category_name: category_names.get(id).copied().unwrap_or(id).to_string(),
            currency: currency.to_string(),
            amount_minor: amount.to_string(),
        })
        .collect();
    let accounts = dimensions
        .accounts
        .iter()
        .filter(|account| selected_accounts.contains(account.id.as_str()))
        .map(|account| {
            Ok(AccountBreakdown {
                account_id: account.id.clone(),
                account_name: account.name.clone(),
                row_count: account_rows.get(account.id.as_str()).copied().unwrap_or(0),
                measures: account_measures
                    .get(account.id.as_str())
                    .copied()
                    .unwrap_or_default()
                    .transport(Some(currency))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(FinanceSections {
        summary: summary.transport(Some(currency))?,
        monthly,
        categories,
        accounts,
    })
}

fn resolve_filters(
    request: &FinanceFilterRequest,
    dimensions: &Dimensions,
) -> Result<ResolvedFilters> {
    let requested_currency = request.currency.as_deref().map(str::to_ascii_uppercase);
    let currency = match requested_currency.as_deref() {
        Some(requested) if dimensions.currencies.iter().any(|value| value == requested) => {
            Some(requested.to_string())
        }
        Some(_) => return Err(UnavailableCurrencyError.into()),
        None => dimensions.currencies.first().cloned(),
    };
    let compatible_accounts: BTreeSet<&str> = dimensions
        .accounts
        .iter()
        .filter(|account| Some(account.currency.as_str()) == currency.as_deref())
        .map(|account| account.id.as_str())
        .collect();
    let requested_accounts = request.account_ids.as_ref();
    let account_ids = requested_accounts
        .map(|ids| {
            ids.iter()
                .filter(|id| compatible_accounts.contains(id.as_str()))
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            compatible_accounts
                .into_iter()
                .map(str::to_string)
                .collect()
        })
        .into_iter()
        .collect();
    let compatible_categories: BTreeSet<&str> = dimensions
        .categories
        .iter()
        .filter(|item| {
            currency
                .as_deref()
                .is_some_and(|currency| item.currencies.iter().any(|value| value == currency))
        })
        .map(|item| item.id.as_str())
        .collect();
    let category_ids = request
        .category_ids
        .as_ref()
        .map(|ids| {
            ids.iter()
                .filter(|id| compatible_categories.contains(id.as_str()))
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            compatible_categories
                .into_iter()
                .map(str::to_string)
                .collect()
        })
        .into_iter()
        .collect();
    Ok(ResolvedFilters {
        start_date: request.start_date.clone(),
        end_date: request.end_date.clone(),
        currency,
        account_ids,
        category_ids,
        category_scope: if request.category_ids.is_some() {
            "expense_only"
        } else {
            "all_expenses"
        },
    })
}

fn load_dimensions(
    connection: &Connection,
    transactions: &[TransactionRow],
    lines: &[ExpenseLineRow],
) -> Result<Dimensions> {
    let currencies = transactions
        .iter()
        .map(|row| row.currency.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut statement = connection
        .prepare("SELECT id, label, currency FROM accounts WHERE is_owned = 1 ORDER BY id")?;
    let accounts = statement
        .query_map([], |row| {
            Ok(AccountDimension {
                id: row.get(0)?,
                name: row.get(1)?,
                currency: row.get::<_, String>(2)?.to_ascii_uppercase(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut category_values: BTreeMap<String, (String, BTreeSet<String>)> = BTreeMap::new();
    for line in lines {
        let entry = category_values
            .entry(line.category_id.clone())
            .or_insert_with(|| (line.category_name.clone(), BTreeSet::new()));
        entry.1.insert(line.currency.clone());
    }
    let mut categories = category_values
        .into_iter()
        .map(|(id, (name, currencies))| CategoryDimension {
            id,
            name,
            currencies: currencies.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| left.id.cmp(&right.id));
    let mut statement = connection.prepare(
        "SELECT id, name, instrument_type, denomination_currency FROM investment_instruments ORDER BY id",
    )?;
    let instruments = statement
        .query_map([], |row| {
            Ok(InstrumentDimension {
                id: row.get(0)?,
                name: row.get(1)?,
                instrument_type: row.get(2)?,
                denomination_currency: row.get::<_, String>(3)?.to_ascii_uppercase(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Dimensions {
        currencies,
        accounts,
        categories,
        instruments,
    })
}

fn load_transactions(
    connection: &Connection,
    start: &str,
    end: &str,
) -> Result<Vec<TransactionRow>> {
    let mut statement = connection.prepare(
        "SELECT id, account_id, posted_date, description, amount_minor, currency, COALESCE(transaction_kind, '') FROM canonical_transactions WHERE posted_date BETWEEN ?1 AND ?2 ORDER BY posted_date, id",
    )?;
    let rows = statement
        .query_map(params![start, end], |row| {
            Ok(TransactionRow {
                id: row.get(0)?,
                account_id: row.get(1)?,
                posted_date: row.get(2)?,
                description: row.get(3)?,
                amount_minor: row.get(4)?,
                currency: row.get::<_, String>(5)?.to_ascii_uppercase(),
                kind: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn load_expense_lines(
    connection: &Connection,
    start: &str,
    end: &str,
) -> Result<Vec<ExpenseLineRow>> {
    let mut statement = connection.prepare(
        "SELECT tl.id, ct.id, ct.account_id, ct.posted_date, ct.description, tl.category_id, c.name, tl.amount_minor, tl.currency FROM transaction_lines tl JOIN canonical_transactions ct ON ct.id = tl.canonical_transaction_id JOIN categories c ON c.id = tl.category_id WHERE ct.transaction_kind = 'expense' AND ct.posted_date BETWEEN ?1 AND ?2 ORDER BY ct.posted_date, tl.id",
    )?;
    let rows = statement
        .query_map(params![start, end], |row| {
            Ok(ExpenseLineRow {
                id: row.get(0)?,
                transaction_id: row.get(1)?,
                account_id: row.get(2)?,
                posted_date: row.get(3)?,
                description: row.get(4)?,
                category_id: row.get(5)?,
                category_name: row.get(6)?,
                amount_minor: row.get(7)?,
                currency: row.get::<_, String>(8)?.to_ascii_uppercase(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn month_buckets(start: &str, end: &str) -> Result<BTreeMap<String, NumericMeasures>> {
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;
    let mut month = NaiveDate::from_ymd_opt(start.year(), start.month(), 1).unwrap();
    let mut buckets = BTreeMap::new();
    while month <= end {
        buckets.insert(
            month.format("%Y-%m").to_string(),
            NumericMeasures::default(),
        );
        month = month
            .checked_add_months(Months::new(1))
            .ok_or_else(|| anyhow::anyhow!("dashboard date range overflow"))?;
    }
    Ok(buckets)
}

fn zero_months(filters: &ResolvedFilters) -> Result<Vec<MonthlyMeasures>> {
    month_buckets(&filters.start_date, &filters.end_date)?
        .into_iter()
        .map(|(month, measures)| {
            Ok(MonthlyMeasures {
                month,
                measures: measures.transport(filters.currency.as_deref())?,
            })
        })
        .collect()
}

fn validate_range(start: &str, end: &str) -> Result<()> {
    let start = NaiveDate::parse_from_str(start, "%Y-%m-%d")?;
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d")?;
    if start > end {
        bail!("dashboard start date must not follow end date");
    }
    Ok(())
}

fn magnitude(amount: i64) -> Result<i64> {
    amount
        .checked_neg()
        .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))
}

fn checked_add_map<'a>(map: &mut BTreeMap<&'a str, i64>, key: &'a str, amount: i64) -> Result<()> {
    let total = map.entry(key).or_default();
    *total = total
        .checked_add(amount)
        .ok_or_else(|| anyhow::anyhow!("dashboard amount overflow"))?;
    Ok(())
}

fn parse_cursor(cursor: &str) -> Result<(&str, &str)> {
    let Some((date, id)) = cursor.split_once('|') else {
        bail!("invalid dashboard cursor");
    };
    if id.is_empty() || NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        bail!("invalid dashboard cursor");
    }
    Ok((date, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::apply_migrations;
    use serde_json::{json, Value};
    use std::fs;

    fn fixture_with_seed(seed: &str) -> (tempfile::TempDir, Connection) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tracky.sqlite3");
        let connection = Connection::open(path).unwrap();
        apply_migrations(&connection).unwrap();
        connection.execute_batch(seed).unwrap();
        (directory, connection)
    }

    fn fixture() -> (tempfile::TempDir, Connection) {
        fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/finance.sql"
        ))
    }

    fn request() -> FinanceFilterRequest {
        FinanceFilterRequest {
            start_date: "2026-01-01".to_string(),
            end_date: "2026-03-31".to_string(),
            currency: Some("COP".to_string()),
            ..FinanceFilterRequest::default()
        }
    }

    fn finance(
        connection: &mut Connection,
        request: FinanceFilterRequest,
    ) -> Result<DashboardResponse> {
        let transaction = connection.transaction().unwrap();
        read_finance(&transaction, request)
    }

    fn drill(connection: &mut Connection, request: DrillRequest) -> Result<DrillResponse> {
        let transaction = connection.transaction().unwrap();
        read_drill_down(&transaction, request)
    }

    #[test]
    fn finance_model_matches_the_manual_monthly_oracle_and_uses_string_transport() {
        let (directory, mut connection) = fixture();
        let database = directory.path().join("tracky.sqlite3");
        let before = fs::read(&database).unwrap();
        let response = finance(&mut connection, request()).unwrap();
        assert_eq!(fs::read(&database).unwrap(), before);
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/dashboard/oracles/finance.json"
        ))
        .unwrap();
        let mut actual = serde_json::to_value(&response).unwrap();
        assert_eq!(response.schema_version, DASHBOARD_SCHEMA_VERSION);
        assert_eq!(
            response.summary.income_minor,
            oracle["expected"]["summary"]["income_minor"]
        );
        assert_eq!(
            response.summary.consumption_expense_minor,
            oracle["expected"]["summary"]["consumption_expense_minor"]
        );
        assert_eq!(
            response.summary.net_cash_flow_minor,
            oracle["expected"]["summary"]["savings_minor"]
        );
        assert_eq!(
            response.summary.investment_contribution_minor,
            oracle["expected"]["summary"]["investment_contribution_minor"]
        );
        for (actual, expected) in response
            .monthly
            .iter()
            .zip(oracle["expected"]["months"].as_array().unwrap())
        {
            assert_eq!(actual.month, expected["month"]);
            assert_eq!(actual.measures.income_minor, expected["income_minor"]);
            assert_eq!(
                actual.measures.consumption_expense_minor,
                expected["consumption_expense_minor"]
            );
            assert_eq!(
                actual.measures.net_cash_flow_minor,
                expected["savings_minor"]
            );
            assert_eq!(
                actual.measures.investment_contribution_minor,
                expected["investment_contribution_minor"]
            );
        }
        for (actual, expected) in response
            .categories
            .iter()
            .zip(oracle["expected"]["category_breakdown"].as_array().unwrap())
        {
            assert_eq!(actual.category_id, expected["category_id"]);
            assert_eq!(actual.amount_minor, expected["amount_minor"]);
        }
        let account_income = response
            .accounts
            .iter()
            .map(|account| account.measures.income_minor.parse::<i64>().unwrap())
            .sum::<i64>();
        let account_expense = response
            .accounts
            .iter()
            .map(|account| {
                account
                    .measures
                    .consumption_expense_minor
                    .parse::<i64>()
                    .unwrap()
            })
            .sum::<i64>();
        let category_expense = response
            .categories
            .iter()
            .map(|category| category.amount_minor.parse::<i64>().unwrap())
            .sum::<i64>();
        assert_eq!(account_income.to_string(), response.summary.income_minor);
        assert_eq!(
            account_expense.to_string(),
            response.summary.consumption_expense_minor
        );
        assert_eq!(category_expense, account_expense);
        assert!(actual["summary"]["income_minor"].is_string());
        assert!(response.investments.flows.is_empty());
        assert_eq!(response.alerts.len(), 1);
        actual.as_object_mut().unwrap().remove("read_at");
        assert_eq!(
            actual,
            json!({
                "schema_version": "tracky.dashboard.v1",
                "ok": true,
                "state": "ready",
                "filters": {
                    "start_date": "2026-01-01",
                    "end_date": "2026-03-31",
                    "currency": "COP",
                    "account_ids": ["cop-checking", "cop-investment", "cop-savings"],
                    "category_ids": ["food", "housing"],
                    "category_scope": "all_expenses"
                },
                "dimensions": {
                    "currencies": ["COP", "USD"],
                    "accounts": [
                        {"id": "cop-checking", "name": "COP Checking", "currency": "COP"},
                        {"id": "cop-investment", "name": "COP Investment", "currency": "COP"},
                        {"id": "cop-savings", "name": "COP Savings", "currency": "COP"},
                        {"id": "usd-checking", "name": "USD Checking", "currency": "USD"}
                    ],
                    "categories": [
                        {"id": "food", "name": "Food", "currencies": ["COP"]},
                        {"id": "housing", "name": "Housing", "currencies": ["COP"]}
                    ],
                    "instruments": []
                },
                "summary": {
                    "currency": "COP", "income_minor": "500000",
                    "consumption_expense_minor": "170000", "net_cash_flow_minor": "330000",
                    "investment_contribution_minor": "100000"
                },
                "monthly": [
                    {"month": "2026-01", "currency": "COP", "income_minor": "500000", "consumption_expense_minor": "120000", "net_cash_flow_minor": "380000", "investment_contribution_minor": "0"},
                    {"month": "2026-02", "currency": "COP", "income_minor": "0", "consumption_expense_minor": "50000", "net_cash_flow_minor": "-50000", "investment_contribution_minor": "100000"},
                    {"month": "2026-03", "currency": "COP", "income_minor": "0", "consumption_expense_minor": "0", "net_cash_flow_minor": "0", "investment_contribution_minor": "0"}
                ],
                "categories": [
                    {"category_id": "food", "category_name": "Food", "currency": "COP", "amount_minor": "120000"},
                    {"category_id": "housing", "category_name": "Housing", "currency": "COP", "amount_minor": "50000"}
                ],
                "accounts": [
                    {"account_id": "cop-checking", "account_name": "COP Checking", "row_count": 3, "currency": "COP", "income_minor": "500000", "consumption_expense_minor": "150000", "net_cash_flow_minor": "350000", "investment_contribution_minor": "0"},
                    {"account_id": "cop-investment", "account_name": "COP Investment", "row_count": 1, "currency": "COP", "income_minor": "0", "consumption_expense_minor": "0", "net_cash_flow_minor": "0", "investment_contribution_minor": "100000"},
                    {"account_id": "cop-savings", "account_name": "COP Savings", "row_count": 1, "currency": "COP", "income_minor": "0", "consumption_expense_minor": "20000", "net_cash_flow_minor": "-20000", "investment_contribution_minor": "0"}
                ],
                "investments": {
                    "state": "unavailable",
                    "pending_allocation_minor": "100000",
                    "flows": [],
                    "closing_positions": []
                },
                "alerts": [{
                    "id": "pending-allocation:cop-invest-feb",
                    "kind": "pending_allocation",
                    "severity": "warning",
                    "account_id": "cop-investment",
                    "instrument_id": null,
                    "currency": "COP",
                    "effective_date": null,
                    "observed_at": null,
                    "age_days": null,
                    "pending_amount_minor": "100000",
                    "quantity_difference": null,
                    "value_difference_minor": null
                }],
                "errors": []
            })
        );
    }

    #[test]
    fn composed_filters_apply_categories_only_to_expenses_and_clear_incompatible_ids() {
        let (_directory, mut connection) = fixture();
        connection
            .execute_batch(
                "INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind) VALUES
                 ('transfer-out', 'cop-checking', '2026-02-10', 'Synthetic transfer', -1000, 'COP', 'transfer'),
                 ('transfer-in', 'cop-savings', '2026-02-10', 'Synthetic transfer', 1000, 'COP', 'transfer');",
            )
            .unwrap();
        let response = finance(
            &mut connection,
            FinanceFilterRequest {
                account_ids: Some(vec!["cop-checking".to_string(), "usd-checking".to_string()]),
                category_ids: Some(vec!["food".to_string()]),
                ..request()
            },
        )
        .unwrap();
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/dashboard/oracles/filtering.json"
        ))
        .unwrap();
        assert_eq!(response.filters.account_ids, vec!["cop-checking"]);
        assert_eq!(
            response.summary.income_minor,
            oracle["expected"]["summary"]["income_minor"]
        );
        assert_eq!(
            response.summary.consumption_expense_minor,
            oracle["expected"]["summary"]["consumption_expense_minor"]
        );
        assert_eq!(
            response.summary.net_cash_flow_minor,
            oracle["expected"]["summary"]["savings_minor"]
        );
        assert_eq!(
            response.summary.investment_contribution_minor,
            oracle["expected"]["summary"]["investment_contribution_minor"]
        );

        let empty = finance(
            &mut connection,
            FinanceFilterRequest {
                account_ids: Some(Vec::new()),
                ..request()
            },
        )
        .unwrap();
        assert_eq!(empty.state, "filter_empty");
        assert_eq!(empty.monthly.len(), 3);

        let usd = finance(
            &mut connection,
            FinanceFilterRequest {
                currency: Some("USD".to_string()),
                ..request()
            },
        )
        .unwrap();
        assert_eq!(usd.summary.currency.as_deref(), Some("USD"));
        assert_eq!(usd.summary.income_minor, "10000");
        assert_eq!(usd.filters.account_ids, vec!["usd-checking"]);

        connection
            .execute_batch(
                "UPDATE accounts SET currency = 'usd' WHERE id = 'usd-checking';
                 UPDATE canonical_transactions SET currency = 'usd' WHERE id = 'usd-income-jan';",
            )
            .unwrap();
        let canonicalized = finance(
            &mut connection,
            FinanceFilterRequest {
                currency: Some("usd".to_string()),
                ..request()
            },
        )
        .unwrap();
        assert_eq!(canonicalized.summary.currency.as_deref(), Some("USD"));
        assert_eq!(canonicalized.dimensions.currencies, vec!["COP", "USD"]);
        assert_eq!(
            canonicalized.accounts[0].measures.currency.as_deref(),
            Some("USD")
        );
        assert!(empty
            .monthly
            .iter()
            .all(|month| month.measures.net_cash_flow_minor == "0"));

        let empty = finance(
            &mut connection,
            FinanceFilterRequest {
                category_ids: Some(Vec::new()),
                ..request()
            },
        )
        .unwrap();
        assert_eq!(empty.state, "filter_empty");
        assert_eq!(empty.monthly.len(), 3);
    }

    #[test]
    fn canonical_activity_pagination_matches_the_manual_stable_cursor_oracle() {
        let (_directory, mut connection) = fixture();
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/dashboard/oracles/pagination.json"
        ))
        .unwrap();
        let mut cursor = None;
        for page_name in ["page_1", "page_2", "page_3"] {
            let page = drill(
                &mut connection,
                DrillRequest {
                    filters: request(),
                    metric: DrillMetric::Activity,
                    month: None,
                    cursor,
                    limit: 2,
                },
            )
            .unwrap();
            let ids = page
                .rows
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>();
            let expected_ids = oracle["expected"][page_name]["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(ids, expected_ids);
            assert_eq!(
                page.next_cursor,
                oracle["expected"][page_name]["next_cursor"]
                    .as_str()
                    .map(str::to_string)
            );
            cursor = page.next_cursor;
        }
    }

    #[test]
    fn category_drill_down_returns_matching_expense_lines_not_whole_transactions() {
        let (_directory, mut connection) = fixture();
        let page = drill(
            &mut connection,
            DrillRequest {
                filters: FinanceFilterRequest {
                    category_ids: Some(vec!["food".to_string()]),
                    ..request()
                },
                metric: DrillMetric::ConsumptionExpense,
                month: Some("2026-01".to_string()),
                cursor: None,
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].row_type, "expense_line");
        assert_eq!(page.rows[0].amount_minor, "-70000");

        for metric in [
            DrillMetric::Income,
            DrillMetric::NetCashFlow,
            DrillMetric::InvestmentContribution,
        ] {
            let response = drill(
                &mut connection,
                DrillRequest {
                    filters: request(),
                    metric,
                    month: None,
                    cursor: None,
                    limit: 10,
                },
            )
            .unwrap();
            assert!(!response.rows.is_empty());
        }
    }

    #[test]
    fn valid_empty_and_checked_overflow_match_the_manual_adverse_state_oracles() {
        let (_directory, mut empty_connection) =
            fixture_with_seed(include_str!("../tests/fixtures/dashboard/seeds/empty.sql"));
        let empty = finance(&mut empty_connection, request()).unwrap_err();
        assert_eq!(
            empty.to_string(),
            "requested currency is not available in this date range"
        );
        let valid_empty = finance(
            &mut empty_connection,
            FinanceFilterRequest {
                currency: None,
                ..request()
            },
        )
        .unwrap();
        assert_eq!(valid_empty.state, "empty");
        assert!(valid_empty.dimensions.currencies.is_empty());
        assert!(valid_empty.monthly.is_empty());

        let (_directory, mut overflow_connection) = fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/overflow.sql"
        ));
        overflow_connection
            .execute(
                "DELETE FROM canonical_transactions WHERE id = 'one-income'",
                [],
            )
            .unwrap();
        let boundary = finance(&mut overflow_connection, request()).unwrap();
        assert_eq!(boundary.summary.income_minor, i64::MAX.to_string());
        overflow_connection
            .execute(
                "INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind, income_source_id, income_kind) VALUES ('one-income', 'cop-checking', '2026-01-02', 'Synthetic overflow addend', 1, 'COP', 'income', 'synthetic-income', 'other')",
                [],
            )
            .unwrap();
        let overflow = finance(&mut overflow_connection, request()).unwrap_err();
        assert_eq!(overflow.to_string(), "dashboard amount overflow");
    }

    #[test]
    fn investment_projection_matches_canonical_report_with_exact_transport() {
        let (_directory, mut connection) = fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/investment.sql"
        ));
        let response = finance(
            &mut connection,
            FinanceFilterRequest {
                start_date: "2026-01-01".into(),
                end_date: "2026-02-28".into(),
                currency: Some("COP".into()),
                ..FinanceFilterRequest::default()
            },
        )
        .unwrap();
        let oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/dashboard/oracles/investment.json"
        ))
        .unwrap();
        let actual = serde_json::to_value(&response).unwrap();
        let position = &actual["investments"]["closing_positions"][0];
        assert_eq!(
            position["instrument_id"],
            oracle["expected"]["position"]["instrument_id"]
        );
        assert_eq!(
            position["quantity"],
            oracle["expected"]["position"]["quantity"]
        );
        assert_eq!(
            position["historical_cost_minor"],
            oracle["expected"]["position"]["historical_cost_minor"]
        );
        assert_eq!(
            position["observed_value_minor"],
            oracle["expected"]["position"]["observed_value_minor"]
        );
        assert_eq!(
            position["valuation_currency"],
            oracle["expected"]["position"]["valuation_currency"]
        );
        assert_eq!(
            position["effective_date"],
            oracle["expected"]["position"]["valuation_date"]
        );
        assert_eq!(
            position["freshness"],
            oracle["expected"]["position"]["freshness"]
        );
        assert_eq!(
            actual["investments"],
            json!({
                "state": "ready",
                "pending_allocation_minor": "0",
                "flows": [{
                    "account_id": "broker-cop",
                    "currency": "COP",
                    "amount_minor": "250000"
                }],
                "closing_positions": [{
                    "account_id": "broker-cop",
                    "instrument_id": "fund-cop",
                    "instrument_type": "security",
                    "quantity": "1.250000000000000001",
                    "historical_cost_minor": "250000",
                    "cost_currency": "COP",
                    "observed_value_minor": "275000",
                    "valuation_currency": "COP",
                    "effective_date": "2026-02-27",
                    "observed_at": "2026-02-27T12:00:00Z",
                    "age_days": 1,
                    "freshness": "fresh",
                    "reconciliation_status": "matched",
                    "quantity_difference": "0",
                    "value_difference_minor": "0"
                }]
            })
        );
        assert!(response.alerts.is_empty());
    }

    #[test]
    fn investment_alerts_preserve_stale_unavailable_and_reconciliation_states() {
        let (_directory, mut connection) = fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/reconciliation.sql"
        ));
        let difference = finance(
            &mut connection,
            FinanceFilterRequest {
                start_date: "2026-01-01".into(),
                end_date: "2026-02-28".into(),
                currency: Some("COP".into()),
                ..FinanceFilterRequest::default()
            },
        )
        .unwrap();
        let difference = serde_json::to_value(difference).unwrap();
        assert_eq!(
            difference["alerts"][0]["id"],
            "reconciliation-difference:broker-cop:fund-cop:COP"
        );
        assert_eq!(difference["alerts"][0]["kind"], "reconciliation_difference");
        assert_eq!(difference["alerts"][0]["quantity_difference"], "0.25");
        assert_eq!(difference["alerts"][0]["value_difference_minor"], "50000");

        let stale = finance(
            &mut connection,
            FinanceFilterRequest {
                start_date: "2026-01-01".into(),
                end_date: "2026-04-30".into(),
                currency: Some("COP".into()),
                ..FinanceFilterRequest::default()
            },
        )
        .unwrap();
        let stale = serde_json::to_value(stale).unwrap();
        assert_eq!(
            stale["investments"]["closing_positions"][0]["freshness"],
            "stale"
        );
        assert_eq!(
            stale["investments"]["closing_positions"][0]["reconciliation_status"],
            "difference"
        );
        assert!(stale["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alert| { alert["id"] == "stale-valuation:broker-cop:fund-cop" }));
        assert!(stale["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alert| alert["kind"] == "reconciliation_difference"));

        let (_directory, mut stale_connection) = fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/investment.sql"
        ));
        let stale_oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/dashboard/oracles/stale.json"
        ))
        .unwrap();
        let stale_position = serde_json::to_value(
            finance(
                &mut stale_connection,
                FinanceFilterRequest {
                    start_date: "2026-01-01".into(),
                    end_date: "2026-04-30".into(),
                    currency: Some("COP".into()),
                    ..FinanceFilterRequest::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(stale_position["investments"]["state"], "stale");
        assert_eq!(
            stale_position["investments"]["closing_positions"][0]["observed_value_minor"],
            stale_oracle["expected"]["position"]["observed_value_minor"]
        );
        assert!(stale_position["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alert| {
                alert["id"] == stale_oracle["expected"]["alerts"][0]["id"]
                    && alert["kind"] == stale_oracle["expected"]["alerts"][0]["kind"]
            }));

        stale_connection
            .execute_batch(
                "DELETE FROM investment_snapshot_positions;
                 DELETE FROM investment_snapshots;",
            )
            .unwrap();
        let missing = serde_json::to_value(
            finance(
                &mut stale_connection,
                FinanceFilterRequest {
                    start_date: "2026-01-01".into(),
                    end_date: "2026-02-28".into(),
                    currency: Some("COP".into()),
                    ..FinanceFilterRequest::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(missing["investments"]["state"], "unavailable");
        assert_eq!(
            missing["investments"]["closing_positions"][0]["observed_value_minor"],
            Value::Null
        );
        assert!(missing["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alert| { alert["kind"] == "missing_valuation" }));

        let (_directory, mut unavailable) = fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/unavailable.sql"
        ));
        unavailable
            .execute_batch(
                "INSERT INTO source_documents(id,input_name,content_sha256,mime_type,byte_size,institution_hint) VALUES
                   ('pending-doc','synthetic.pdf','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','application/pdf',1,'wenia');
                 INSERT INTO import_batches(id,source_document_id,started_at,status,candidate_count,error_count,duplicate_count,error_details_json) VALUES
                   ('pending-batch','pending-doc','2026-02-01','completed',1,0,0,'[]');
                 INSERT INTO investment_document_events(id,source_document_id,import_batch_id,provider,parser_id,parser_version,event_type,provider_effective_date,currency,amount_minor,page_number,row_index,evidence_redaction,fingerprint,status) VALUES
                   ('pending-event','pending-doc','pending-batch','wenia','synthetic','1','deposit','2026-02-02','COP',50000,1,1,'REDACTED','pending-event-fingerprint','pending_review');",
            )
            .unwrap();
        let unavailable = finance(
            &mut unavailable,
            FinanceFilterRequest {
                start_date: "2026-01-01".into(),
                end_date: "2026-02-28".into(),
                currency: Some("COP".into()),
                ..FinanceFilterRequest::default()
            },
        )
        .unwrap();
        let unavailable = serde_json::to_value(unavailable).unwrap();
        let unavailable_oracle: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/dashboard/oracles/unavailable.json"
        ))
        .unwrap();
        assert_eq!(
            unavailable["investments"]["state"],
            unavailable_oracle["expected"]["state"]
        );
        assert_eq!(
            unavailable["investments"]["pending_allocation_minor"],
            unavailable_oracle["expected"]["pending_allocation_minor"]
        );
        assert_eq!(
            unavailable["alerts"][0]["id"],
            "pending-allocation:pending-capital"
        );
        assert_eq!(unavailable["alerts"][0]["kind"], "pending_allocation");
        assert_eq!(unavailable["alerts"][0]["pending_amount_minor"], "50000");
        assert!(unavailable["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alert| {
                alert["id"] == "pending-provider-event:pending-event"
                    && alert["kind"] == "pending_provider_event"
            }));
    }

    #[test]
    fn investment_filters_preserve_unlike_currencies_and_checked_decimal_quantities() {
        let (_directory, mut connection) = fixture_with_seed(include_str!(
            "../tests/fixtures/dashboard/seeds/investment.sql"
        ));
        connection
            .execute_batch(
                "INSERT INTO accounts(id, institution_id, label, currency, kind, is_owned) VALUES
                   ('broker-usd', 'synthetic-broker', 'USD Brokerage', 'USD', 'investment', 1);
                 INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind) VALUES
                   ('capital-usd', 'broker-usd', '2026-01-13', 'Synthetic USD capital', -500, 'USD', 'investment_contribution');
                 INSERT INTO investment_instruments(id, name, instrument_type, denomination_currency, provider, provider_identifier) VALUES
                   ('fund-usd', 'Synthetic USD Fund', 'security', 'USD', 'synthetic-broker', 'FUND2');
                 INSERT INTO investment_allocation_revisions(id, allocation_id, revision, contribution_transaction_id, instrument_id, cash_amount_minor, cash_currency, acquired_quantity, effective_date, provenance_source) VALUES
                   ('allocation-usd-r1', 'allocation-usd', 1, 'capital-usd', 'fund-usd', 500, 'USD', '99999999999999999999.123456789012345678', '2026-01-13', 'manual_entry');
                 INSERT INTO investment_allocation_heads(allocation_id, current_revision_id) VALUES
                   ('allocation-usd', 'allocation-usd-r1');
                 INSERT INTO investment_snapshot_positions(snapshot_id, account_id, instrument_id, quantity, currency, observed_value_minor, valuation_currency, observed_price) VALUES
                   ('snapshot-feb', 'broker-usd', 'fund-usd', '99999999999999999999.123456789012345678', 'USD', 700, 'EUR', '0.000000000000000007');",
            )
            .unwrap();

        let cop = finance(
            &mut connection,
            FinanceFilterRequest {
                start_date: "2026-01-01".into(),
                end_date: "2026-02-28".into(),
                currency: Some("COP".into()),
                ..FinanceFilterRequest::default()
            },
        )
        .unwrap();
        assert_eq!(cop.investments.closing_positions.len(), 1);
        assert_eq!(
            cop.investments.closing_positions[0]
                .cost_currency
                .as_deref(),
            Some("COP")
        );

        let usd = finance(
            &mut connection,
            FinanceFilterRequest {
                start_date: "2026-01-01".into(),
                end_date: "2026-02-28".into(),
                currency: Some("USD".into()),
                account_ids: Some(vec!["broker-usd".into()]),
                ..FinanceFilterRequest::default()
            },
        )
        .unwrap();
        assert_eq!(usd.investments.closing_positions.len(), 1);
        let position = &usd.investments.closing_positions[0];
        assert_eq!(
            position.quantity.as_deref(),
            Some("99999999999999999999.123456789012345678")
        );
        assert_eq!(position.cost_currency.as_deref(), Some("USD"));
        assert_eq!(position.valuation_currency.as_deref(), Some("EUR"));
        assert_eq!(position.observed_value_minor.as_deref(), Some("700"));
        assert!(usd
            .investments
            .flows
            .iter()
            .all(|flow| flow.currency == "USD" && flow.account_id == "broker-usd"));
    }
}
