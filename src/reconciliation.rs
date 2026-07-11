use crate::investments::{canonical_exact_decimal, valid_currency};
use crate::storage::ReviewError;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: &str = "tracky.investment-reconciliation.v1";
const FRESH_DAYS: i64 = 7;

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotInput {
    pub observed_at: String,
    pub provider_effective_date: Option<String>,
    pub source: String,
    pub external_reference: Option<String>,
    pub provenance_source: String,
    pub positions: Vec<SnapshotPositionInput>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotPositionInput {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub quantity: Option<String>,
    pub currency: String,
    pub observed_cash_minor: Option<i64>,
    pub observed_value_minor: Option<i64>,
    pub valuation_currency: Option<String>,
    pub observed_price: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct AdjustmentInput {
    pub snapshot_id: String,
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub quantity_delta: Option<String>,
    pub cash_delta_minor: Option<i64>,
    #[serde(default)]
    pub historical_cost_delta_minor: i64,
    pub effective_date: String,
    pub reason: String,
    pub provenance_source: String,
}
pub type AdjustmentReplacement = AdjustmentInput;
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub id: String,
    pub observed_at: String,
    pub provider_effective_date: Option<String>,
    pub source: String,
    pub external_reference: Option<String>,
    pub provenance_source: String,
    pub positions: Vec<SnapshotPosition>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SnapshotPosition {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub quantity: Option<String>,
    pub currency: String,
    pub observed_cash_minor: Option<i64>,
    pub observed_value_minor: Option<i64>,
    pub valuation_currency: Option<String>,
    pub observed_price: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Reconciliation {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub derived_quantity: Option<String>,
    pub observed_quantity: Option<String>,
    pub quantity_difference: Option<String>,
    pub derived_historical_cost_minor: Option<i64>,
    pub cost_currency: Option<String>,
    pub derived_cash_minor: Option<i64>,
    pub observed_cash_minor: Option<i64>,
    pub cash_difference_minor: Option<i64>,
    pub observed_value_minor: Option<i64>,
    pub valuation_currency: Option<String>,
    pub derived_value_minor: Option<i64>,
    pub value_difference_minor: Option<i64>,
    pub status: String,
    pub age_days: i64,
    pub freshness: String,
    pub original_status: String,
    pub original_quantity_difference: Option<String>,
    pub original_cash_difference_minor: Option<i64>,
    pub original_derived_historical_cost_minor: Option<i64>,
    pub original_derived_value_minor: Option<i64>,
    pub original_value_difference_minor: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct DerivedClosingPosition {
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub quantity: Option<String>,
    pub historical_cost_minor: Option<i64>,
    pub cost_currency: Option<String>,
}

pub fn derived_closing_positions(
    c: &Connection,
    as_of: &str,
) -> Result<Vec<DerivedClosingPosition>> {
    if !valid_date(as_of) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for (account_id, instrument_id, currency) in derived_keys(c, as_of)? {
        let value = derived(c, &account_id, instrument_id.as_deref(), &currency, as_of)?;
        out.push(DerivedClosingPosition {
            account_id,
            instrument_id,
            currency,
            quantity: value.quantity,
            historical_cost_minor: value.cost,
            cost_currency: value.cost.map(|_| value.cost_currency),
        });
    }
    Ok(out)
}
#[derive(Debug, Clone, Serialize)]
pub struct Adjustment {
    pub id: String,
    pub adjustment_id: String,
    pub revision: i64,
    pub snapshot_id: String,
    pub account_id: String,
    pub instrument_id: Option<String>,
    pub currency: String,
    pub quantity_delta: Option<String>,
    pub cash_delta_minor: Option<i64>,
    pub historical_cost_delta_minor: i64,
    pub effective_date: String,
    pub reason: String,
    pub provenance_source: String,
    pub correction_reason: Option<String>,
    pub replaces_revision_id: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<Snapshot>,
    pub snapshots: Vec<Snapshot>,
    pub reconciliations: Vec<Reconciliation>,
    pub adjustments: Vec<Adjustment>,
    pub freshness_policy: String,
    pub errors: Vec<ReviewError>,
}

pub fn record(c: &mut Connection, input: SnapshotInput) -> Result<Response> {
    let command = "snapshots record";
    if !valid_timestamp(&input.observed_at) {
        return Ok(err(command, "invalid_observation_time", "observed_at"));
    }
    if input
        .provider_effective_date
        .as_deref()
        .is_some_and(|d| !valid_date(d))
    {
        return Ok(err(command, "invalid_date", "provider_effective_date"));
    }
    if input.source.trim().is_empty() || input.provenance_source.trim().is_empty() {
        return Ok(err(command, "provenance_required", "source"));
    }
    if input.positions.is_empty() {
        return Ok(err(command, "snapshot_empty", "positions"));
    }
    let mut keys = BTreeSet::new();
    let mut validated = Vec::new();
    for mut p in input.positions {
        if !valid_currency(&p.currency)
            || p.valuation_currency
                .as_deref()
                .is_some_and(|x| !valid_currency(x))
        {
            return Ok(err(command, "invalid_currency", "currency"));
        }
        let owned: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND is_owned=1)",
            params![p.account_id],
            |r| r.get(0),
        )?;
        if !owned {
            return Ok(err(command, "account_not_found", "account_id"));
        }
        if let Some(inst) = p.instrument_id.as_deref() {
            if !instrument_compatible(c, &p.account_id, inst)? {
                return Ok(err(command, "instrument_incompatible", "instrument_id"));
            }
        }
        p.quantity = match p.quantity.as_deref() {
            Some(q) => match canonical_exact_decimal(q, true) {
                Some(v) => Some(v),
                None => return Ok(err(command, "invalid_quantity", "quantity")),
            },
            None => None,
        };
        p.observed_price = match p.observed_price.as_deref() {
            Some(q) => match canonical_exact_decimal(q, true) {
                Some(v) => Some(v),
                None => return Ok(err(command, "invalid_price", "observed_price")),
            },
            None => None,
        };
        if p.observed_cash_minor.is_some_and(|x| x < 0)
            || p.observed_value_minor.is_some_and(|x| x < 0)
        {
            return Ok(err(command, "negative_value", "observed_value_minor"));
        }
        if p.observed_value_minor.is_some() != p.valuation_currency.is_some() {
            return Ok(err(
                command,
                "valuation_currency_required",
                "valuation_currency",
            ));
        }
        if p.quantity.is_none() && p.observed_cash_minor.is_none() {
            return Ok(err(command, "snapshot_position_empty", "positions"));
        }
        let key = (
            p.account_id.clone(),
            p.instrument_id.clone(),
            p.currency.clone(),
        );
        if !keys.insert(key) {
            return Ok(err(command, "duplicate_snapshot_position", "positions"));
        }
        validated.push(p);
    }
    let tx = c.transaction()?;
    let id = unique("snapshot", &input.source);
    let inserted=tx.execute("INSERT INTO investment_snapshots(id,observed_at,provider_effective_date,source,external_reference,provenance_source) VALUES(?1,?2,?3,?4,?5,?6)",params![id,input.observed_at,input.provider_effective_date,input.source,input.external_reference,input.provenance_source]);
    if inserted.is_err() {
        return Ok(err(
            command,
            "duplicate_provider_reference",
            "external_reference",
        ));
    }
    for p in validated {
        tx.execute("INSERT INTO investment_snapshot_positions(snapshot_id,account_id,instrument_id,quantity,currency,observed_cash_minor,observed_value_minor,valuation_currency,observed_price) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,p.account_id,p.instrument_id,p.quantity,p.currency,p.observed_cash_minor,p.observed_value_minor,p.valuation_currency,p.observed_price])?;
    }
    let baseline = compare_internal(&tx, &id, &input.observed_at[..10], true)?;
    if !baseline.ok {
        return Ok(baseline);
    }
    tx.commit()?;
    inspect(c, &id, command)
}
pub fn list(c: &Connection) -> Result<Response> {
    let snapshots = load_snapshots(c, None)?;
    Ok(ok("snapshots list", None, snapshots, vec![], vec![]))
}
pub fn inspect(c: &Connection, id: &str, command: &'static str) -> Result<Response> {
    let mut x = load_snapshots(c, Some(id))?;
    if x.is_empty() {
        return Ok(err(command, "snapshot_not_found", "snapshot_id"));
    }
    let s = x.remove(0);
    Ok(ok(
        command,
        Some(s),
        vec![],
        vec![],
        adjustments(c, Some(id))?,
    ))
}
pub fn compare(c: &Connection, id: &str, as_of: &str) -> Result<Response> {
    compare_internal(c, id, as_of, false)
}
fn compare_internal(
    c: &Connection,
    id: &str,
    as_of: &str,
    capture_baseline: bool,
) -> Result<Response> {
    let command = "snapshots compare";
    if !valid_date(as_of) {
        return Ok(err(command, "invalid_date", "as_of"));
    }
    let mut xs = load_snapshots(c, Some(id))?;
    if xs.is_empty() {
        return Ok(err(command, "snapshot_not_found", "snapshot_id"));
    }
    let s = xs.remove(0);
    let governing = s
        .provider_effective_date
        .as_deref()
        .unwrap_or(&s.observed_at[..10]);
    if as_of < &s.observed_at[..10] {
        return Ok(err(command, "comparison_out_of_order", "as_of"));
    }
    let age = days_between(&s.observed_at[..10], as_of).unwrap_or(0);
    let freshness = if age > FRESH_DAYS { "stale" } else { "fresh" };
    let mut observed: BTreeMap<(String, Option<String>, String), SnapshotPosition> = s
        .positions
        .iter()
        .cloned()
        .map(|p| {
            (
                (
                    p.account_id.clone(),
                    p.instrument_id.clone(),
                    p.currency.clone(),
                ),
                p,
            )
        })
        .collect();
    let derived_keys = derived_keys(c, governing)?;
    let mut keys: BTreeSet<_> = observed.keys().cloned().collect();
    for k in &derived_keys {
        let k = k.clone();
        keys.insert(k);
    }
    let mut out = Vec::new();
    for (account, instrument, currency) in keys {
        let obs = observed.remove(&(account.clone(), instrument.clone(), currency.clone()));
        let d = derived(c, &account, instrument.as_deref(), &currency, governing)?;
        let oq = obs.as_ref().and_then(|x| x.quantity.clone());
        let dq = d.quantity.clone();
        let qdiff = decimal_difference(oq.as_deref(), dq.as_deref());
        let oc = obs.as_ref().and_then(|x| x.observed_cash_minor);
        let cdiff = match (oc, d.cash) {
            (Some(o), Some(v)) => o.checked_sub(v),
            _ => None,
        };
        let valuation_currency = obs.as_ref().and_then(|x| x.valuation_currency.clone());
        let observed_value = obs.as_ref().and_then(|x| x.observed_value_minor);
        let derived_value = match (
            observed_value,
            oq.as_deref().and_then(scaled),
            dq.as_deref().and_then(scaled),
        ) {
            (Some(value), Some(observed_quantity), Some(derived_quantity))
                if observed_quantity > 0 =>
            {
                (value as i128)
                    .checked_mul(derived_quantity)
                    .and_then(|amount| i64::try_from(amount / observed_quantity).ok())
            }
            _ => None,
        };
        let value_difference = match (observed_value, derived_value) {
            (Some(observed), Some(derived)) => observed.checked_sub(derived),
            _ => None,
        };
        let incompatible_currency = obs.is_some()
            && dq.is_none()
            && d.cash.is_none()
            && derived_keys
                .iter()
                .any(|(a, i, cur)| a == &account && i == &instrument && cur != &currency);
        let base = if incompatible_currency {
            "currency_mismatch"
        } else if obs.is_none() {
            "missing_snapshot_position"
        } else if dq.is_none() && d.cash.is_none() {
            "missing_derived_position"
        } else if qdiff.as_deref().is_some_and(|x| x != "0") {
            "quantity_mismatch"
        } else if cdiff.is_some_and(|x| x != 0) {
            "cash_mismatch"
        } else if observed_value.is_none() {
            "valuation_unavailable"
        } else {
            "matched"
        };
        let status = if freshness == "stale" { "stale" } else { base };
        let original =
            baseline(c, id, &account, instrument.as_deref(), &currency)?.unwrap_or_else(|| {
                Baseline {
                    status: status.to_string(),
                    quantity_difference: qdiff.clone(),
                    cash_difference_minor: cdiff,
                    derived_historical_cost_minor: d.cost,
                    derived_value_minor: derived_value,
                    value_difference_minor: value_difference,
                }
            });
        if capture_baseline {
            save_baseline(c, id, &account, instrument.as_deref(), &currency, &original)?;
        }
        out.push(Reconciliation {
            account_id: account,
            instrument_id: instrument,
            currency,
            derived_quantity: dq,
            observed_quantity: oq,
            quantity_difference: qdiff,
            derived_historical_cost_minor: d.cost,
            cost_currency: d.cost.map(|_| d.cost_currency),
            derived_cash_minor: d.cash,
            observed_cash_minor: oc,
            cash_difference_minor: cdiff,
            observed_value_minor: observed_value,
            valuation_currency,
            derived_value_minor: derived_value,
            value_difference_minor: value_difference,
            status: status.into(),
            age_days: age,
            freshness: freshness.into(),
            original_status: original.status,
            original_quantity_difference: original.quantity_difference,
            original_cash_difference_minor: original.cash_difference_minor,
            original_derived_historical_cost_minor: original.derived_historical_cost_minor,
            original_derived_value_minor: original.derived_value_minor,
            original_value_difference_minor: original.value_difference_minor,
        });
    }
    Ok(ok(command, Some(s), vec![], out, adjustments(c, Some(id))?))
}

pub(crate) fn capture_baseline(
    connection: &Connection,
    snapshot_id: &str,
    as_of: &str,
) -> Result<Response> {
    compare_internal(connection, snapshot_id, as_of, true)
}

#[derive(Default)]
struct Derived {
    quantity: Option<String>,
    cost: Option<i64>,
    cost_currency: String,
    cash: Option<i64>,
}
fn derived(
    c: &Connection,
    account: &str,
    instrument: Option<&str>,
    currency: &str,
    date: &str,
) -> Result<Derived> {
    let mut d = Derived {
        cost_currency: currency.into(),
        ..Default::default()
    };
    if let Some(inst) = instrument {
        let mut qty: i128 = 0;
        let mut cost = 0i64;
        let mut st=c.prepare("SELECT r.acquired_quantity,r.cash_amount_minor,COALESCE(r.fee_amount_minor,0),r.fee_treatment FROM investment_allocation_heads h JOIN investment_allocation_revisions r ON r.id=h.current_revision_id JOIN canonical_transactions ct ON ct.id=r.contribution_transaction_id WHERE ct.account_id=?1 AND r.instrument_id=?2 AND r.cash_currency=?3 AND ct.posted_date<=?4 AND NOT EXISTS(SELECT 1 FROM investment_allocation_consumptions x WHERE x.allocation_id=r.allocation_id)")?;
        let rows = st.query_map(params![account, inst, currency, date], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for row in rows {
            let (q, a, f, t) = row?;
            qty = qty
                .checked_add(scaled(&q).ok_or_else(|| anyhow::anyhow!("invalid stored quantity"))?)
                .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
            let capitalized_fee = if t.as_deref() == Some("capitalized") {
                f
            } else {
                0
            };
            cost = cost
                .checked_add(
                    a.checked_add(capitalized_fee)
                        .ok_or_else(|| anyhow::anyhow!("cost overflow"))?,
                )
                .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
        }
        let mut bst=c.prepare("SELECT operation_type,quantity,historical_cost_minor FROM (SELECT r.operation_type,r.quantity,r.historical_cost_minor,r.effective_date,(SELECT MIN(rowid) FROM brokerage_operation_revisions z WHERE z.operation_id=r.operation_id) AS sequence FROM brokerage_operation_revisions r WHERE r.account_id=?1 AND r.instrument_id=?2 AND r.currency=?3 AND r.effective_date<=?4 AND r.id=(SELECT rr.id FROM brokerage_operation_revisions rr WHERE rr.operation_id=r.operation_id AND rr.effective_date<=?4 ORDER BY rr.revision DESC LIMIT 1) UNION ALL SELECT 'adjustment',r.quantity_delta,r.historical_cost_delta_minor,r.effective_date,r.rowid FROM investment_adjustment_revisions r WHERE r.account_id=?1 AND r.instrument_id=?2 AND r.currency=?3 AND r.effective_date<=?4 AND r.id=(SELECT rr.id FROM investment_adjustment_revisions rr WHERE rr.adjustment_id=r.adjustment_id AND rr.effective_date<=?4 ORDER BY rr.revision DESC LIMIT 1)) ORDER BY effective_date,sequence")?;
        let rows = bst.query_map(params![account, inst, currency, date], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (k, q, cst) = row?;
            let q = scaled(q.as_deref().unwrap_or("0"))
                .ok_or_else(|| anyhow::anyhow!("invalid stored quantity"))?;
            if k == "buy" {
                qty = qty
                    .checked_add(q)
                    .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
                cost = cost
                    .checked_add(cst)
                    .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
            } else if k == "sell" {
                if qty == q {
                    cost = 0
                } else if qty > 0 {
                    let disposed = i64::try_from(
                        (cost as i128)
                            .checked_mul(q)
                            .ok_or_else(|| anyhow::anyhow!("cost overflow"))?
                            / qty,
                    )?;
                    cost = cost
                        .checked_sub(disposed)
                        .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
                }
                qty = qty
                    .checked_sub(q)
                    .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
            } else if k == "adjustment" {
                qty = qty
                    .checked_add(q)
                    .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
                cost = cost
                    .checked_add(cst)
                    .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
            }
        }
        let cdt:Option<i64>=c.query_row("SELECT r.principal_after_minor FROM cdt_positions p JOIN cdt_operation_revisions r ON r.cdt_position_id=p.id WHERE p.account_id=?1 AND p.instrument_id=?2 AND r.currency=?3 AND r.effective_date<=?4 AND r.id=(SELECT rr.id FROM cdt_operation_revisions rr WHERE rr.operation_id=r.operation_id AND rr.effective_date<=?4 ORDER BY rr.revision DESC LIMIT 1) ORDER BY r.effective_date DESC LIMIT 1",params![account,inst,currency,date],|r|r.get(0)).optional()?;
        if let Some(v) = cdt {
            qty = qty
                .checked_add(
                    scaled(&v.to_string())
                        .ok_or_else(|| anyhow::anyhow!("invalid CDT principal"))?,
                )
                .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
            cost = cost
                .checked_add(v)
                .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
        }
        if qty != 0 {
            d.quantity = Some(unscaled(qty));
            d.cost = Some(cost)
        }
    } else {
        let cash:Option<i64>=c.query_row("SELECT SUM(r.net_cash_minor) FROM brokerage_operation_revisions r WHERE r.account_id=?1 AND r.currency=?2 AND r.effective_date<=?3 AND r.id=(SELECT rr.id FROM brokerage_operation_revisions rr WHERE rr.operation_id=r.operation_id AND rr.effective_date<=?3 ORDER BY rr.revision DESC LIMIT 1)",params![account,currency,date],|r|r.get(0))?;
        let mut cash = cash.unwrap_or(0);
        apply_cash_adjustments(c, account, currency, date, &mut cash)?;
        if cash != 0 {
            d.cash = Some(cash)
        }
    }
    Ok(d)
}
fn apply_cash_adjustments(
    c: &Connection,
    account_id: &str,
    currency: &str,
    date: &str,
    cash: &mut i64,
) -> Result<()> {
    let mut st=c.prepare("SELECT r.cash_delta_minor FROM investment_adjustment_revisions r WHERE r.account_id=?1 AND r.instrument_id IS NULL AND r.currency=?2 AND r.effective_date<=?3 AND r.id=(SELECT rr.id FROM investment_adjustment_revisions rr WHERE rr.adjustment_id=r.adjustment_id AND rr.effective_date<=?3 ORDER BY rr.revision DESC LIMIT 1)")?;
    let rows = st.query_map(params![account_id, currency, date], |row| {
        row.get::<_, Option<i64>>(0)
    })?;
    for row in rows {
        if let Some(value) = row? {
            *cash = cash
                .checked_add(value)
                .ok_or_else(|| anyhow::anyhow!("cash overflow"))?;
        }
    }
    Ok(())
}
fn derived_keys(c: &Connection, date: &str) -> Result<Vec<(String, Option<String>, String)>> {
    let mut out = vec![];
    for sql in [
        "SELECT DISTINCT ct.account_id,r.instrument_id,r.cash_currency FROM investment_allocation_heads h JOIN investment_allocation_revisions r ON r.id=h.current_revision_id JOIN canonical_transactions ct ON ct.id=r.contribution_transaction_id WHERE ct.posted_date<=?1 AND NOT EXISTS(SELECT 1 FROM investment_allocation_consumptions x WHERE x.allocation_id=r.allocation_id)",
        "SELECT DISTINCT r.account_id,r.instrument_id,r.currency FROM brokerage_operation_revisions r WHERE r.effective_date<=?1 AND r.id=(SELECT rr.id FROM brokerage_operation_revisions rr WHERE rr.operation_id=r.operation_id AND rr.effective_date<=?1 ORDER BY rr.revision DESC LIMIT 1)",
        "SELECT DISTINCT r.account_id,NULL,r.currency FROM brokerage_operation_revisions r WHERE r.effective_date<=?1 AND r.id=(SELECT rr.id FROM brokerage_operation_revisions rr WHERE rr.operation_id=r.operation_id AND rr.effective_date<=?1 ORDER BY rr.revision DESC LIMIT 1)",
        "SELECT DISTINCT p.account_id,p.instrument_id,r.currency FROM cdt_positions p JOIN cdt_operation_revisions r ON r.cdt_position_id=p.id WHERE r.effective_date<=?1 AND r.id=(SELECT rr.id FROM cdt_operation_revisions rr WHERE rr.operation_id=r.operation_id AND rr.effective_date<=?1 ORDER BY rr.revision DESC LIMIT 1)",
        "SELECT DISTINCT r.account_id,r.instrument_id,r.currency FROM investment_adjustment_revisions r WHERE r.effective_date<=?1 AND r.id=(SELECT rr.id FROM investment_adjustment_revisions rr WHERE rr.adjustment_id=r.adjustment_id AND rr.effective_date<=?1 ORDER BY rr.revision DESC LIMIT 1)",
    ] {
        let mut statement = c.prepare(sql)?;
        let rows = statement.query_map(params![date], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        for row in rows {
            out.push(row?)
        }
    }
    Ok(out)
}

pub fn adjust(c: &mut Connection, i: AdjustmentInput) -> Result<Response> {
    let command = "snapshots adjust";
    insert_adjustment(c, command, None, None, i)
}
pub fn replace_adjustment(
    c: &mut Connection,
    id: &str,
    reason: &str,
    r: AdjustmentReplacement,
) -> Result<Response> {
    if reason.trim().is_empty() {
        return Ok(err(
            "snapshots replace-adjustment",
            "correction_reason_required",
            "reason",
        ));
    }
    insert_adjustment(c, "snapshots replace-adjustment", Some(id), Some(reason), r)
}
fn insert_adjustment(
    c: &mut Connection,
    command: &'static str,
    id: Option<&str>,
    correction: Option<&str>,
    mut r: AdjustmentReplacement,
) -> Result<Response> {
    if r.reason.trim().is_empty() || r.provenance_source.trim().is_empty() {
        return Ok(err(command, "reason_and_provenance_required", "reason"));
    }
    if !valid_date(&r.effective_date) || !valid_currency(&r.currency) {
        return Ok(err(command, "invalid_adjustment", "effective_date"));
    }
    r.quantity_delta = match r.quantity_delta.as_deref() {
        Some(x) => match signed_decimal(x) {
            Some(v) => Some(v),
            None => return Ok(err(command, "invalid_quantity", "quantity_delta")),
        },
        None => None,
    };
    if r.quantity_delta.is_none() && r.cash_delta_minor.is_none() {
        return Ok(err(command, "adjustment_empty", "quantity_delta"));
    }
    let tx = c.transaction()?;
    let snap: Option<String> = tx
        .query_row(
            "SELECT observed_at FROM investment_snapshots WHERE id=?1",
            params![r.snapshot_id],
            |x| x.get(0),
        )
        .optional()?;
    let Some(observed) = snap else {
        return Ok(err(command, "snapshot_not_found", "snapshot_id"));
    };
    if r.effective_date.as_str() > &observed[..10] {
        return Ok(err(command, "adjustment_out_of_order", "effective_date"));
    }
    let owned: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND is_owned=1)",
        params![r.account_id],
        |row| row.get(0),
    )?;
    if !owned {
        return Ok(err(command, "account_not_found", "account_id"));
    }
    let snapshot_key_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM investment_snapshot_positions WHERE snapshot_id=?1 AND account_id=?2 AND instrument_id IS ?3 AND currency=?4)",
        params![r.snapshot_id, r.account_id, r.instrument_id, r.currency],
        |row| row.get(0),
    )?;
    if !snapshot_key_exists {
        return Ok(err(
            command,
            "adjustment_snapshot_mismatch",
            "instrument_id",
        ));
    }
    if let Some(instrument_id) = r.instrument_id.as_deref() {
        if !instrument_compatible(&tx, &r.account_id, instrument_id)? {
            return Ok(err(command, "instrument_incompatible", "instrument_id"));
        }
    }
    if id.is_none() {
        let duplicate: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM investment_adjustment_revisions WHERE revision=1 AND snapshot_id=?1 AND account_id=?2 AND instrument_id IS ?3 AND currency=?4)",
            params![r.snapshot_id, r.account_id, r.instrument_id, r.currency],
            |row| row.get(0),
        )?;
        if duplicate {
            return Ok(err(command, "duplicate_adjustment", "snapshot_id"));
        }
    }
    let (aid, rev, old) = if let Some(id) = id {
        let x:Option<(String,i64,String)>=tx.query_row("SELECT r.adjustment_id,r.revision,r.id FROM investment_adjustment_heads h JOIN investment_adjustment_revisions r ON r.id=h.current_revision_id WHERE h.adjustment_id=?1",params![id],|x|Ok((x.get(0)?,x.get(1)?,x.get(2)?))).optional()?;
        let Some(x) = x else {
            return Ok(err(command, "adjustment_not_found", "adjustment_id"));
        };
        (x.0, x.1 + 1, Some(x.2))
    } else {
        (unique("adjustment", &r.snapshot_id), 1, None)
    };
    let rid = unique("adjrev", &aid);
    tx.execute("INSERT INTO investment_adjustment_revisions(id,adjustment_id,revision,snapshot_id,account_id,instrument_id,currency,quantity_delta,cash_delta_minor,historical_cost_delta_minor,effective_date,reason,provenance_source,correction_reason,replaces_revision_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",params![rid,aid,rev,r.snapshot_id,r.account_id,r.instrument_id,r.currency,r.quantity_delta,r.cash_delta_minor,r.historical_cost_delta_minor,r.effective_date,r.reason,r.provenance_source,correction,old])?;
    if rev == 1 {
        tx.execute(
            "INSERT INTO investment_adjustment_heads VALUES(?1,?2)",
            params![aid, rid],
        )?;
    } else {
        tx.execute(
            "UPDATE investment_adjustment_heads SET current_revision_id=?1 WHERE adjustment_id=?2",
            params![rid, aid],
        )?;
    }
    for validation_date in [r.effective_date.as_str(), &observed[..10]] {
        let derived_state = derived(
            &tx,
            &r.account_id,
            r.instrument_id.as_deref(),
            &r.currency,
            validation_date,
        )?;
        if derived_state.cost.is_some_and(|x| x < 0)
            || derived_state.cash.is_some_and(|x| x < 0)
            || derived_state
                .quantity
                .as_deref()
                .and_then(scaled)
                .is_some_and(|x| x < 0)
        {
            return Ok(err(
                command,
                "adjustment_breaks_position",
                "replacement_json",
            ));
        }
    }
    tx.commit()?;
    Ok(ok(command, None, vec![], vec![], adjustments(c, None)?))
}

fn load_snapshots(c: &Connection, id: Option<&str>) -> Result<Vec<Snapshot>> {
    let mut st=c.prepare("SELECT id,observed_at,provider_effective_date,source,external_reference,provenance_source FROM investment_snapshots WHERE (?1 IS NULL OR id=?1) ORDER BY observed_at,id")?;
    let rows = st.query_map(params![id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })?;
    let mut out = vec![];
    for row in rows {
        let (
            id,
            observed_at,
            provider_effective_date,
            source,
            external_reference,
            provenance_source,
        ) = row?;
        let mut ps=c.prepare("SELECT account_id,instrument_id,quantity,currency,observed_cash_minor,observed_value_minor,valuation_currency,observed_price FROM investment_snapshot_positions WHERE snapshot_id=?1 ORDER BY account_id,instrument_id,currency")?;
        let positions = ps
            .query_map(params![id], |r| {
                Ok(SnapshotPosition {
                    account_id: r.get(0)?,
                    instrument_id: r.get(1)?,
                    quantity: r.get(2)?,
                    currency: r.get(3)?,
                    observed_cash_minor: r.get(4)?,
                    observed_value_minor: r.get(5)?,
                    valuation_currency: r.get(6)?,
                    observed_price: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        out.push(Snapshot {
            id,
            observed_at,
            provider_effective_date,
            source,
            external_reference,
            provenance_source,
            positions,
        })
    }
    Ok(out)
}
fn adjustments(c: &Connection, snapshot: Option<&str>) -> Result<Vec<Adjustment>> {
    let mut st=c.prepare("SELECT r.id,r.adjustment_id,r.revision,r.snapshot_id,r.account_id,r.instrument_id,r.currency,r.quantity_delta,r.cash_delta_minor,r.historical_cost_delta_minor,r.effective_date,r.reason,r.provenance_source,r.correction_reason,r.replaces_revision_id,r.created_at FROM investment_adjustment_revisions r WHERE (?1 IS NULL OR r.snapshot_id=?1) ORDER BY r.adjustment_id,r.revision")?;
    let values = st
        .query_map(params![snapshot], |r| {
            Ok(Adjustment {
                id: r.get(0)?,
                adjustment_id: r.get(1)?,
                revision: r.get(2)?,
                snapshot_id: r.get(3)?,
                account_id: r.get(4)?,
                instrument_id: r.get(5)?,
                currency: r.get(6)?,
                quantity_delta: r.get(7)?,
                cash_delta_minor: r.get(8)?,
                historical_cost_delta_minor: r.get(9)?,
                effective_date: r.get(10)?,
                reason: r.get(11)?,
                provenance_source: r.get(12)?,
                correction_reason: r.get(13)?,
                replaces_revision_id: r.get(14)?,
                created_at: r.get(15)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(values)
}
struct Baseline {
    status: String,
    quantity_difference: Option<String>,
    cash_difference_minor: Option<i64>,
    derived_historical_cost_minor: Option<i64>,
    derived_value_minor: Option<i64>,
    value_difference_minor: Option<i64>,
}
fn baseline(
    c: &Connection,
    s: &str,
    a: &str,
    i: Option<&str>,
    cur: &str,
) -> Result<Option<Baseline>> {
    Ok(c.query_row("SELECT status,quantity_difference,cash_difference_minor,derived_historical_cost_minor,derived_value_minor,value_difference_minor FROM investment_snapshot_baselines WHERE snapshot_id=?1 AND account_id=?2 AND instrument_id IS ?3 AND currency=?4",params![s,a,i,cur],|r|Ok(Baseline{status:r.get(0)?,quantity_difference:r.get(1)?,cash_difference_minor:r.get(2)?,derived_historical_cost_minor:r.get(3)?,derived_value_minor:r.get(4)?,value_difference_minor:r.get(5)?})).optional()?)
}
fn save_baseline(
    c: &Connection,
    s: &str,
    a: &str,
    i: Option<&str>,
    cur: &str,
    v: &Baseline,
) -> Result<()> {
    c.execute("INSERT OR IGNORE INTO investment_snapshot_baselines(snapshot_id,account_id,instrument_id,currency,status,quantity_difference,cash_difference_minor,derived_historical_cost_minor,derived_value_minor,value_difference_minor) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![s,a,i,cur,v.status,v.quantity_difference,v.cash_difference_minor,v.derived_historical_cost_minor,v.derived_value_minor,v.value_difference_minor])?;
    Ok(())
}
fn ok(
    command: &'static str,
    snapshot: Option<Snapshot>,
    snapshots: Vec<Snapshot>,
    reconciliations: Vec<Reconciliation>,
    adjustments: Vec<Adjustment>,
) -> Response {
    Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: true,
        snapshot,
        snapshots,
        reconciliations,
        adjustments,
        freshness_policy: "fresh through 7 calendar days after observed_at; stale afterwards"
            .into(),
        errors: vec![],
    }
}
fn err(command: &'static str, code: &'static str, path: &'static str) -> Response {
    Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: false,
        snapshot: None,
        snapshots: vec![],
        reconciliations: vec![],
        adjustments: vec![],
        freshness_policy: "fresh through 7 calendar days after observed_at; stale afterwards"
            .into(),
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
fn valid_date(v: &str) -> bool {
    chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok()
}
fn instrument_compatible(c: &Connection, account_id: &str, instrument_id: &str) -> Result<bool> {
    let context: Option<String> = c
        .query_row(
            "SELECT CASE WHEN EXISTS(SELECT 1 FROM brokerage_accounts WHERE account_id=?1) THEN 'security' WHEN EXISTS(SELECT 1 FROM cdt_positions WHERE account_id=?1) THEN 'fixed_income' ELSE 'any' END",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    let expected = context.as_deref().unwrap_or("any");
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM investment_instruments WHERE id=?1 AND (?2='any' OR instrument_type=?2))",
        params![instrument_id, expected],
        |row| row.get(0),
    )?)
}
fn valid_timestamp(v: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(v).is_ok()
}
fn days_between(a: &str, b: &str) -> Option<i64> {
    Some(
        (chrono::NaiveDate::parse_from_str(b, "%Y-%m-%d").ok()?
            - chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d").ok()?)
        .num_days(),
    )
}
fn scaled(v: &str) -> Option<i128> {
    let neg = v.starts_with('-');
    let v = v.trim_start_matches('-');
    let mut p = v.split('.');
    let whole = p.next()?.parse::<i128>().ok()?;
    let f = p.next().unwrap_or("");
    if p.next().is_some() || f.len() > 18 {
        return None;
    }
    let frac = if f.is_empty() {
        0
    } else {
        format!("{f:0<18}").parse().ok()?
    };
    whole
        .checked_mul(1_000_000_000_000_000_000i128)?
        .checked_add(frac)?
        .checked_mul(if neg { -1 } else { 1 })
}
fn unscaled(v: i128) -> String {
    let neg = v < 0;
    let n = v.unsigned_abs();
    let w = n / 1_000_000_000_000_000_000u128;
    let f = n % 1_000_000_000_000_000_000u128;
    if f == 0 {
        return format!("{}{}", if neg { "-" } else { "" }, w);
    }
    let x = format!("{f:018}").trim_end_matches('0').to_string();
    format!("{}{}.{}", if neg { "-" } else { "" }, w, x)
}
fn signed_decimal(v: &str) -> Option<String> {
    let n = scaled(v)?;
    Some(unscaled(n))
}
fn decimal_difference(observed: Option<&str>, derived: Option<&str>) -> Option<String> {
    match (observed, derived) {
        (Some(o), Some(d)) => Some(unscaled(scaled(o)?.checked_sub(scaled(d)?)?)),
        (Some(o), None) => Some(o.into()),
        (None, Some(d)) => Some(unscaled(scaled(d)?.checked_neg()?)),
        _ => None,
    }
}
fn unique(prefix: &str, seed: &str) -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut h = Sha256::new();
    h.update(prefix);
    h.update(seed);
    h.update(n.to_string());
    let digest = h.finalize();
    let suffix: String = digest[..10]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{prefix}_{suffix}")
}
