use crate::reconciliation;
use crate::storage::{ReportDateRange, ReviewError};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub const SCHEMA_VERSION: &str = "tracky.investment-report.v1";

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
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct ReturnSection {
    pub principal_returned: Vec<MoneyTotal>,
    pub gross_interest: Vec<MoneyTotal>,
    pub gross_dividends: Vec<MoneyTotal>,
    pub realized_results: Vec<MoneyTotal>,
    pub net_cash: Vec<MoneyTotal>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct CostSection {
    pub fees_and_commissions: Vec<MoneyTotal>,
    pub withholding: Vec<MoneyTotal>,
    pub other_deductions: Vec<MoneyTotal>,
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
    pub errors: Vec<ReviewError>,
}

fn valid_date(value: &str) -> bool {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
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

pub fn report(c: &Connection, from: &str, to: &str) -> Result<InvestmentReportResponse> {
    if !valid_date(from) {
        return Ok(error(from, to, "invalid_from_date", "from"));
    }
    if !valid_date(to) {
        return Ok(error(from, to, "invalid_to_date", "to"));
    }
    if from > to {
        return Ok(error(from, to, "invalid_date_range", "date_range"));
    }
    let mut contributed = BTreeMap::new();
    let mut withdrawn = BTreeMap::new();
    let mut stmt=c.prepare("SELECT currency,SUM(-amount_minor) FROM canonical_transactions WHERE transaction_kind='investment_contribution' AND posted_date BETWEEN ?1 AND ?2 GROUP BY currency ORDER BY currency")?;
    for row in stmt.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))? {
        let (cur, v) = row?;
        add(&mut contributed, cur, v)?;
    }
    let mut principal = BTreeMap::new();
    let mut interest = BTreeMap::new();
    let mut dividends = BTreeMap::new();
    let mut results = BTreeMap::new();
    let mut net = BTreeMap::new();
    let mut fees = BTreeMap::new();
    let mut withholding = BTreeMap::new();
    let mut deductions = BTreeMap::new();
    let mut gross = BTreeMap::new();
    let mut external = BTreeMap::new();
    let mut existing = BTreeMap::new();
    let mut sales = BTreeMap::new();
    let mut maturities = BTreeMap::new();
    let mut reinvest = BTreeMap::new();
    let mut source_cash: HashMap<(String, String), [i64; 3]> = HashMap::new();
    let mut stmt=c.prepare("SELECT r.account_id,r.operation_type,r.currency,r.gross_amount_minor,r.realized_result_minor,r.fee_minor,r.withholding_minor,r.other_deductions_minor,r.net_cash_minor,r.funding_allocation_id FROM brokerage_operation_heads h JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id WHERE r.effective_date BETWEEN ?1 AND ?2 ORDER BY r.effective_date,r.operation_id")?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, Option<String>>(9)?,
        ))
    })? {
        let (account, kind, cur, g, rr, f, w, d, n, funding) = row?;
        let pools = source_cash.entry((account, cur.clone())).or_default();
        match kind.as_str() {
            "deposit" if funding.is_some() => {
                pools[0] = pools[0]
                    .checked_add(n)
                    .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?
            }
            "buy" => {
                add(&mut gross, cur.clone(), g)?;
                let mut remaining = g;
                let used = remaining.min(pools[0].max(0));
                pools[0] -= used;
                remaining -= used;
                add(&mut external, cur.clone(), used)?;
                let used = remaining.min(pools[1].max(0));
                pools[1] -= used;
                remaining -= used;
                add(&mut sales, cur.clone(), used)?;
                add(&mut reinvest, cur.clone(), used)?;
                let used = remaining.min(pools[2].max(0));
                pools[2] -= used;
                remaining -= used;
                add(&mut reinvest, cur.clone(), used)?;
                add(&mut existing, cur.clone(), remaining)?;
            }
            "sell" => {
                add(&mut principal, cur.clone(), g.checked_sub(rr).unwrap_or(g))?;
                add(&mut results, cur.clone(), rr)?;
                pools[1] = pools[1]
                    .checked_add(n)
                    .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
            }
            "dividend" => {
                add(&mut dividends, cur.clone(), g)?;
                pools[2] = pools[2]
                    .checked_add(n)
                    .ok_or_else(|| anyhow::anyhow!("report amount overflow"))?;
            }
            "withdrawal" => add(&mut withdrawn, cur.clone(), g)?,
            _ => {}
        }
        add(&mut fees, cur.clone(), f)?;
        add(&mut withholding, cur.clone(), w)?;
        add(&mut deductions, cur.clone(), d)?;
        add(&mut net, cur, n)?
    }
    let mut stmt=c.prepare("SELECT r.operation_type,r.currency,r.principal_returned_minor,r.external_capital_minor,r.capitalized_interest_minor,r.gross_interest_minor,r.withholding_minor,r.other_deductions_minor,r.net_cash_received_minor,r.principal_before_minor,r.principal_after_minor FROM cdt_operation_heads h JOIN cdt_operation_revisions r ON r.id=h.current_revision_id WHERE r.effective_date BETWEEN ?1 AND ?2 ORDER BY r.effective_date,r.operation_id")?;
    for row in stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, i64>(10)?,
        ))
    })? {
        let (kind, cur, p, e, ci, gi, w, d, n, before, after) = row?;
        if matches!(kind.as_str(), "constitution" | "renewal") {
            add(&mut gross, cur.clone(), after)?;
        }
        if kind == "renewal" {
            let matured = before.min(after.saturating_sub(e).saturating_sub(ci));
            add(&mut maturities, cur.clone(), matured)?;
            add(&mut reinvest, cur.clone(), matured)?;
        }
        if e > 0 {
            add(&mut external, cur.clone(), e)?
        }
        if ci > 0 {
            add(&mut reinvest, cur.clone(), ci)?
        }
        add(&mut principal, cur.clone(), p)?;
        add(&mut interest, cur.clone(), gi)?;
        add(&mut withholding, cur.clone(), w)?;
        add(&mut deductions, cur.clone(), d)?;
        add(&mut net, cur, n)?;
    }
    let net_external = contributed
        .iter()
        .chain(withdrawn.iter())
        .map(|(k, _)| k.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|k| {
            let v =
                contributed.get(&k).copied().unwrap_or(0) - withdrawn.get(&k).copied().unwrap_or(0);
            (k, v)
        })
        .collect();
    let mut pending = PendingSection::default();
    let mut stmt=c.prepare("SELECT t.id,t.currency,-t.amount_minor,COALESCE(SUM(r.cash_amount_minor),0) FROM canonical_transactions t LEFT JOIN investment_allocation_revisions r ON r.contribution_transaction_id=t.id AND EXISTS(SELECT 1 FROM investment_allocation_heads h WHERE h.current_revision_id=r.id) WHERE t.transaction_kind='investment_contribution' AND t.posted_date<=?1 GROUP BY t.id,t.currency,t.amount_minor HAVING COALESCE(SUM(r.cash_amount_minor),0)<-t.amount_minor ORDER BY t.posted_date,t.id")?;
    pending.allocations = stmt
        .query_map([to], |r| {
            let total: i64 = r.get(2)?;
            let allocated: i64 = r.get(3)?;
            Ok(PendingAllocation {
                contribution_id: r.get(0)?,
                currency: r.get(1)?,
                contributed_minor: total,
                allocated_minor: allocated,
                unallocated_minor: total - allocated,
                status: if allocated == 0 {
                    "pending".into()
                } else {
                    "partial".into()
                },
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
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
        .collect::<rusqlite::Result<_>>()?;
    let mut positions = Vec::new();
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
            let tuple = (
                x.account_id.clone(),
                x.instrument_id.clone(),
                x.currency.clone(),
            );
            if !observed_keys.contains(&tuple) || position_map.contains_key(&tuple) {
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
                    instrument_type: x.instrument_id.as_ref().and_then(|id| {
                        c.query_row(
                            "SELECT instrument_type FROM investment_instruments WHERE id=?1",
                            [id],
                            |r| r.get(0),
                        )
                        .ok()
                    }),
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
                instrument_type: x.instrument_id.as_ref().and_then(|id| {
                    c.query_row(
                        "SELECT instrument_type FROM investment_instruments WHERE id=?1",
                        [id],
                        |r| r.get(0),
                    )
                    .ok()
                }),
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
            capital_withdrawn: totals(withdrawn),
            net_external_contribution: totals(net_external),
        },
        acquisitions_and_reinvestment: AcquisitionSection {
            gross_acquisitions: totals(gross),
            funded_by_external_contribution: totals(external),
            funded_by_existing_cash: totals(existing),
            funded_by_sales: totals(sales),
            funded_by_maturities: totals(maturities),
            reinvestment: totals(reinvest),
        },
        returns_and_income: ReturnSection {
            principal_returned: totals(principal),
            gross_interest: totals(interest),
            gross_dividends: totals(dividends),
            realized_results: totals(results),
            net_cash: totals(net),
        },
        costs_and_withholding: CostSection {
            fees_and_commissions: totals(fees),
            withholding: totals(withholding),
            other_deductions: totals(deductions),
        },
        closing_positions: positions,
        pending_and_reconciliation: pending,
        errors: vec![],
    })
}
