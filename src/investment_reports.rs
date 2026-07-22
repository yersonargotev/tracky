use crate::reconciliation;
use crate::storage::{ReportDateRange, ReviewError};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const SCHEMA_VERSION: &str = "tracky.investment-report.v1";

#[derive(Debug, Default)]
struct ReportFilters {
    currency: Option<String>,
    account_ids: BTreeSet<String>,
}

impl ReportFilters {
    fn new(selected_currency: Option<&str>, selected_account_ids: &[String]) -> Self {
        Self {
            currency: selected_currency.map(|currency| currency.trim().to_ascii_uppercase()),
            account_ids: selected_account_ids.iter().cloned().collect(),
        }
    }

    fn includes(&self, account_id: &str, currency: &str) -> bool {
        self.currency
            .as_deref()
            .is_none_or(|selected| selected == currency.to_ascii_uppercase())
            && (self.account_ids.is_empty() || self.account_ids.contains(account_id))
    }

    fn includes_currency(&self, currency: &str) -> bool {
        self.currency
            .as_deref()
            .is_none_or(|selected| selected == currency.to_ascii_uppercase())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MoneyTotal {
    pub currency: String,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct CapitalSection {
    pub external_capital_contributed: Vec<MoneyTotal>,
    pub capital_withdrawn: Vec<MoneyTotal>,
    pub net_external_contribution: Vec<MoneyTotal>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct AcquisitionSection {
    pub gross_acquisitions: Vec<MoneyTotal>,
    pub funded_by_external_contribution: Vec<MoneyTotal>,
    pub funded_by_existing_cash: Vec<MoneyTotal>,
    pub funded_by_sales: Vec<MoneyTotal>,
    pub funded_by_maturities: Vec<MoneyTotal>,
    pub reinvestment: Vec<MoneyTotal>,
    pub by_instrument: Vec<InstrumentAcquisition>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstrumentAcquisition {
    pub account_id: String,
    pub instrument_id: String,
    pub instrument_type: String,
    pub currency: String,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct ReturnSection {
    pub principal_returned: Vec<MoneyTotal>,
    pub gross_interest: Vec<MoneyTotal>,
    pub gross_dividends: Vec<MoneyTotal>,
    pub realized_results: Vec<MoneyTotal>,
    pub net_cash: Vec<MoneyTotal>,
    pub by_instrument: Vec<InstrumentReturnDetail>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstrumentReturnDetail {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub principal_returned_minor: i64,
    pub gross_interest_minor: i64,
    pub gross_dividends_minor: i64,
    pub realized_result_minor: i64,
    pub net_cash_minor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct CostSection {
    pub fees_and_commissions: Vec<MoneyTotal>,
    pub withholding: Vec<MoneyTotal>,
    pub other_deductions: Vec<MoneyTotal>,
    pub by_instrument: Vec<InstrumentCostDetail>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstrumentCostDetail {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub fees_and_commissions_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClosingPosition {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub instrument_type: Option<String>,
    pub quantity: Option<String>,
    pub historical_cost_minor: Option<i64>,
    pub cost_currency: Option<String>,
    pub observed_value_minor: Option<i64>,
    pub valuation_currency: Option<String>,
    pub effective_date: Option<String>,
    pub observed_at: Option<String>,
    pub freshness: String,
    pub reconciliation_status: String,
    pub original_status: Option<String>,
    pub original_quantity_difference: Option<String>,
    pub current_quantity_difference: Option<String>,
    pub original_value_difference_minor: Option<i64>,
    pub current_value_difference_minor: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PendingAllocation {
    pub contribution_id: String,
    pub currency: String,
    pub contributed_minor: i64,
    pub allocated_minor: i64,
    pub unallocated_minor: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PendingProviderEvent {
    pub event_id: String,
    pub provider: String,
    pub effective_date: String,
    pub currency: String,
    pub event_type: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct PendingSection {
    pub allocations: Vec<PendingAllocation>,
    pub undated_acquisition_ids: Vec<String>,
    pub unattributed_withdrawals: Vec<MoneyTotal>,
    pub provider_events: Vec<PendingProviderEvent>,
    pub missing_snapshot_positions: Vec<String>,
    pub unreconciled_differences: Vec<String>,
    pub missing_valuations: Vec<String>,
    pub stale_valuations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InvestmentReportResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    pub date_range: ReportDateRange,
    pub capital_external: CapitalSection,
    pub acquisitions_and_reinvestment: AcquisitionSection,
    pub returns_and_income: ReturnSection,
    pub costs_and_withholding: CostSection,
    pub closing_positions: Vec<ClosingPosition>,
    pub pending_and_reconciliation: PendingSection,
    pub cdt_provider_enrichments: Vec<CdtProviderEnrichmentReport>,
    pub errors: Vec<ReviewError>,
}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CdtProviderEnrichmentReport {
    pub event_id: String,
    pub operation_revision_id: String,
    pub effective_date: String,
    pub provider_evidence: serde_json::Value,
    pub reviewer_terms: serde_json::Value,
}

#[derive(Default)]
struct FundingPools {
    deposited_external: i64,
    sale_external: i64,
    nonexternal_principal: i64,
    unattributed_principal: i64,
    realized_income: i64,
    investment_income: i64,
}

#[derive(Default)]
struct PositionOrigins {
    quantity: i128,
    external: i64,
    existing: i64,
    reinvested: i64,
    unattributed: i64,
}

fn allocated_origin(total: i64, sold: i128, held: i128, final_sale: bool) -> Result<i64> {
    if final_sale {
        return Ok(total);
    }
    i64::try_from(
        i128::from(total)
            .checked_mul(sold)
            .ok_or_else(|| anyhow::anyhow!("origin allocation overflow"))?
            / held,
    )
    .map_err(|_| anyhow::anyhow!("origin allocation overflow"))
}

fn cash_backed_origins(origins: [i64; 4], net_cash: i64) -> Result<[i64; 4]> {
    let cost = origins
        .iter()
        .try_fold(0_i64, |sum, value| sum.checked_add(*value))
        .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
    let cash = net_cash.max(0).min(cost);
    if cost == 0 || cash == cost {
        return Ok(origins);
    }
    let mut result = [0_i64; 4];
    let mut assigned = 0_i64;
    for index in 0..3 {
        result[index] = i64::try_from(
            i128::from(origins[index])
                .checked_mul(i128::from(cash))
                .ok_or_else(|| anyhow::anyhow!("origin overflow"))?
                / i128::from(cost),
        )?;
        assigned = assigned
            .checked_add(result[index])
            .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
    }
    result[3] = cash
        .checked_sub(assigned)
        .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
    Ok(result)
}

impl FundingPools {
    fn add(slot: &mut i64, amount: i64) -> Result<()> {
        *slot = slot
            .checked_add(amount)
            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
        Ok(())
    }
    fn consume(slot: &mut i64, wanted: &mut i64) -> i64 {
        let used = (*wanted).min((*slot).max(0));
        *slot = slot
            .checked_sub(used)
            .expect("used never exceeds source pool");
        *wanted = wanted
            .checked_sub(used)
            .expect("used never exceeds wanted amount");
        used
    }
}

fn valid_date(value: &str) -> bool {
    value.len() == 10 && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}
fn parse_report_quantity(value: &str) -> Result<i128> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 9
        || whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(anyhow::anyhow!("invalid persisted brokerage quantity"));
    }
    let whole = whole.parse::<i128>()?;
    let fraction = format!("{fraction:0<9}").parse::<i128>()?;
    whole
        .checked_mul(1_000_000_000)
        .and_then(|v| v.checked_add(fraction))
        .ok_or_else(|| anyhow::anyhow!("quantity overflow"))
}
fn add(map: &mut BTreeMap<String, i64>, currency: String, amount: i64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    *map.entry(currency).or_default() = map
        .get(&currency)
        .copied()
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
    Ok(())
}
fn totals(map: BTreeMap<String, i64>) -> Vec<MoneyTotal> {
    map.into_iter()
        .map(|(currency, amount_minor)| MoneyTotal {
            currency,
            amount_minor,
        })
        .collect()
}
fn add_instrument(
    map: &mut BTreeMap<(String, String, String, String), i64>,
    key: (String, String, String, String),
    amount: i64,
) -> Result<()> {
    let total = map
        .get(&key)
        .copied()
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
    map.insert(key, total);
    Ok(())
}
fn add_components<const N: usize>(
    map: &mut BTreeMap<(String, Option<String>, String), [i64; N]>,
    key: (String, Option<String>, String),
    values: [i64; N],
) -> Result<()> {
    let entry = map.entry(key).or_insert([0; N]);
    for (target, value) in entry.iter_mut().zip(values) {
        *target = target
            .checked_add(value)
            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
    }
    Ok(())
}
fn error(from: &str, to: &str, code: &'static str, path: &'static str) -> InvestmentReportResponse {
    InvestmentReportResponse {
        schema_version: SCHEMA_VERSION,
        command: "reports investments",
        ok: false,
        date_range: ReportDateRange {
            start_date: from.into(),
            end_date: to.into(),
        },
        capital_external: CapitalSection::default(),
        acquisitions_and_reinvestment: AcquisitionSection::default(),
        returns_and_income: ReturnSection::default(),
        costs_and_withholding: CostSection::default(),
        closing_positions: vec![],
        pending_and_reconciliation: PendingSection::default(),
        cdt_provider_enrichments: vec![],
        errors: vec![ReviewError {
            category: "validation_failure",
            code,
            message: code.replace('_', " "),
            path,
            recoverable: true,
            details: serde_json::json!({}),
        }],
    }
}

pub fn report_error(
    from: &str,
    to: &str,
    code: &'static str,
    path: &'static str,
) -> InvestmentReportResponse {
    error(from, to, code, path)
}

fn external_contributions(
    c: &Connection,
    from: &str,
    to: &str,
    filters: &ReportFilters,
) -> Result<BTreeMap<String, i64>> {
    let mut contributed = BTreeMap::new();
    let mut statement = c.prepare("SELECT account_id,currency,amount_minor FROM canonical_transactions WHERE transaction_kind='investment_contribution' AND posted_date BETWEEN ?1 AND ?2 ORDER BY posted_date,id")?;
    for row in statement.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (account_id, currency, signed_amount) = row?;
        if !filters.includes(&account_id, &currency) {
            continue;
        }
        let amount = signed_amount
            .checked_neg()
            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
        add(&mut contributed, currency, amount)?;
    }
    Ok(contributed)
}

struct DirectAcquisition {
    account_id: String,
    instrument_id: String,
    instrument_type: String,
    currency: String,
    amount_minor: i64,
}

fn direct_acquisitions(
    c: &Connection,
    from: &str,
    to: &str,
    filters: &ReportFilters,
) -> Result<Vec<DirectAcquisition>> {
    let mut statement=c.prepare("SELECT t.account_id,r.instrument_id,i.instrument_type,r.cash_currency,r.cash_amount_minor FROM investment_allocation_heads h JOIN investment_allocation_revisions r ON r.id=h.current_revision_id JOIN canonical_transactions t ON t.id=r.contribution_transaction_id JOIN investment_instruments i ON i.id=r.instrument_id LEFT JOIN investment_allocation_consumptions c ON c.allocation_id=h.allocation_id WHERE r.effective_date BETWEEN ?1 AND ?2 AND c.allocation_id IS NULL ORDER BY r.effective_date,h.allocation_id")?;
    let rows = statement
        .query_map(params![from, to], |r| {
            Ok(DirectAcquisition {
                account_id: r.get(0)?,
                instrument_id: r.get(1)?,
                instrument_type: r.get(2)?,
                currency: r.get(3)?,
                amount_minor: r.get(4)?,
            })
        })?
        .filter_map(|row| match row {
            Ok(acquisition) if filters.includes(&acquisition.account_id, &acquisition.currency) => {
                Some(Ok(acquisition))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn closing_state(
    c: &Connection,
    to: &str,
    unattributed_withdrawals: BTreeMap<String, i64>,
    filters: &ReportFilters,
) -> Result<(PendingSection, Vec<ClosingPosition>)> {
    let mut pending = PendingSection {
        unattributed_withdrawals: totals(unattributed_withdrawals),
        ..PendingSection::default()
    };
    let mut undated = c.prepare("SELECT h.allocation_id,t.account_id,r.cash_currency FROM investment_allocation_heads h JOIN investment_allocation_revisions r ON r.id=h.current_revision_id JOIN canonical_transactions t ON t.id=r.contribution_transaction_id WHERE r.effective_date IS NULL AND t.posted_date<=?1 ORDER BY h.allocation_id")?;
    pending.undated_acquisition_ids = undated
        .query_map([to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .filter_map(|row| match row {
            Ok((id, account_id, currency)) if filters.includes(&account_id, &currency) => {
                Some(Ok(id))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut stmt=c.prepare("SELECT t.posted_date,t.id,t.account_id,t.currency,t.amount_minor,r.cash_amount_minor FROM canonical_transactions t LEFT JOIN investment_allocation_revisions r ON r.contribution_transaction_id=t.id AND r.effective_date<=?1 AND EXISTS(SELECT 1 FROM investment_allocation_heads h WHERE h.current_revision_id=r.id) WHERE t.transaction_kind='investment_contribution' AND t.posted_date<=?1 ORDER BY t.posted_date,t.id,r.allocation_id")?;
    let mut allocation_totals = BTreeMap::new();
    for row in stmt.query_map([to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, Option<i64>>(5)?,
        ))
    })? {
        let (posted_date, id, account_id, currency, signed_total, allocation) = row?;
        if !filters.includes(&account_id, &currency) {
            continue;
        }
        let total = signed_total
            .checked_neg()
            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
        let entry = allocation_totals
            .entry((posted_date, id))
            .or_insert((currency, total, 0_i64));
        entry.2 = entry
            .2
            .checked_add(allocation.unwrap_or(0))
            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
    }
    pending.allocations = allocation_totals
        .into_iter()
        .filter_map(
            |((_posted_date, contribution_id), (currency, total, allocated))| {
                (allocated < total).then_some((contribution_id, currency, total, allocated))
            },
        )
        .map(|(contribution_id, currency, total, allocated)| {
            Ok(PendingAllocation {
                contribution_id,
                currency,
                contributed_minor: total,
                allocated_minor: allocated,
                unallocated_minor: total
                    .checked_sub(allocated)
                    .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?,
                status: if allocated == 0 {
                    "pending".into()
                } else {
                    "partial".into()
                },
            })
        })
        .collect::<Result<_>>()?;
    let mut stmt=c.prepare("SELECT id,provider,provider_effective_date,currency,event_type FROM investment_document_events WHERE status='pending_review' AND provider_effective_date<=?1 ORDER BY provider_effective_date,id")?;
    pending.provider_events = stmt
        .query_map([to], |r| {
            Ok(PendingProviderEvent {
                event_id: r.get(0)?,
                provider: r.get(1)?,
                effective_date: r.get(2)?,
                currency: r.get(3)?,
                event_type: r.get(4)?,
            })
        })?
        .filter_map(|row| match row {
            Ok(event) if filters.includes_currency(&event.currency) => Some(Ok(event)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<rusqlite::Result<_>>()?;
    let mut positions = Vec::new();
    let mut instrument_types = BTreeMap::new();
    let mut instrument_stmt = c.prepare("SELECT id,instrument_type FROM investment_instruments")?;
    for row in
        instrument_stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
    {
        let (id, instrument_type) = row?;
        instrument_types.insert(id, instrument_type);
    }
    let mut snapshot_ids=c.prepare("SELECT id FROM investment_snapshots WHERE COALESCE(provider_effective_date,substr(observed_at,1,10))<=?1 ORDER BY COALESCE(provider_effective_date,substr(observed_at,1,10)) DESC,observed_at DESC,id DESC")?;
    let snapshot_ids = snapshot_ids
        .query_map([to], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut position_map = BTreeMap::new();
    for id in snapshot_ids {
        let comparison = reconciliation::compare(c, &id, to)?;
        let s = comparison.snapshot.expect("comparison snapshot");
        let observed_keys = s
            .positions
            .iter()
            .map(|p| {
                (
                    p.account_id.clone(),
                    p.instrument_id.clone(),
                    p.currency.clone(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        for x in comparison.reconciliations {
            if !filters.includes(&x.account_id, &x.currency) {
                continue;
            }
            let tuple = (
                x.account_id.clone(),
                x.instrument_id.clone(),
                x.currency.clone(),
            );
            let account_in_snapshot = s.positions.iter().any(|p| p.account_id == x.account_id);
            if !(observed_keys.contains(&tuple)
                || account_in_snapshot && x.status == "missing_snapshot_position")
                || position_map.contains_key(&tuple)
            {
                continue;
            }
            let key = format!(
                "{}:{}:{}",
                x.account_id,
                x.instrument_id.as_deref().unwrap_or("cash"),
                x.currency
            );
            match x.status.as_str() {
                "missing_snapshot_position" => pending.missing_snapshot_positions.push(key.clone()),
                "valuation_unavailable" => pending.missing_valuations.push(key.clone()),
                "stale" => pending.stale_valuations.push(key.clone()),
                "matched" => {}
                _ => pending.unreconciled_differences.push(key.clone()),
            };
            position_map.insert(
                tuple,
                ClosingPosition {
                    account_id: x.account_id,
                    instrument_id: x.instrument_id.clone(),
                    instrument_type: x
                        .instrument_id
                        .as_ref()
                        .and_then(|id| instrument_types.get(id).cloned()),
                    quantity: x.derived_quantity.or(x.observed_quantity),
                    historical_cost_minor: x.derived_historical_cost_minor,
                    cost_currency: x.cost_currency,
                    observed_value_minor: x.observed_value_minor,
                    valuation_currency: x.valuation_currency,
                    effective_date: s
                        .provider_effective_date
                        .clone()
                        .or_else(|| Some(s.observed_at[..10].into())),
                    observed_at: Some(s.observed_at.clone()),
                    freshness: x.freshness,
                    reconciliation_status: x.status,
                    original_status: Some(x.original_status),
                    original_quantity_difference: x.original_quantity_difference,
                    current_quantity_difference: x.quantity_difference,
                    original_value_difference_minor: x.original_value_difference_minor,
                    current_value_difference_minor: x.value_difference_minor,
                },
            );
        }
    }
    for x in reconciliation::derived_closing_positions(c, to)? {
        if !filters.includes(&x.account_id, &x.currency) {
            continue;
        }
        let tuple = (
            x.account_id.clone(),
            x.instrument_id.clone(),
            x.currency.clone(),
        );
        if let std::collections::btree_map::Entry::Vacant(entry) = position_map.entry(tuple) {
            let key = format!(
                "{}:{}:{}",
                x.account_id,
                x.instrument_id.as_deref().unwrap_or("cash"),
                x.currency
            );
            pending.missing_valuations.push(key.clone());
            pending.missing_snapshot_positions.push(key);
            entry.insert(ClosingPosition {
                account_id: x.account_id,
                instrument_id: x.instrument_id.clone(),
                instrument_type: x
                    .instrument_id
                    .as_ref()
                    .and_then(|id| instrument_types.get(id).cloned()),
                quantity: x.quantity,
                historical_cost_minor: x.historical_cost_minor,
                cost_currency: x.cost_currency,
                observed_value_minor: None,
                valuation_currency: None,
                effective_date: None,
                observed_at: None,
                freshness: "unavailable".into(),
                reconciliation_status: "no_snapshot".into(),
                original_status: None,
                original_quantity_difference: None,
                current_quantity_difference: None,
                original_value_difference_minor: None,
                current_value_difference_minor: None,
            });
        }
    }
    positions.extend(position_map.into_values());
    Ok((pending, positions))
}

#[derive(Default)]
struct ReportAggregation {
    withdrawn: BTreeMap<String, i64>,
    principal: BTreeMap<String, i64>,
    interest: BTreeMap<String, i64>,
    dividends: BTreeMap<String, i64>,
    results: BTreeMap<String, i64>,
    net: BTreeMap<String, i64>,
    fees: BTreeMap<String, i64>,
    withholding: BTreeMap<String, i64>,
    deductions: BTreeMap<String, i64>,
    gross: BTreeMap<String, i64>,
    external: BTreeMap<String, i64>,
    existing: BTreeMap<String, i64>,
    sales: BTreeMap<String, i64>,
    maturities: BTreeMap<String, i64>,
    reinvest: BTreeMap<String, i64>,
    instrument_acquisitions: BTreeMap<(String, String, String, String), i64>,
    return_details: BTreeMap<(String, Option<String>, String), [i64; 5]>,
    cost_details: BTreeMap<(String, Option<String>, String), [i64; 3]>,
    unattributed_withdrawals: BTreeMap<String, i64>,
}

struct BrokerageReplayRow {
    account: String,
    kind: String,
    cur: String,
    g: i64,
    rr: i64,
    f: i64,
    w: i64,
    d: i64,
    n: i64,
    funding: Option<String>,
    instrument: Option<String>,
    instrument_type: Option<String>,
    date: String,
    quantity: Option<String>,
    origin_external: i64,
    origin_existing: i64,
    origin_reinvested: i64,
    origin_income: i64,
    origin_unattributed: i64,
}

fn brokerage_replay_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BrokerageReplayRow> {
    Ok(BrokerageReplayRow {
        account: r.get(0)?,
        kind: r.get(1)?,
        cur: r.get(2)?,
        g: r.get(3)?,
        rr: r.get(4)?,
        f: r.get(5)?,
        w: r.get(6)?,
        d: r.get(7)?,
        n: r.get(8)?,
        funding: r.get(9)?,
        instrument: r.get(10)?,
        instrument_type: r.get(11)?,
        date: r.get(12)?,
        quantity: r.get(13)?,
        origin_external: r.get(14)?,
        origin_existing: r.get(15)?,
        origin_reinvested: r.get(16)?,
        origin_income: r.get(17)?,
        origin_unattributed: r.get(18)?,
    })
}

fn replay_brokerage(
    c: &Connection,
    from: &str,
    to: &str,
    aggregation: &mut ReportAggregation,
    filters: &ReportFilters,
) -> Result<()> {
    let mut source_cash: HashMap<(String, String), FundingPools> = HashMap::new();
    let mut origins: HashMap<(String, String, String), PositionOrigins> = HashMap::new();
    let mut stmt=c.prepare("SELECT r.account_id,r.operation_type,r.currency,r.gross_amount_minor,r.realized_result_minor,r.fee_minor,r.withholding_minor,r.other_deductions_minor,r.net_cash_minor,r.funding_allocation_id,r.instrument_id,i.instrument_type,r.effective_date,r.quantity,COALESCE(a.external_capital_minor,0),COALESCE(a.existing_cash_minor,0),COALESCE(a.reinvested_minor,0),COALESCE(a.investment_income_minor,0),COALESCE(a.unattributed_minor,CASE WHEN r.operation_type='buy' THEN r.historical_cost_minor ELSE 0 END) FROM brokerage_operation_heads h JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id LEFT JOIN investment_instruments i ON i.id=r.instrument_id LEFT JOIN brokerage_buy_funding_attributions a ON a.operation_revision_id=r.id WHERE r.effective_date<=?1 ORDER BY r.effective_date,(SELECT MIN(rr.rowid) FROM brokerage_operation_revisions rr WHERE rr.operation_id=r.operation_id)")?;
    for row in stmt.query_map([to], brokerage_replay_row)? {
        let BrokerageReplayRow {
            account,
            kind,
            cur,
            g,
            rr,
            f,
            w,
            d,
            n,
            funding,
            instrument,
            instrument_type,
            date,
            quantity,
            origin_external,
            origin_existing,
            origin_reinvested,
            origin_income,
            origin_unattributed,
        } = row?;
        if !filters.includes(&account, &cur) {
            continue;
        }
        let in_range = date.as_str() >= from;
        let detail_instrument = instrument.clone();
        let pools = source_cash
            .entry((account.clone(), cur.clone()))
            .or_default();
        match kind.as_str() {
            "deposit" if funding.is_some() => FundingPools::add(&mut pools.deposited_external, n)?,
            "buy" => {
                let mut consume_external = origin_external;
                let direct_external =
                    FundingPools::consume(&mut pools.deposited_external, &mut consume_external);
                if consume_external != 0 {
                    return Err(anyhow::anyhow!("incompatible external funding provenance"));
                }
                let mut sale_remaining = origin_reinvested;
                let sale_external =
                    FundingPools::consume(&mut pools.sale_external, &mut sale_remaining);
                let sale_existing =
                    FundingPools::consume(&mut pools.nonexternal_principal, &mut sale_remaining);
                let sale_unattributed =
                    FundingPools::consume(&mut pools.unattributed_principal, &mut sale_remaining);
                if sale_remaining != 0 {
                    return Err(anyhow::anyhow!("incompatible sale funding provenance"));
                }
                let mut income_remaining = origin_income;
                FundingPools::consume(&mut pools.investment_income, &mut income_remaining);
                FundingPools::consume(&mut pools.realized_income, &mut income_remaining);
                if income_remaining != 0 {
                    return Err(anyhow::anyhow!("incompatible income funding provenance"));
                }
                if in_range {
                    add(&mut aggregation.gross, cur.clone(), g)?;
                }
                if in_range {
                    if let (Some(instrument), Some(kind)) = (instrument.clone(), instrument_type) {
                        add_instrument(
                            &mut aggregation.instrument_acquisitions,
                            (account.clone(), instrument, kind, cur.clone()),
                            g,
                        )?;
                    }
                }
                if in_range {
                    add(&mut aggregation.external, cur.clone(), origin_external)?;
                    add(&mut aggregation.existing, cur.clone(), origin_existing)?;
                    add(&mut aggregation.sales, cur.clone(), origin_reinvested)?;
                    add(
                        &mut aggregation.reinvest,
                        cur.clone(),
                        origin_reinvested
                            .checked_add(origin_income)
                            .ok_or_else(|| anyhow::anyhow!("origin overflow"))?,
                    )?;
                }
                if let (Some(instrument), Some(quantity)) = (instrument, quantity) {
                    let q = parse_report_quantity(&quantity)?;
                    let position = origins
                        .entry((account.clone(), instrument, cur.clone()))
                        .or_default();
                    position.quantity = position
                        .quantity
                        .checked_add(q)
                        .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
                    FundingPools::add(
                        &mut position.external,
                        direct_external
                            .checked_add(sale_external)
                            .ok_or_else(|| anyhow::anyhow!("origin overflow"))?,
                    )?;
                    FundingPools::add(
                        &mut position.existing,
                        origin_existing
                            .checked_add(sale_existing)
                            .ok_or_else(|| anyhow::anyhow!("origin overflow"))?,
                    )?;
                    FundingPools::add(&mut position.reinvested, origin_income)?;
                    FundingPools::add(
                        &mut position.unattributed,
                        origin_unattributed
                            .checked_add(sale_unattributed)
                            .ok_or_else(|| anyhow::anyhow!("origin overflow"))?,
                    )?;
                }
            }
            "sell" => {
                if in_range {
                    add(
                        &mut aggregation.principal,
                        cur.clone(),
                        g.checked_sub(rr)
                            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?,
                    )?;
                    add(&mut aggregation.results, cur.clone(), rr)?;
                }
                let sold_principal = g
                    .checked_sub(rr)
                    .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
                let instrument = instrument.unwrap_or_else(|| "__unattributed__".into());
                let sold = parse_report_quantity(
                    quantity
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("sell quantity missing"))?,
                )?;
                let position = origins
                    .entry((account.clone(), instrument, cur.clone()))
                    .or_insert_with(|| PositionOrigins {
                        quantity: sold,
                        unattributed: sold_principal,
                        ..PositionOrigins::default()
                    });
                let final_sale = sold == position.quantity;
                let ext = allocated_origin(position.external, sold, position.quantity, final_sale)?;
                let existing_origin =
                    allocated_origin(position.existing, sold, position.quantity, final_sale)?;
                let reinvested_origin =
                    allocated_origin(position.reinvested, sold, position.quantity, final_sale)?;
                let unattributed_origin =
                    allocated_origin(position.unattributed, sold, position.quantity, final_sale)?;
                position.quantity = position
                    .quantity
                    .checked_sub(sold)
                    .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
                position.external = position
                    .external
                    .checked_sub(ext)
                    .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
                position.existing = position
                    .existing
                    .checked_sub(existing_origin)
                    .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
                position.reinvested = position
                    .reinvested
                    .checked_sub(reinvested_origin)
                    .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
                position.unattributed = position
                    .unattributed
                    .checked_sub(unattributed_origin)
                    .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
                let [ext_cash, existing_cash, reinvested_cash, unattributed_cash] =
                    cash_backed_origins(
                        [ext, existing_origin, reinvested_origin, unattributed_origin],
                        n,
                    )?;
                FundingPools::add(&mut pools.sale_external, ext_cash)?;
                FundingPools::add(
                    &mut pools.nonexternal_principal,
                    existing_cash
                        .checked_add(reinvested_cash)
                        .ok_or_else(|| anyhow::anyhow!("origin overflow"))?,
                )?;
                FundingPools::add(&mut pools.unattributed_principal, unattributed_cash)?;
                let returned_cost = ext_cash
                    .checked_add(existing_cash)
                    .and_then(|v| v.checked_add(reinvested_cash))
                    .and_then(|v| v.checked_add(unattributed_cash))
                    .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
                FundingPools::add(
                    &mut pools.realized_income,
                    n.checked_sub(returned_cost)
                        .ok_or_else(|| anyhow::anyhow!("incompatible sale provenance"))?
                        .max(0),
                )?;
            }
            "dividend" => {
                if in_range {
                    add(&mut aggregation.dividends, cur.clone(), g)?;
                }
                FundingPools::add(&mut pools.investment_income, n)?;
            }
            "withdrawal" => {
                let mut remaining = g;
                FundingPools::consume(&mut pools.investment_income, &mut remaining);
                FundingPools::consume(&mut pools.realized_income, &mut remaining);
                FundingPools::consume(&mut pools.nonexternal_principal, &mut remaining);
                let unattributed =
                    FundingPools::consume(&mut pools.unattributed_principal, &mut remaining);
                let returned_from_sale =
                    FundingPools::consume(&mut pools.sale_external, &mut remaining);
                let returned_from_deposit =
                    FundingPools::consume(&mut pools.deposited_external, &mut remaining);
                let returned_external = returned_from_sale
                    .checked_add(returned_from_deposit)
                    .ok_or_else(|| anyhow::anyhow!("origin overflow"))?;
                if in_range {
                    add(&mut aggregation.withdrawn, cur.clone(), returned_external)?;
                    add(
                        &mut aggregation.unattributed_withdrawals,
                        cur.clone(),
                        unattributed
                            .checked_add(remaining)
                            .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?,
                    )?;
                }
            }
            _ => {}
        }
        if in_range {
            let returned = if kind == "sell" {
                g.checked_sub(rr)
                    .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?
            } else {
                0
            };
            add_components(
                &mut aggregation.return_details,
                (account.clone(), detail_instrument.clone(), cur.clone()),
                [
                    returned,
                    0,
                    if kind == "dividend" { g } else { 0 },
                    if kind == "sell" { rr } else { 0 },
                    n,
                ],
            )?;
            add_components(
                &mut aggregation.cost_details,
                (account.clone(), detail_instrument, cur.clone()),
                [f, w, d],
            )?;
            add(&mut aggregation.fees, cur.clone(), f)?;
            add(&mut aggregation.withholding, cur.clone(), w)?;
            add(&mut aggregation.deductions, cur.clone(), d)?;
            add(&mut aggregation.net, cur, n)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_brokerage_attribution(c: &Connection, through: &str) -> Result<()> {
    replay_brokerage(
        c,
        "0001-01-01",
        through,
        &mut ReportAggregation::default(),
        &ReportFilters::default(),
    )
}

struct CdtReplayRow {
    kind: String,
    cur: String,
    p: i64,
    e: i64,
    ci: i64,
    gi: i64,
    w: i64,
    d: i64,
    n: i64,
    before: i64,
    after: i64,
    account: String,
    instrument: String,
}

fn cdt_replay_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CdtReplayRow> {
    Ok(CdtReplayRow {
        kind: r.get(0)?,
        cur: r.get(1)?,
        p: r.get(2)?,
        e: r.get(3)?,
        ci: r.get(4)?,
        gi: r.get(5)?,
        w: r.get(6)?,
        d: r.get(7)?,
        n: r.get(8)?,
        before: r.get(9)?,
        after: r.get(10)?,
        account: r.get(11)?,
        instrument: r.get(12)?,
    })
}

fn replay_cdt(
    c: &Connection,
    from: &str,
    to: &str,
    aggregation: &mut ReportAggregation,
    filters: &ReportFilters,
) -> Result<()> {
    let mut stmt=c.prepare("SELECT r.operation_type,r.currency,r.principal_returned_minor,r.external_capital_minor,r.capitalized_interest_minor,r.gross_interest_minor,r.withholding_minor,r.other_deductions_minor,r.net_cash_received_minor,r.principal_before_minor,r.principal_after_minor,p.account_id,p.instrument_id FROM cdt_operation_heads h JOIN cdt_operation_revisions r ON r.id=h.current_revision_id JOIN cdt_positions p ON p.id=r.cdt_position_id WHERE r.effective_date BETWEEN ?1 AND ?2 ORDER BY r.effective_date,r.operation_id")?;
    for row in stmt.query_map(params![from, to], cdt_replay_row)? {
        let CdtReplayRow {
            kind,
            cur,
            p,
            e,
            ci,
            gi,
            w,
            d,
            n,
            before,
            after,
            account,
            instrument,
        } = row?;
        if !filters.includes(&account, &cur) {
            continue;
        }
        add_components(
            &mut aggregation.return_details,
            (account.clone(), Some(instrument.clone()), cur.clone()),
            [p, gi, 0, 0, n],
        )?;
        add_components(
            &mut aggregation.cost_details,
            (account.clone(), Some(instrument.clone()), cur.clone()),
            [0, w, d],
        )?;
        if matches!(kind.as_str(), "constitution" | "renewal") {
            add(&mut aggregation.gross, cur.clone(), after)?;
            add_instrument(
                &mut aggregation.instrument_acquisitions,
                (account, instrument, "fixed_income".into(), cur.clone()),
                after,
            )?;
        }
        if kind == "renewal" {
            let reinvestable = after
                .checked_sub(e)
                .and_then(|v| v.checked_sub(ci))
                .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
            let matured = before.min(reinvestable);
            add(&mut aggregation.maturities, cur.clone(), matured)?;
            add(&mut aggregation.reinvest, cur.clone(), matured)?;
        }
        if e > 0 {
            add(&mut aggregation.external, cur.clone(), e)?
        }
        if ci > 0 {
            add(&mut aggregation.reinvest, cur.clone(), ci)?
        }
        add(&mut aggregation.principal, cur.clone(), p)?;
        add(&mut aggregation.interest, cur.clone(), gi)?;
        add(&mut aggregation.withholding, cur.clone(), w)?;
        add(&mut aggregation.deductions, cur.clone(), d)?;
        add(&mut aggregation.net, cur, n)?;
    }
    Ok(())
}

pub fn report(c: &Connection, from: &str, to: &str) -> Result<InvestmentReportResponse> {
    report_filtered(c, from, to, None, &[])
}

pub(crate) fn report_filtered(
    c: &Connection,
    from: &str,
    to: &str,
    selected_currency: Option<&str>,
    selected_account_ids: &[String],
) -> Result<InvestmentReportResponse> {
    if !valid_date(from) {
        return Ok(error(from, to, "invalid_from_date", "from"));
    }
    if !valid_date(to) {
        return Ok(error(from, to, "invalid_to_date", "to"));
    }
    if from > to {
        return Ok(error(from, to, "invalid_date_range", "date_range"));
    }
    let filters = ReportFilters::new(selected_currency, selected_account_ids);
    let contributed = external_contributions(c, from, to, &filters)?;
    let mut aggregation = ReportAggregation::default();
    for acquisition in direct_acquisitions(c, from, to, &filters)? {
        add(
            &mut aggregation.gross,
            acquisition.currency.clone(),
            acquisition.amount_minor,
        )?;
        add(
            &mut aggregation.external,
            acquisition.currency.clone(),
            acquisition.amount_minor,
        )?;
        add_instrument(
            &mut aggregation.instrument_acquisitions,
            (
                acquisition.account_id,
                acquisition.instrument_id,
                acquisition.instrument_type,
                acquisition.currency,
            ),
            acquisition.amount_minor,
        )?;
    }
    replay_brokerage(c, from, to, &mut aggregation, &filters)?;
    replay_cdt(c, from, to, &mut aggregation, &filters)?;
    let net_external = contributed
        .iter()
        .chain(aggregation.withdrawn.iter())
        .map(|(k, _)| k.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|k| {
            contributed
                .get(&k)
                .copied()
                .unwrap_or(0)
                .checked_sub(aggregation.withdrawn.get(&k).copied().unwrap_or(0))
                .map(|v| (k, v))
                .ok_or_else(|| anyhow::anyhow!("report amount overflow"))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let (pending, positions) =
        closing_state(c, to, aggregation.unattributed_withdrawals, &filters)?;
    let mut enrichment_statement=c.prepare("SELECT x.event_id,x.operation_revision_id,r.effective_date,x.provider_evidence_json,x.reviewer_terms_json,p.account_id,r.currency FROM cdt_provider_enrichments x JOIN cdt_operation_revisions r ON r.id=x.operation_revision_id JOIN cdt_positions p ON p.id=r.cdt_position_id WHERE r.effective_date BETWEEN ?1 AND ?2 ORDER BY r.effective_date,x.event_id")?;
    let cdt_provider_enrichments = enrichment_statement
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .map(|row| {
            let (
                event_id,
                operation_revision_id,
                effective_date,
                evidence,
                terms,
                account,
                currency,
            ) = row?;
            Ok((
                account,
                currency,
                CdtProviderEnrichmentReport {
                    event_id,
                    operation_revision_id,
                    effective_date,
                    provider_evidence: serde_json::from_str(&evidence)?,
                    reviewer_terms: serde_json::from_str(&terms)?,
                },
            ))
        })
        .filter_map(|row| match row {
            Ok((account, currency, enrichment)) if filters.includes(&account, &currency) => {
                Some(Ok(enrichment))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(InvestmentReportResponse {
        schema_version: SCHEMA_VERSION,
        command: "reports investments",
        ok: true,
        date_range: ReportDateRange {
            start_date: from.into(),
            end_date: to.into(),
        },
        capital_external: CapitalSection {
            external_capital_contributed: totals(contributed),
            capital_withdrawn: totals(aggregation.withdrawn),
            net_external_contribution: totals(net_external),
        },
        acquisitions_and_reinvestment: AcquisitionSection {
            gross_acquisitions: totals(aggregation.gross),
            funded_by_external_contribution: totals(aggregation.external),
            funded_by_existing_cash: totals(aggregation.existing),
            funded_by_sales: totals(aggregation.sales),
            funded_by_maturities: totals(aggregation.maturities),
            reinvestment: totals(aggregation.reinvest),
            by_instrument: aggregation
                .instrument_acquisitions
                .into_iter()
                .map(
                    |((account_id, instrument_id, instrument_type, currency), amount_minor)| {
                        InstrumentAcquisition {
                            account_id,
                            instrument_id,
                            instrument_type,
                            currency,
                            amount_minor,
                        }
                    },
                )
                .collect(),
        },
        returns_and_income: ReturnSection {
            principal_returned: totals(aggregation.principal),
            gross_interest: totals(aggregation.interest),
            gross_dividends: totals(aggregation.dividends),
            realized_results: totals(aggregation.results),
            net_cash: totals(aggregation.net),
            by_instrument: aggregation
                .return_details
                .into_iter()
                .map(
                    |((account_id, instrument_id, currency), value)| InstrumentReturnDetail {
                        account_id,
                        instrument_id,
                        currency,
                        principal_returned_minor: value[0],
                        gross_interest_minor: value[1],
                        gross_dividends_minor: value[2],
                        realized_result_minor: value[3],
                        net_cash_minor: value[4],
                    },
                )
                .collect(),
        },
        costs_and_withholding: CostSection {
            fees_and_commissions: totals(aggregation.fees),
            withholding: totals(aggregation.withholding),
            other_deductions: totals(aggregation.deductions),
            by_instrument: aggregation
                .cost_details
                .into_iter()
                .map(
                    |((account_id, instrument_id, currency), value)| InstrumentCostDetail {
                        account_id,
                        instrument_id,
                        currency,
                        fees_and_commissions_minor: value[0],
                        withholding_minor: value[1],
                        other_deductions_minor: value[2],
                    },
                )
                .collect(),
        },
        closing_positions: positions,
        pending_and_reconciliation: pending,
        cdt_provider_enrichments,
        errors: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::apply_migrations;

    fn filtered_report_fixture() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        apply_migrations(&connection).unwrap();
        connection
            .execute_batch(
                "INSERT INTO institutions(id,name) VALUES('institution','Institution');
                 INSERT INTO accounts(id,institution_id,label,kind,currency,is_owned) VALUES
                   ('bank','institution','Bank','checking','COP',1),
                   ('broker','institution','Broker','brokerage','COP',1),
                   ('excluded','institution','Excluded','checking','COP',1);
                 INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind) VALUES
                   ('bank-contribution','bank','2026-06-01','Capital',-100,'COP','investment_contribution'),
                   ('bank-usd','bank','2026-06-01','USD capital',-200,'USD','investment_contribution');
                 INSERT INTO brokerage_accounts(account_id,opened_date,provenance_source) VALUES
                   ('broker','2026-01-01','manual_entry');
                 INSERT INTO brokerage_operation_revisions(id,operation_id,revision,account_id,operation_type,effective_date,currency,gross_amount_minor,net_cash_minor,provenance_source) VALUES
                   ('dividend-revision','dividend',1,'broker','dividend','2026-06-10','COP',30,30,'manual_entry');
                 INSERT INTO brokerage_operation_heads(operation_id,current_revision_id) VALUES
                   ('dividend','dividend-revision');",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO canonical_transactions(id,account_id,posted_date,description,amount_minor,currency,transaction_kind) VALUES('excluded-overflow','excluded','2026-06-01','Excluded',?1,'COP','investment_contribution')",
                [i64::MIN],
            )
            .unwrap();
        connection
    }

    #[test]
    fn filters_source_contributions_and_investment_operations_before_aggregation() {
        let connection = filtered_report_fixture();

        let bank = report_filtered(
            &connection,
            "2026-06-01",
            "2026-06-30",
            Some("cop"),
            &["bank".into()],
        )
        .unwrap();
        assert_eq!(
            bank.capital_external.external_capital_contributed,
            vec![MoneyTotal {
                currency: "COP".into(),
                amount_minor: 100,
            }]
        );
        assert!(bank.returns_and_income.gross_dividends.is_empty());
        assert_eq!(bank.pending_and_reconciliation.allocations.len(), 1);
        assert_eq!(
            bank.pending_and_reconciliation.allocations[0].contribution_id,
            "bank-contribution"
        );

        let broker = report_filtered(
            &connection,
            "2026-06-01",
            "2026-06-30",
            Some("COP"),
            &["broker".into()],
        )
        .unwrap();
        assert!(broker
            .capital_external
            .external_capital_contributed
            .is_empty());
        assert_eq!(
            broker.returns_and_income.gross_dividends,
            vec![MoneyTotal {
                currency: "COP".into(),
                amount_minor: 30,
            }]
        );
        assert_eq!(
            broker.returns_and_income.by_instrument[0].account_id,
            "broker"
        );
        assert!(broker.pending_and_reconciliation.allocations.is_empty());
    }
}
