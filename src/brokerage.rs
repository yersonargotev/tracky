use crate::storage::ReviewError;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub const BROKERAGE_SCHEMA_VERSION: &str = "tracky.brokerage.v1";
const SCALE: i128 = 1_000_000_000;

#[derive(Debug, Clone)]
pub struct BrokerageOpenInput {
    pub account_id: String,
    pub opened_date: String,
}
#[derive(Debug, Clone)]
pub struct BrokerageDepositInput {
    pub account_id: String,
    pub allocation_id: String,
    pub effective_date: String,
    pub amount_minor: i64,
    pub currency: String,
}
#[derive(Debug, Clone)]
pub struct BrokerageBuyInput {
    pub account_id: String,
    pub instrument_id: String,
    pub effective_date: String,
    pub quantity: String,
    pub gross_amount_minor: i64,
    pub fee_minor: i64,
    pub fee_treatment: String,
    pub component_id: Option<String>,
}
#[derive(Debug, Clone)]
pub struct BrokerageSellInput {
    pub account_id: String,
    pub instrument_id: String,
    pub effective_date: String,
    pub quantity: String,
    pub gross_proceeds_minor: i64,
    pub fee_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_minor: i64,
    pub component_id: Option<String>,
}
#[derive(Debug, Clone)]
pub struct BrokerageDividendInput {
    pub account_id: String,
    pub instrument_id: String,
    pub effective_date: String,
    pub gross_dividend_minor: i64,
    pub fee_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_minor: i64,
    pub component_id: Option<String>,
}
#[derive(Debug, Clone)]
pub struct BrokerageWithdrawalInput {
    pub account_id: String,
    pub effective_date: String,
    pub amount_minor: i64,
    pub currency: String,
    pub destination_account_id: Option<String>,
    pub linked_transaction_id: Option<String>,
}
#[derive(Debug, Clone)]
pub struct BrokerageReplacementInput {
    pub operation_id: String,
    pub reason: String,
    pub replacement: BrokerageOperationReplacement,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrokerageOperationReplacement {
    pub operation_type: String,
    pub effective_date: String,
    pub currency: String,
    pub instrument_id: Option<String>,
    pub quantity: Option<String>,
    #[serde(default)]
    pub gross_amount_minor: i64,
    #[serde(default)]
    pub fee_minor: i64,
    pub fee_treatment: Option<String>,
    #[serde(default)]
    pub withholding_minor: i64,
    #[serde(default)]
    pub other_deductions_minor: i64,
    #[serde(default)]
    pub net_cash_minor: i64,
    pub funding_allocation_id: Option<String>,
    pub destination_account_id: Option<String>,
    pub linked_transaction_id: Option<String>,
    pub component_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrokerageOperation {
    pub id: String,
    pub operation_id: String,
    pub revision: i64,
    pub operation_type: String,
    pub effective_date: String,
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<String>,
    pub gross_amount_minor: i64,
    pub historical_cost_minor: i64,
    pub realized_result_minor: i64,
    pub fee_minor: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_treatment: Option<String>,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_minor: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_allocation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_transaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_id: Option<String>,
    pub provenance_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correction_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces_revision_id: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BrokerageCash {
    pub currency: String,
    pub available_minor: i64,
    pub external_capital_minor: i64,
    pub withdrawals_minor: i64,
    pub gross_dividends_minor: i64,
    pub gross_sale_proceeds_minor: i64,
    pub realized_result_minor: i64,
    pub fees_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub latest_operation_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BrokeragePosition {
    pub instrument_id: String,
    pub quantity: String,
    pub historical_cost_minor: i64,
    pub cost_currency: String,
    pub latest_operation_id: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct BrokerageAccount {
    pub account_id: String,
    pub opened_date: String,
    pub cash: Vec<BrokerageCash>,
    pub positions: Vec<BrokeragePosition>,
    pub active_operations: Vec<BrokerageOperation>,
}
#[derive(Debug, Clone, Serialize)]
pub struct BrokerageResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub accounts: Vec<BrokerageAccount>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub operation_history: Vec<BrokerageOperation>,
    pub errors: Vec<ReviewError>,
}

#[derive(Clone)]
struct Terms {
    kind: String,
    date: String,
    currency: String,
    instrument: Option<String>,
    quantity: Option<String>,
    gross: i64,
    cost: i64,
    result: i64,
    fee: i64,
    fee_treatment: Option<String>,
    withholding: i64,
    other: i64,
    net: i64,
    allocation: Option<String>,
    destination: Option<String>,
    linked: Option<String>,
    component: Option<String>,
}
struct ReplacementHead {
    revision_id: String,
    account_id: String,
    revision: i64,
    operation_type: String,
    funding_allocation_id: Option<String>,
    gross_amount_minor: i64,
    currency: String,
}

pub fn open_brokerage(c: &mut Connection, input: BrokerageOpenInput) -> Result<BrokerageResponse> {
    let command = "brokerages open";
    if !valid_date(&input.opened_date) {
        return Ok(err(command, "invalid_date", "opened_date"));
    }
    let owned: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND is_owned=1)",
        params![input.account_id],
        |r| r.get(0),
    )?;
    if !owned {
        return Ok(err(command, "account_not_found", "account_id"));
    }
    c.execute("INSERT INTO brokerage_accounts(account_id,opened_date,provenance_source) VALUES(?1,?2,'manual_entry')",params![input.account_id,input.opened_date])?;
    inspect(c, command, None, false)
}

pub fn deposit(c: &mut Connection, i: BrokerageDepositInput) -> Result<BrokerageResponse> {
    let command = "brokerages deposit";
    if i.amount_minor <= 0 {
        return Ok(err(command, "invalid_amount", "amount_minor"));
    }
    let tx = c.transaction()?;
    let consumed: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM investment_allocation_consumptions WHERE allocation_id=?1)",
        params![i.allocation_id],
        |r| r.get(0),
    )?;
    if consumed {
        return Ok(err(command, "allocation_already_consumed", "allocation_id"));
    }
    let row:Option<(String,i64,String)>=tx.query_row("SELECT ct.account_id,r.cash_amount_minor,r.cash_currency FROM investment_allocation_heads h JOIN investment_allocation_revisions r ON r.id=h.current_revision_id JOIN canonical_transactions ct ON ct.id=r.contribution_transaction_id WHERE h.allocation_id=?1",params![i.allocation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((_source, amount, currency)) = row else {
        return Ok(err(command, "allocation_not_found", "allocation_id"));
    };
    if amount != i.amount_minor || currency != i.currency {
        return Ok(err(command, "allocation_mismatch", "amount_minor"));
    }
    let t = Terms {
        kind: "deposit".into(),
        date: i.effective_date,
        currency: i.currency,
        instrument: None,
        quantity: None,
        gross: i.amount_minor,
        cost: 0,
        result: 0,
        fee: 0,
        fee_treatment: None,
        withholding: 0,
        other: 0,
        net: i.amount_minor,
        allocation: Some(i.allocation_id),
        destination: None,
        linked: None,
        component: None,
    };
    if let Some(e) = validate(&tx, &i.account_id, &t, None)? {
        return Ok(e);
    }
    let op = insert(&tx, &i.account_id, &t, 1, None, None)?;
    tx.execute("INSERT INTO investment_allocation_consumptions(allocation_id,consumer_kind,cdt_position_id,consumer_operation_id) VALUES(?1,'brokerage_deposit',NULL,?2)",params![t.allocation,op])?;
    tx.commit()?;
    inspect(c, command, Some(&i.account_id), false)
}

pub fn buy(c: &mut Connection, i: BrokerageBuyInput) -> Result<BrokerageResponse> {
    let command = "brokerages buy";
    let fee_cap = i.fee_treatment == "capitalized";
    if !fee_cap && i.fee_treatment != "separate" {
        return Ok(err(command, "invalid_fee_treatment", "fee_treatment"));
    }
    let tx = c.transaction()?;
    let currency = instrument_currency(&tx, &i.instrument_id, "security")?
        .ok_or_else(|| anyhow::anyhow!("missing"));
    let Ok(currency) = currency else {
        return Ok(err(command, "instrument_incompatible", "instrument_id"));
    };
    let cost = match i
        .gross_amount_minor
        .checked_add(if fee_cap { i.fee_minor } else { 0 })
    {
        Some(x) => x,
        None => return Ok(err(command, "amount_overflow", "gross_amount_minor")),
    };
    let cash_out = match i.gross_amount_minor.checked_add(i.fee_minor) {
        Some(value) => value,
        None => return Ok(err(command, "amount_overflow", "fee_minor")),
    };
    let Some(quantity) = parse_decimal(&i.quantity).map(format_decimal) else {
        return Ok(err(command, "invalid_quantity", "quantity"));
    };
    let t = Terms {
        kind: "buy".into(),
        date: i.effective_date,
        currency,
        instrument: Some(i.instrument_id),
        quantity: Some(quantity),
        gross: i.gross_amount_minor,
        cost,
        result: 0,
        fee: i.fee_minor,
        fee_treatment: Some(i.fee_treatment),
        withholding: 0,
        other: 0,
        net: -cash_out,
        allocation: None,
        destination: None,
        linked: None,
        component: i.component_id,
    };
    if let Some(e) = validate(&tx, &i.account_id, &t, None)? {
        return Ok(e);
    }
    insert(&tx, &i.account_id, &t, 1, None, None)?;
    tx.commit()?;
    inspect(c, command, Some(&i.account_id), false)
}

pub fn sell(c: &mut Connection, i: BrokerageSellInput) -> Result<BrokerageResponse> {
    let command = "brokerages sell";
    let tx = c.transaction()?;
    let currency = match instrument_currency(&tx, &i.instrument_id, "security")? {
        Some(x) => x,
        None => return Ok(err(command, "instrument_incompatible", "instrument_id")),
    };
    let expected = i
        .gross_proceeds_minor
        .checked_sub(i.fee_minor)
        .and_then(|x| x.checked_sub(i.withholding_minor))
        .and_then(|x| x.checked_sub(i.other_deductions_minor));
    if expected != Some(i.net_cash_minor) {
        return Ok(err(command, "net_reconciliation_failed", "net_cash_minor"));
    }
    let state = state(&tx, &i.account_id)?;
    let p = state
        .positions
        .get(&(i.instrument_id.clone(), currency.clone()));
    let sold = parse_decimal(&i.quantity);
    let Some(sold) = sold else {
        return Ok(err(command, "invalid_quantity", "quantity"));
    };
    let Some(p) = p else {
        return Ok(err(command, "insufficient_position", "quantity"));
    };
    if sold <= 0 || sold > p.quantity {
        return Ok(err(command, "insufficient_position", "quantity"));
    }
    let cost = if sold == p.quantity {
        p.cost
    } else {
        i64::try_from(
            (p.cost as i128)
                .checked_mul(sold)
                .ok_or_else(|| anyhow::anyhow!("cost overflow"))?
                / p.quantity,
        )
        .map_err(|_| anyhow::anyhow!("cost overflow"))?
    };
    let t = Terms {
        kind: "sell".into(),
        date: i.effective_date,
        currency,
        instrument: Some(i.instrument_id),
        quantity: Some(format_decimal(sold)),
        gross: i.gross_proceeds_minor,
        cost,
        result: match i.gross_proceeds_minor.checked_sub(cost) {
            Some(value) => value,
            None => return Ok(err(command, "amount_overflow", "gross_proceeds_minor")),
        },
        fee: i.fee_minor,
        fee_treatment: None,
        withholding: i.withholding_minor,
        other: i.other_deductions_minor,
        net: i.net_cash_minor,
        allocation: None,
        destination: None,
        linked: None,
        component: i.component_id,
    };
    if let Some(e) = validate(&tx, &i.account_id, &t, None)? {
        return Ok(e);
    }
    insert(&tx, &i.account_id, &t, 1, None, None)?;
    tx.commit()?;
    inspect(c, command, Some(&i.account_id), false)
}

pub fn dividend(c: &mut Connection, i: BrokerageDividendInput) -> Result<BrokerageResponse> {
    let command = "brokerages dividend";
    let tx = c.transaction()?;
    let currency = match instrument_currency(&tx, &i.instrument_id, "security")? {
        Some(x) => x,
        None => return Ok(err(command, "instrument_incompatible", "instrument_id")),
    };
    let held = state(&tx, &i.account_id)?
        .positions
        .get(&(i.instrument_id.clone(), currency.clone()))
        .is_some_and(|position| position.quantity > 0);
    if !held {
        return Ok(err(command, "dividend_without_position", "instrument_id"));
    }
    let expected = i
        .gross_dividend_minor
        .checked_sub(i.fee_minor)
        .and_then(|x| x.checked_sub(i.withholding_minor))
        .and_then(|x| x.checked_sub(i.other_deductions_minor));
    if expected != Some(i.net_cash_minor) {
        return Ok(err(command, "net_reconciliation_failed", "net_cash_minor"));
    }
    let t = Terms {
        kind: "dividend".into(),
        date: i.effective_date,
        currency,
        instrument: Some(i.instrument_id),
        quantity: None,
        gross: i.gross_dividend_minor,
        cost: 0,
        result: 0,
        fee: i.fee_minor,
        fee_treatment: None,
        withholding: i.withholding_minor,
        other: i.other_deductions_minor,
        net: i.net_cash_minor,
        allocation: None,
        destination: None,
        linked: None,
        component: i.component_id,
    };
    if let Some(e) = validate(&tx, &i.account_id, &t, None)? {
        return Ok(e);
    }
    insert(&tx, &i.account_id, &t, 1, None, None)?;
    tx.commit()?;
    inspect(c, command, Some(&i.account_id), false)
}

pub fn withdraw(c: &mut Connection, i: BrokerageWithdrawalInput) -> Result<BrokerageResponse> {
    let command = "brokerages withdraw";
    let tx = c.transaction()?;
    let t = Terms {
        kind: "withdrawal".into(),
        date: i.effective_date,
        currency: i.currency,
        instrument: None,
        quantity: None,
        gross: i.amount_minor,
        cost: 0,
        result: 0,
        fee: 0,
        fee_treatment: None,
        withholding: 0,
        other: 0,
        net: -i.amount_minor,
        allocation: None,
        destination: i.destination_account_id,
        linked: i.linked_transaction_id,
        component: None,
    };
    if let Some(e) = validate(&tx, &i.account_id, &t, None)? {
        return Ok(e);
    }
    insert(&tx, &i.account_id, &t, 1, None, None)?;
    tx.commit()?;
    inspect(c, command, Some(&i.account_id), false)
}

pub fn replace_operation(
    c: &mut Connection,
    i: BrokerageReplacementInput,
) -> Result<BrokerageResponse> {
    let command = "brokerages replace-operation";
    if i.reason.trim().is_empty() {
        return Ok(err(command, "correction_reason_required", "reason"));
    }
    let tx = c.transaction()?;
    let old: Option<ReplacementHead> = tx.query_row("SELECT r.id,r.account_id,r.revision,r.operation_type,r.funding_allocation_id,r.gross_amount_minor,r.currency FROM brokerage_operation_heads h JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id WHERE h.operation_id=?1",params![i.operation_id],|r|Ok(ReplacementHead { revision_id:r.get(0)?, account_id:r.get(1)?, revision:r.get(2)?, operation_type:r.get(3)?, funding_allocation_id:r.get(4)?, gross_amount_minor:r.get(5)?, currency:r.get(6)? })).optional()?;
    let Some(old) = old else {
        return Ok(err(command, "operation_not_found", "operation_id"));
    };
    let old_id = old.revision_id;
    let account = old.account_id;
    let revision = old.revision;
    let old_kind = old.operation_type;
    let old_allocation = old.funding_allocation_id;
    let old_gross = old.gross_amount_minor;
    let old_currency = old.currency;
    let x = i.replacement;
    let mut t = Terms {
        kind: x.operation_type,
        date: x.effective_date,
        currency: x.currency,
        instrument: x.instrument_id,
        quantity: x.quantity,
        gross: x.gross_amount_minor,
        cost: 0,
        result: 0,
        fee: x.fee_minor,
        fee_treatment: x.fee_treatment,
        withholding: x.withholding_minor,
        other: x.other_deductions_minor,
        net: x.net_cash_minor,
        allocation: x.funding_allocation_id,
        destination: x.destination_account_id,
        linked: x.linked_transaction_id,
        component: x.component_id,
    };
    if let Some(quantity) = t.quantity.as_deref() {
        let Some(parsed) = parse_decimal(quantity) else {
            return Ok(err(command, "invalid_quantity", "quantity"));
        };
        t.quantity = Some(format_decimal(parsed));
    }
    if t.kind != old_kind {
        return Ok(err(command, "operation_type_immutable", "operation_type"));
    }
    if t.kind == "deposit"
        && (t.allocation != old_allocation || t.gross != old_gross || t.currency != old_currency)
    {
        return Ok(err(
            command,
            "deposit_funding_immutable",
            "funding_allocation_id",
        ));
    }
    recompute(&tx, &account, &mut t, Some(&i.operation_id))?;
    if let Some(e) = validate(&tx, &account, &t, Some(&i.operation_id))? {
        return Ok(e);
    }
    let id = insert(
        &tx,
        &account,
        &t,
        revision + 1,
        Some(&i.reason),
        Some(&old_id),
    )?;
    tx.execute(
        "UPDATE brokerage_operation_heads SET current_revision_id=?1 WHERE operation_id=?2",
        params![id, i.operation_id],
    )?;
    if state(&tx, &account).is_err() {
        return Ok(err(
            command,
            "correction_breaks_continuity",
            "replacement_json",
        ));
    }
    tx.commit()?;
    inspect(c, command, Some(&account), true)
}

pub fn list(c: &Connection) -> Result<BrokerageResponse> {
    inspect(c, "brokerages list", None, false)
}
pub fn inspect_account(c: &Connection, id: &str) -> Result<BrokerageResponse> {
    inspect(c, "brokerages inspect", Some(id), true)
}

#[derive(Default)]
struct PositionState {
    quantity: i128,
    cost: i64,
    latest: String,
}
#[derive(Default)]
struct CashState {
    available: i64,
    external: i64,
    withdrawals: i64,
    dividends: i64,
    sales: i64,
    result: i64,
    fees: i64,
    withholding: i64,
    other: i64,
    latest: String,
}
#[derive(Default)]
struct State {
    cash: BTreeMap<String, CashState>,
    positions: BTreeMap<(String, String), PositionState>,
}

fn state(c: &Connection, account: &str) -> Result<State> {
    state_from_ops(operations(c, Some(account), true)?)
}

fn state_from_ops(ops: Vec<BrokerageOperation>) -> Result<State> {
    let mut s = State::default();
    for o in ops {
        let cash = s.cash.entry(o.currency.clone()).or_default();
        cash.available = cash
            .available
            .checked_add(o.net_cash_minor)
            .ok_or_else(|| anyhow::anyhow!("cash overflow"))?;
        if cash.available < 0 {
            return Err(anyhow::anyhow!("negative brokerage cash"));
        }
        cash.latest = o.operation_id.clone();
        cash.fees = checked(cash.fees, o.fee_minor)?;
        cash.withholding = checked(cash.withholding, o.withholding_minor)?;
        cash.other = checked(cash.other, o.other_deductions_minor)?;
        match o.operation_type.as_str() {
            "deposit" => cash.external = checked(cash.external, o.gross_amount_minor)?,
            "withdrawal" => cash.withdrawals = checked(cash.withdrawals, o.gross_amount_minor)?,
            "dividend" => cash.dividends = checked(cash.dividends, o.gross_amount_minor)?,
            "sell" => {
                cash.sales = checked(cash.sales, o.gross_amount_minor)?;
                cash.result = checked(cash.result, o.realized_result_minor)?;
            }
            _ => {}
        }
        if let (Some(inst), Some(q)) = (o.instrument_id.clone(), o.quantity.as_deref()) {
            let q = parse_decimal(q).ok_or_else(|| anyhow::anyhow!("invalid stored quantity"))?;
            let p = s.positions.entry((inst, o.currency.clone())).or_default();
            if o.operation_type == "buy" {
                p.quantity = p
                    .quantity
                    .checked_add(q)
                    .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
                p.cost = checked(p.cost, o.historical_cost_minor)?;
            } else if o.operation_type == "sell" {
                let disposed_cost = if q == p.quantity {
                    p.cost
                } else {
                    i64::try_from(
                        (p.cost as i128)
                            .checked_mul(q)
                            .ok_or_else(|| anyhow::anyhow!("cost overflow"))?
                            / p.quantity,
                    )
                    .map_err(|_| anyhow::anyhow!("cost overflow"))?
                };
                p.quantity = p
                    .quantity
                    .checked_sub(q)
                    .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
                p.cost = p
                    .cost
                    .checked_sub(disposed_cost)
                    .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
            }
            p.latest = o.operation_id.clone();
            if p.quantity < 0 || p.cost < 0 {
                return Err(anyhow::anyhow!("negative brokerage position"));
            }
        }
    }
    Ok(s)
}

fn validate(
    c: &Connection,
    account: &str,
    t: &Terms,
    exclude: Option<&str>,
) -> Result<Option<BrokerageResponse>> {
    if !valid_date(&t.date) {
        return Ok(Some(err(
            "brokerages operation",
            "invalid_date",
            "effective_date",
        )));
    }
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM brokerage_accounts WHERE account_id=?1)",
        params![account],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(Some(err(
            "brokerages operation",
            "brokerage_not_found",
            "account_id",
        )));
    }
    if !valid_currency(&t.currency) {
        return Ok(Some(err(
            "brokerages operation",
            "invalid_currency",
            "currency",
        )));
    }
    if t.kind == "buy" || t.kind == "sell" || t.kind == "dividend" {
        let compatible = match t.instrument.as_deref() {
            Some(instrument) => instrument_currency(c, instrument, "security")?
                .is_some_and(|currency| currency == t.currency),
            None => false,
        };
        if !compatible {
            return Ok(Some(err(
                "brokerages operation",
                "instrument_incompatible",
                "instrument_id",
            )));
        }
    }
    if t.kind == "buy" && !matches!(t.fee_treatment.as_deref(), Some("capitalized" | "separate")) {
        return Ok(Some(err(
            "brokerages operation",
            "invalid_fee_treatment",
            "fee_treatment",
        )));
    }
    if (t.fee > 0 || t.withholding > 0 || t.other > 0) && t.component.is_none() {
        return Ok(Some(err(
            "brokerages operation",
            "component_id_required",
            "component_id",
        )));
    }
    if (t.kind == "sell" || t.kind == "dividend")
        && t.gross
            .checked_sub(t.fee)
            .and_then(|x| x.checked_sub(t.withholding))
            .and_then(|x| x.checked_sub(t.other))
            != Some(t.net)
    {
        return Ok(Some(err(
            "brokerages operation",
            "net_reconciliation_failed",
            "net_cash_minor",
        )));
    }
    if t.kind == "withdrawal" {
        if let Some(destination) = &t.destination {
            let owned: bool = c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND is_owned=1 AND currency=?2)",
                params![destination, t.currency],
                |r| r.get(0),
            )?;
            if !owned {
                return Ok(Some(err(
                    "brokerages operation",
                    "destination_account_incompatible",
                    "destination_account_id",
                )));
            }
        }
        if let Some(linked) = &t.linked {
            let exists: bool = c.query_row(
                "SELECT EXISTS(SELECT 1 FROM canonical_transactions WHERE id=?1 AND account_id=?2 AND currency=?3 AND posted_date=?4 AND amount_minor=?5)",
                params![linked, t.destination, t.currency, t.date, t.gross],
                |r| r.get(0),
            )?;
            if !exists {
                return Ok(Some(err(
                    "brokerages operation",
                    "linked_transaction_incompatible",
                    "linked_transaction_id",
                )));
            }
        }
    }
    if t.gross <= 0
        || ((t.kind == "buy" || t.kind == "sell")
            && t.quantity
                .as_deref()
                .and_then(parse_decimal)
                .is_none_or(|q| q <= 0))
    {
        return Ok(Some(err(
            "brokerages operation",
            "invalid_amount",
            "gross_amount_minor",
        )));
    }
    if t.fee < 0 || t.withholding < 0 || t.other < 0 || t.cost < 0 {
        return Ok(Some(err(
            "brokerages operation",
            "negative_component",
            "monetary_components",
        )));
    }
    if let Some(component) = &t.component {
        let used:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM brokerage_operation_revisions r JOIN brokerage_operation_heads h ON h.current_revision_id=r.id WHERE r.component_id=?1 AND (?2 IS NULL OR r.operation_id<>?2))",params![component,exclude],|r|r.get(0))?;
        if used {
            return Ok(Some(err(
                "brokerages operation",
                "component_already_used",
                "component_id",
            )));
        }
        let expense:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM canonical_transactions WHERE investment_fee_component_id=?1)",params![component],|r|r.get(0))?;
        if expense {
            return Ok(Some(err(
                "brokerages operation",
                "component_already_expensed",
                "component_id",
            )));
        }
    }
    if exclude.is_none() {
        let s = state(c, account)?;
        if t.kind == "dividend" {
            let held = s
                .positions
                .get(&(t.instrument.clone().unwrap_or_default(), t.currency.clone()))
                .is_some_and(|position| position.quantity > 0);
            if !held {
                return Ok(Some(err(
                    "brokerages operation",
                    "dividend_without_position",
                    "instrument_id",
                )));
            }
        }
        let available = s.cash.get(&t.currency).map_or(0, |x| x.available);
        if available.checked_add(t.net).is_none_or(|x| x < 0) {
            return Ok(Some(err(
                "brokerages operation",
                "insufficient_cash",
                "net_cash_minor",
            )));
        }
        if t.kind == "sell" {
            let q = t.quantity.as_deref().and_then(parse_decimal).unwrap_or(0);
            let held = s
                .positions
                .get(&(t.instrument.clone().unwrap_or_default(), t.currency.clone()))
                .map_or(0, |p| p.quantity);
            if q > held {
                return Ok(Some(err(
                    "brokerages operation",
                    "insufficient_position",
                    "quantity",
                )));
            }
        }
    }
    let latest:Option<String>=c.query_row("SELECT MAX(r.effective_date) FROM brokerage_operation_heads h JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id WHERE r.account_id=?1 AND (?2 IS NULL OR r.operation_id<>?2)",params![account,exclude],|r|r.get(0))?;
    if exclude.is_none() && latest.as_deref().is_some_and(|d| t.date.as_str() < d) {
        return Ok(Some(err(
            "brokerages operation",
            "operation_out_of_order",
            "effective_date",
        )));
    }
    Ok(None)
}
fn state_before(c: &Connection, account: &str, date: &str, exclude: Option<&str>) -> Result<State> {
    let ops = operations(c, Some(account), true)?
        .into_iter()
        .filter(|operation| {
            operation.effective_date.as_str() <= date
                && Some(operation.operation_id.as_str()) != exclude
        })
        .collect();
    state_from_ops(ops)
}
fn checked(left: i64, right: i64) -> Result<i64> {
    left.checked_add(right)
        .ok_or_else(|| anyhow::anyhow!("minor-unit overflow"))
}

fn recompute(c: &Connection, account: &str, t: &mut Terms, exclude: Option<&str>) -> Result<()> {
    match t.kind.as_str() {
        "deposit" => {
            t.net = t.gross;
            t.cost = 0;
            t.result = 0
        }
        "buy" => {
            let cash_out = t
                .gross
                .checked_add(t.fee)
                .ok_or_else(|| anyhow::anyhow!("overflow"))?;
            t.cost = if t.fee_treatment.as_deref() == Some("separate") {
                t.gross
            } else {
                cash_out
            };
            t.net = -cash_out
        }
        "sell" => {
            let s = state_before(c, account, &t.date, exclude)?;
            let p = s
                .positions
                .get(&(t.instrument.clone().unwrap_or_default(), t.currency.clone()))
                .ok_or_else(|| anyhow::anyhow!("position missing"))?;
            let q = t
                .quantity
                .as_deref()
                .and_then(parse_decimal)
                .ok_or_else(|| anyhow::anyhow!("quantity"))?;
            t.cost = if q == p.quantity {
                p.cost
            } else {
                i64::try_from(
                    (p.cost as i128)
                        .checked_mul(q)
                        .ok_or_else(|| anyhow::anyhow!("cost overflow"))?
                        / p.quantity,
                )
                .map_err(|_| anyhow::anyhow!("cost overflow"))?
            };
            t.result = t
                .gross
                .checked_sub(t.cost)
                .ok_or_else(|| anyhow::anyhow!("realized result overflow"))?
        }
        "dividend" => {}
        "withdrawal" => t.net = -t.gross,
        _ => {}
    }
    Ok(())
}

fn insert(
    tx: &Transaction,
    account: &str,
    t: &Terms,
    revision: i64,
    reason: Option<&str>,
    replaces: Option<&str>,
) -> Result<String> {
    let op = if revision == 1 {
        unique("brop", account)
    } else {
        tx.query_row(
            "SELECT operation_id FROM brokerage_operation_revisions WHERE id=?1",
            params![replaces],
            |r| r.get(0),
        )?
    };
    let id = unique("broprev", &op);
    tx.execute("INSERT INTO brokerage_operation_revisions(id,operation_id,revision,account_id,operation_type,effective_date,currency,instrument_id,quantity,gross_amount_minor,historical_cost_minor,realized_result_minor,fee_minor,fee_treatment,withholding_minor,other_deductions_minor,net_cash_minor,funding_allocation_id,destination_account_id,linked_transaction_id,component_id,provenance_source,correction_reason,replaces_revision_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,'manual_entry',?22,?23)",params![id,op,revision,account,t.kind,t.date,t.currency,t.instrument,t.quantity,t.gross,t.cost,t.result,t.fee,t.fee_treatment,t.withholding,t.other,t.net,t.allocation,t.destination,t.linked,t.component,reason,replaces])?;
    if revision == 1 {
        tx.execute(
            "INSERT INTO brokerage_operation_heads(operation_id,current_revision_id)VALUES(?1,?2)",
            params![op, id],
        )?;
    }
    Ok(id)
}

fn inspect(
    c: &Connection,
    command: &'static str,
    id: Option<&str>,
    history: bool,
) -> Result<BrokerageResponse> {
    let mut stmt=c.prepare("SELECT account_id,opened_date FROM brokerage_accounts WHERE (?1 IS NULL OR account_id=?1) ORDER BY account_id")?;
    let rows = stmt.query_map(params![id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut accounts = Vec::new();
    for row in rows {
        let (account, opened) = row?;
        let s = state(c, &account)?;
        let active = operations(c, Some(&account), true)?;
        accounts.push(BrokerageAccount {
            account_id: account,
            opened_date: opened,
            cash: s
                .cash
                .into_iter()
                .map(|(currency, x)| BrokerageCash {
                    currency,
                    available_minor: x.available,
                    external_capital_minor: x.external,
                    withdrawals_minor: x.withdrawals,
                    gross_dividends_minor: x.dividends,
                    gross_sale_proceeds_minor: x.sales,
                    realized_result_minor: x.result,
                    fees_minor: x.fees,
                    withholding_minor: x.withholding,
                    other_deductions_minor: x.other,
                    latest_operation_id: x.latest,
                })
                .collect(),
            positions: s
                .positions
                .into_iter()
                .filter(|(_, x)| x.quantity != 0)
                .map(|((instrument_id, currency), x)| BrokeragePosition {
                    instrument_id,
                    quantity: format_decimal(x.quantity),
                    historical_cost_minor: x.cost,
                    cost_currency: currency,
                    latest_operation_id: x.latest,
                })
                .collect(),
            active_operations: active,
        });
    }
    if id.is_some() && accounts.is_empty() {
        return Ok(err(command, "brokerage_not_found", "account_id"));
    }
    let operation_history = if history {
        operations(c, id, false)?
    } else {
        Vec::new()
    };
    Ok(BrokerageResponse {
        schema_version: BROKERAGE_SCHEMA_VERSION,
        command,
        ok: true,
        accounts,
        operation_history,
        errors: Vec::new(),
    })
}

fn operations(
    c: &Connection,
    account: Option<&str>,
    active: bool,
) -> Result<Vec<BrokerageOperation>> {
    let base="SELECT r.id,r.operation_id,r.revision,r.operation_type,r.effective_date,r.currency,r.instrument_id,r.quantity,r.gross_amount_minor,r.historical_cost_minor,r.realized_result_minor,r.fee_minor,r.fee_treatment,r.withholding_minor,r.other_deductions_minor,r.net_cash_minor,r.funding_allocation_id,r.destination_account_id,r.linked_transaction_id,r.component_id,r.provenance_source,r.correction_reason,r.replaces_revision_id,r.created_at FROM ";
    let sql = if active {
        format!("{base} brokerage_operation_heads h JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id WHERE (?1 IS NULL OR r.account_id=?1) ORDER BY r.effective_date,(SELECT MIN(rr.rowid) FROM brokerage_operation_revisions rr WHERE rr.operation_id=r.operation_id)")
    } else {
        format!("{base} brokerage_operation_revisions r WHERE (?1 IS NULL OR r.account_id=?1) ORDER BY r.operation_id,r.revision")
    };
    let mut st = c.prepare(&sql)?;
    let rows = st.query_map(params![account], |r| {
        Ok(BrokerageOperation {
            id: r.get(0)?,
            operation_id: r.get(1)?,
            revision: r.get(2)?,
            operation_type: r.get(3)?,
            effective_date: r.get(4)?,
            currency: r.get(5)?,
            instrument_id: r.get(6)?,
            quantity: r.get(7)?,
            gross_amount_minor: r.get(8)?,
            historical_cost_minor: r.get(9)?,
            realized_result_minor: r.get(10)?,
            fee_minor: r.get(11)?,
            fee_treatment: r.get(12)?,
            withholding_minor: r.get(13)?,
            other_deductions_minor: r.get(14)?,
            net_cash_minor: r.get(15)?,
            funding_allocation_id: r.get(16)?,
            destination_account_id: r.get(17)?,
            linked_transaction_id: r.get(18)?,
            component_id: r.get(19)?,
            provenance_source: r.get(20)?,
            correction_reason: r.get(21)?,
            replaces_revision_id: r.get(22)?,
            created_at: r.get(23)?,
        })
    })?;
    let mut result = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if active {
        derive_sale_economics(&mut result)?;
    }
    Ok(result)
}

fn derive_sale_economics(operations: &mut [BrokerageOperation]) -> Result<()> {
    let mut positions: BTreeMap<(String, String), (i128, i64)> = BTreeMap::new();
    for operation in operations {
        let (Some(instrument), Some(quantity)) = (
            operation.instrument_id.clone(),
            operation.quantity.as_deref(),
        ) else {
            continue;
        };
        let quantity =
            parse_decimal(quantity).ok_or_else(|| anyhow::anyhow!("invalid stored quantity"))?;
        let position = positions
            .entry((instrument, operation.currency.clone()))
            .or_default();
        if operation.operation_type == "buy" {
            position.0 = position
                .0
                .checked_add(quantity)
                .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
            position.1 = checked(position.1, operation.historical_cost_minor)?;
        } else if operation.operation_type == "sell" {
            if quantity > position.0 || position.0 <= 0 {
                return Err(anyhow::anyhow!("negative brokerage position"));
            }
            let cost = if quantity == position.0 {
                position.1
            } else {
                i64::try_from(
                    (position.1 as i128)
                        .checked_mul(quantity)
                        .ok_or_else(|| anyhow::anyhow!("cost overflow"))?
                        / position.0,
                )
                .map_err(|_| anyhow::anyhow!("cost overflow"))?
            };
            operation.historical_cost_minor = cost;
            operation.realized_result_minor = operation
                .gross_amount_minor
                .checked_sub(cost)
                .ok_or_else(|| anyhow::anyhow!("result overflow"))?;
            position.0 = position
                .0
                .checked_sub(quantity)
                .ok_or_else(|| anyhow::anyhow!("quantity overflow"))?;
            position.1 = position
                .1
                .checked_sub(cost)
                .ok_or_else(|| anyhow::anyhow!("cost overflow"))?;
        }
    }
    Ok(())
}

fn instrument_currency(c: &Connection, id: &str, kind: &str) -> Result<Option<String>> {
    Ok(c.query_row("SELECT denomination_currency FROM investment_instruments WHERE id=?1 AND instrument_type=?2",params![id,kind],|r|r.get(0)).optional()?)
}
fn parse_decimal(v: &str) -> Option<i128> {
    let v = v.trim();
    if v.is_empty() || v.starts_with('-') || v.starts_with('+') {
        return None;
    }
    let mut parts = v.split('.');
    let whole: i128 = parts.next()?.parse().ok()?;
    let frac = parts.next();
    if parts.next().is_some() {
        return None;
    }
    let fraction = match frac {
        None => 0,
        Some(f) => {
            if f.is_empty() || f.len() > 9 || !f.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let raw: i128 = f.parse().ok()?;
            raw * 10_i128.pow((9 - f.len()) as u32)
        }
    };
    whole.checked_mul(SCALE)?.checked_add(fraction)
}
fn format_decimal(v: i128) -> String {
    let whole = v / SCALE;
    let frac = (v % SCALE).abs();
    if frac == 0 {
        return whole.to_string();
    }
    format!("{whole}.{:09}", frac)
        .trim_end_matches('0')
        .to_string()
}
fn valid_currency(v: &str) -> bool {
    v.len() == 3 && v.bytes().all(|b| b.is_ascii_uppercase())
}
fn valid_date(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, x)| i == 4 || i == 7 || x.is_ascii_digit())
        && v[0..4].parse::<i32>().is_ok_and(|year| {
            let month = v[5..7].parse::<u8>().unwrap_or(0);
            let day = v[8..10].parse::<u8>().unwrap_or(0);
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let max_day = match month {
                2 if leap => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                _ => 0,
            };
            day > 0 && day <= max_day
        })
}
fn unique(prefix: &str, seed: &str) -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}_{:x}", n ^ (seed.len() as u128))
}
fn err(command: &'static str, code: &'static str, path: &'static str) -> BrokerageResponse {
    BrokerageResponse {
        schema_version: BROKERAGE_SCHEMA_VERSION,
        command,
        ok: false,
        accounts: Vec::new(),
        operation_history: Vec::new(),
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
