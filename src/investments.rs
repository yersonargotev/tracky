use crate::storage::ReviewError;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub const INVESTMENTS_SCHEMA_VERSION: &str = "tracky.investments.v1";
const INSTRUMENT_TYPES: &[&str] = &[
    "fiat_currency",
    "dollar_referenced_digital_asset",
    "security",
    "fixed_income",
    "generic",
];

#[derive(Debug, Clone)]
pub struct InstrumentCreateInput {
    pub name: String,
    pub instrument_type: String,
    pub denomination_currency: String,
    pub provider: String,
    pub provider_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvestmentInstrument {
    pub id: String,
    pub name: String,
    pub instrument_type: String,
    pub denomination_currency: String,
    pub provider: String,
    pub provider_identifier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstrumentResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument: Option<InvestmentInstrument>,
    pub instruments: Vec<InvestmentInstrument>,
    pub errors: Vec<ReviewError>,
}

#[derive(Debug, Clone)]
pub struct AllocationInput {
    pub contribution_id: String,
    pub allocations: Vec<AllocationLegInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllocationLegInput {
    pub effective_date: String,
    pub instrument_id: String,
    pub cash_amount_minor: i64,
    pub cash_currency: String,
    pub acquired_quantity: String,
    pub fee_amount_minor: Option<i64>,
    pub fee_currency: Option<String>,
    pub fee_treatment: Option<String>,
    pub fee_component_id: Option<String>,
    pub fee_expense_transaction_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AllocationReplacementInput {
    pub allocation_id: String,
    pub allocation: AllocationLegInput,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EffectiveRate {
    pub cost_minor_numerator: i64,
    pub cost_currency: String,
    pub quantity_denominator: String,
    pub instrument_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvestmentAllocation {
    pub id: String,
    pub allocation_id: String,
    pub revision: i64,
    pub contribution_id: String,
    pub instrument_id: String,
    pub cash_amount_minor: i64,
    pub cash_currency: String,
    pub acquired_quantity: String,
    pub effective_date: Option<String>,
    pub fee_amount_minor: Option<i64>,
    pub fee_currency: Option<String>,
    pub fee_treatment: Option<String>,
    pub fee_component_id: Option<String>,
    pub fee_expense_transaction_id: Option<String>,
    pub effective_rate: Option<EffectiveRate>,
    pub provenance_source: String,
    pub correction_reason: Option<String>,
    pub replaces_revision_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvestmentPosition {
    pub account_id: String,
    pub instrument_id: String,
    pub quantity: String,
    pub accumulated_cost_minor: i64,
    pub cost_currency: String,
    pub latest_contributing_operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvestmentResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_amount_minor: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contribution_currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unallocated_amount_minor: Option<i64>,
    pub allocations: Vec<InvestmentAllocation>,
    pub allocation_history: Vec<InvestmentAllocation>,
    pub positions: Vec<InvestmentPosition>,
    pub errors: Vec<ReviewError>,
}

pub fn allocate_contribution(
    connection: &mut Connection,
    input: AllocationInput,
) -> Result<InvestmentResponse> {
    let command = "investments allocate";
    if input.allocations.is_empty() {
        return Ok(investment_error(
            command,
            "allocation_required",
            "At least one allocation is required.",
            "allocations",
        ));
    }
    let mut validated = Vec::with_capacity(input.allocations.len());
    let mut fee_components = std::collections::HashSet::new();
    let mut fee_expenses = std::collections::HashSet::new();
    for leg in &input.allocations {
        let value = match validate_allocation(connection, command, &input.contribution_id, leg)? {
            Ok(value) => value,
            Err(response) => return Ok(response),
        };
        if let Some(component_id) = value.fee_component_id.as_deref() {
            if !fee_components.insert(component_id.to_owned()) {
                return Ok(investment_error(
                    command,
                    "duplicate_fee_component",
                    "A fee component can appear only once in an allocation action.",
                    "fee_component_id",
                ));
            }
        }
        if let Some(expense_id) = value.fee_expense_transaction_id.as_deref() {
            if !fee_expenses.insert(expense_id.to_owned()) {
                return Ok(investment_error(
                    command,
                    "duplicate_fee_expense",
                    "A canonical expense can represent only one fee component in an allocation action.",
                    "fee_expense_transaction_id",
                ));
            }
        }
        validated.push(value);
    }
    let tx = connection.transaction()?;
    let currently_allocated = active_allocated_minor(&tx, &input.contribution_id, None)?;
    let new_principal = validated.iter().try_fold(0_i64, |sum, allocation| {
        sum.checked_add(allocation.cash_amount_minor)
    });
    if currently_allocated
        .checked_add(new_principal.unwrap_or(i64::MAX))
        .is_none_or(|amount| amount > validated[0].contribution_amount_minor)
    {
        return Ok(investment_error(
            command,
            "contribution_overallocated",
            "Allocation exceeds the contribution's unallocated principal.",
            "cash_amount_minor",
        ));
    }
    for terms in &validated {
        if let Some(response) = validate_fee_component_conflict(&tx, command, terms, None)? {
            return Ok(response);
        }
        let allocation_id = unique_id("alloc", &input.contribution_id);
        let revision_id = unique_id("allocrev", &allocation_id);
        insert_revision(
            &tx,
            &RevisionIdentity {
                revision_id: &revision_id,
                allocation_id: &allocation_id,
                revision: 1,
                contribution_id: &input.contribution_id,
                correction_reason: None,
                replaces_revision_id: None,
            },
            terms,
        )?;
        tx.execute(
            "INSERT INTO investment_allocation_heads (allocation_id, current_revision_id) VALUES (?1, ?2)",
            params![allocation_id, revision_id],
        )?;
    }
    tx.commit()?;
    contribution_response(connection, command, &input.contribution_id)
}

pub fn replace_allocation(
    connection: &mut Connection,
    input: AllocationReplacementInput,
) -> Result<InvestmentResponse> {
    let command = "investments replace-allocation";
    if input.reason.trim().is_empty() {
        return Ok(investment_error(
            command,
            "correction_reason_required",
            "A correction reason is required.",
            "reason",
        ));
    }
    let Some(current) = active_allocation_by_id(connection, &input.allocation_id)? else {
        return Ok(investment_error(
            command,
            "allocation_not_found",
            "Active allocation was not found.",
            "allocation_id",
        ));
    };
    let consumed_by_cdt: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM investment_allocation_consumptions WHERE allocation_id = ?1)",
        params![input.allocation_id],
        |row| row.get(0),
    )?;
    if consumed_by_cdt
        && (current.effective_date.as_deref() != Some(input.allocation.effective_date.as_str())
            || input.allocation.instrument_id != current.instrument_id
            || input.allocation.cash_amount_minor != current.cash_amount_minor
            || !input
                .allocation
                .cash_currency
                .trim()
                .eq_ignore_ascii_case(&current.cash_currency)
            || canonical_exact_decimal(&input.allocation.acquired_quantity, false).as_deref()
                != Some(current.acquired_quantity.as_str()))
    {
        return Ok(investment_error(
            command,
            "allocation_consumed_by_cdt",
            "Instrument, principal, currency, and quantity of CDT funding are immutable after consumption.",
            "allocation_id",
        ));
    }
    let validated = match validate_allocation(
        connection,
        command,
        &current.contribution_id,
        &input.allocation,
    )? {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let tx = connection.transaction()?;
    let other_allocated =
        active_allocated_minor(&tx, &current.contribution_id, Some(&input.allocation_id))?;
    if other_allocated
        .checked_add(validated.cash_amount_minor)
        .is_none_or(|amount| amount > validated.contribution_amount_minor)
    {
        return Ok(investment_error(
            command,
            "contribution_overallocated",
            "Replacement exceeds the contribution's unallocated principal.",
            "cash_amount_minor",
        ));
    }
    let revision = current.revision + 1;
    let revision_id = unique_id("allocrev", &input.allocation_id);
    if let Some(response) =
        validate_fee_component_conflict(&tx, command, &validated, Some(&input.allocation_id))?
    {
        return Ok(response);
    }
    insert_revision(
        &tx,
        &RevisionIdentity {
            revision_id: &revision_id,
            allocation_id: &input.allocation_id,
            revision,
            contribution_id: &current.contribution_id,
            correction_reason: Some(input.reason.trim()),
            replaces_revision_id: Some(&current.id),
        },
        &validated,
    )?;
    tx.execute(
        "UPDATE investment_allocation_heads SET current_revision_id = ?1 WHERE allocation_id = ?2",
        params![revision_id, input.allocation_id],
    )?;
    tx.commit()?;
    contribution_response(connection, command, &current.contribution_id)
}

pub fn inspect_contribution(connection: &Connection, id: &str) -> Result<InvestmentResponse> {
    contribution_response(connection, "investments inspect-contribution", id)
}

pub fn list_positions(
    connection: &Connection,
    account_id: Option<&str>,
) -> Result<InvestmentResponse> {
    let mut statement = connection.prepare(
        "SELECT ct.account_id, r.instrument_id, r.acquired_quantity, r.cash_amount_minor,
                r.cash_currency, r.fee_amount_minor, r.fee_currency, r.fee_treatment, r.id
         FROM investment_allocation_heads h
         JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
         JOIN canonical_transactions ct ON ct.id = r.contribution_transaction_id
         WHERE r.effective_date IS NOT NULL AND (?1 IS NULL OR ct.account_id = ?1)
         ORDER BY r.rowid",
    )?;
    let mut aggregates: BTreeMap<(String, String, String), PositionAggregate> = BTreeMap::new();
    let rows = statement.query_map(params![account_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;
    for row in rows {
        let (account, instrument, quantity, cash, currency, fee, fee_currency, treatment, id) =
            row?;
        let quantity = ExactDecimal::parse(&quantity).expect("persisted quantity is validated");
        let key = (account, instrument, currency.clone());
        let entry = aggregates.entry(key).or_insert_with(|| PositionAggregate {
            quantity: ExactDecimal::zero(),
            cost_minor: 0,
            latest_id: String::new(),
        });
        entry.quantity = entry
            .quantity
            .checked_add(&quantity)
            .ok_or_else(|| anyhow::anyhow!("position quantity exceeds exact decimal limits"))?;
        entry.cost_minor = entry
            .cost_minor
            .checked_add(cash)
            .ok_or_else(|| anyhow::anyhow!("position cost overflow"))?;
        if treatment.as_deref() == Some("capitalized") {
            if fee_currency.as_deref() != Some(currency.as_str()) {
                return Err(anyhow::anyhow!("capitalized fee currency mismatch"));
            }
            entry.cost_minor = entry
                .cost_minor
                .checked_add(fee.unwrap_or(0))
                .ok_or_else(|| anyhow::anyhow!("position cost overflow"))?;
        }
        entry.latest_id = id;
    }
    let positions = aggregates
        .into_iter()
        .map(
            |((account_id, instrument_id, cost_currency), aggregate)| InvestmentPosition {
                account_id,
                instrument_id,
                quantity: aggregate.quantity.canonical(),
                accumulated_cost_minor: aggregate.cost_minor,
                cost_currency,
                latest_contributing_operation_id: aggregate.latest_id,
            },
        )
        .collect();
    Ok(InvestmentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command: "investments positions",
        ok: true,
        contribution_id: None,
        contribution_amount_minor: None,
        contribution_currency: None,
        allocation_status: None,
        unallocated_amount_minor: None,
        allocations: Vec::new(),
        allocation_history: Vec::new(),
        positions,
        errors: Vec::new(),
    })
}

pub fn create_instrument(
    connection: &Connection,
    input: InstrumentCreateInput,
) -> Result<InstrumentResponse> {
    let command = "instruments create";
    let instrument_type = input.instrument_type.trim().to_ascii_lowercase();
    if !INSTRUMENT_TYPES.contains(&instrument_type.as_str()) {
        return Ok(instrument_error(
            command,
            "invalid_instrument_type",
            "Instrument type is not supported.",
            "type",
        ));
    }
    let currency = input.denomination_currency.trim().to_ascii_uppercase();
    if !valid_currency(&currency) {
        return Ok(instrument_error(
            command,
            "invalid_currency",
            "Denomination currency must be a three-letter uppercase code.",
            "denomination_currency",
        ));
    }
    let name = input.name.trim();
    let provider = input.provider.trim();
    if name.is_empty() || provider.is_empty() {
        return Ok(instrument_error(
            command,
            "required_field",
            "Instrument name and provider are required.",
            if name.is_empty() { "name" } else { "provider" },
        ));
    }
    let provider_identifier = input
        .provider_identifier
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let id = instrument_id(
        name,
        &instrument_type,
        &currency,
        provider,
        provider_identifier,
    );
    connection.execute(
        "INSERT OR IGNORE INTO investment_instruments (id, name, instrument_type, denomination_currency, provider, provider_identifier) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, instrument_type, currency, provider, provider_identifier],
    )?;
    let instrument = instrument_by_id(connection, &id)?;
    Ok(InstrumentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command,
        ok: true,
        instrument,
        instruments: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn list_instruments(connection: &Connection) -> Result<InstrumentResponse> {
    let mut statement = connection.prepare(
        "SELECT id, name, instrument_type, denomination_currency, provider, provider_identifier FROM investment_instruments ORDER BY provider, name, id",
    )?;
    let rows = statement.query_map([], instrument_from_row)?;
    Ok(InstrumentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command: "instruments list",
        ok: true,
        instrument: None,
        instruments: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        errors: Vec::new(),
    })
}

pub fn inspect_instrument(connection: &Connection, id: &str) -> Result<InstrumentResponse> {
    let instrument = instrument_by_id(connection, id)?;
    if instrument.is_none() {
        return Ok(instrument_error(
            "instruments inspect",
            "instrument_not_found",
            "Investment instrument was not found.",
            "instrument_id",
        ));
    }
    Ok(InstrumentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command: "instruments inspect",
        ok: true,
        instrument,
        instruments: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn instrument_error(
    command: &'static str,
    code: &'static str,
    message: &'static str,
    path: &'static str,
) -> InstrumentResponse {
    InstrumentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command,
        ok: false,
        instrument: None,
        instruments: Vec::new(),
        errors: vec![ReviewError {
            category: "validation_failure",
            code,
            message: message.to_owned(),
            path,
            recoverable: true,
            details: serde_json::json!({}),
        }],
    }
}

fn instrument_by_id(connection: &Connection, id: &str) -> Result<Option<InvestmentInstrument>> {
    connection
        .query_row(
            "SELECT id, name, instrument_type, denomination_currency, provider, provider_identifier FROM investment_instruments WHERE id = ?1",
            params![id],
            instrument_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn instrument_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<InvestmentInstrument> {
    Ok(InvestmentInstrument {
        id: row.get(0)?,
        name: row.get(1)?,
        instrument_type: row.get(2)?,
        denomination_currency: row.get(3)?,
        provider: row.get(4)?,
        provider_identifier: row.get(5)?,
    })
}

fn instrument_id(
    name: &str,
    instrument_type: &str,
    currency: &str,
    provider: &str,
    provider_identifier: Option<&str>,
) -> String {
    let key = format!(
        "{}|{}|{}|{}|{}",
        instrument_type,
        currency,
        provider.to_ascii_lowercase(),
        provider_identifier.unwrap_or("").to_ascii_lowercase(),
        name.to_ascii_lowercase()
    );
    let hash = Sha256::digest(key.as_bytes());
    format!("instr_{}", hex(&hash[..12]))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

struct ValidatedAllocation {
    contribution_amount_minor: i64,
    instrument_id: String,
    cash_amount_minor: i64,
    cash_currency: String,
    quantity: String,
    effective_date: String,
    fee_amount_minor: Option<i64>,
    fee_currency: Option<String>,
    fee_treatment: Option<String>,
    fee_component_id: Option<String>,
    fee_expense_transaction_id: Option<String>,
}

struct RevisionIdentity<'a> {
    revision_id: &'a str,
    allocation_id: &'a str,
    revision: i64,
    contribution_id: &'a str,
    correction_reason: Option<&'a str>,
    replaces_revision_id: Option<&'a str>,
}

fn validate_allocation(
    connection: &Connection,
    command: &'static str,
    contribution_id: &str,
    input: &AllocationLegInput,
) -> Result<Result<ValidatedAllocation, InvestmentResponse>> {
    let contribution = connection
        .query_row(
            "SELECT amount_minor, currency, transaction_kind, account_id FROM canonical_transactions WHERE id = ?1",
            params![contribution_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((signed_amount, contribution_currency, kind, contribution_account_id)) = contribution
    else {
        return Ok(Err(investment_error(
            command,
            "contribution_not_found",
            "Canonical contribution was not found.",
            "contribution_id",
        )));
    };
    if kind.as_deref() != Some("investment_contribution") || signed_amount >= 0 {
        return Ok(Err(investment_error(
            command,
            "transaction_not_investment_contribution",
            "Only confirmed investment contributions can be allocated.",
            "contribution_id",
        )));
    }
    if instrument_by_id(connection, &input.instrument_id)?.is_none() {
        return Ok(Err(investment_error(
            command,
            "instrument_not_found",
            "Investment instrument was not found.",
            "instrument_id",
        )));
    }
    if input.cash_amount_minor <= 0 {
        return Ok(Err(investment_error(
            command,
            "invalid_cash_amount",
            "Allocated cash principal must be positive.",
            "cash_amount_minor",
        )));
    }
    if input.effective_date.len() != 10
        || chrono::NaiveDate::parse_from_str(&input.effective_date, "%Y-%m-%d").is_err()
    {
        return Ok(Err(investment_error(
            command,
            "invalid_effective_date",
            "Effective date must be a valid YYYY-MM-DD date.",
            "effective_date",
        )));
    }
    let cash_currency = input.cash_currency.trim().to_ascii_uppercase();
    if !valid_currency(&cash_currency) || cash_currency != contribution_currency {
        return Ok(Err(investment_error(
            command,
            "cash_currency_mismatch",
            "Allocated cash currency must match the contribution currency.",
            "cash_currency",
        )));
    }
    let Some(quantity) = ExactDecimal::parse(&input.acquired_quantity) else {
        return Ok(Err(investment_error(
            command,
            "invalid_quantity",
            "Quantity must be a positive plain decimal with at most 38 digits and 18 fractional places.",
            "quantity",
        )));
    };
    let has_any_fee = input.fee_amount_minor.is_some()
        || input.fee_currency.is_some()
        || input.fee_treatment.is_some()
        || input.fee_component_id.is_some()
        || input.fee_expense_transaction_id.is_some();
    let (fee_currency, fee_treatment, fee_component_id, fee_expense_transaction_id) = if has_any_fee
    {
        let Some(fee_amount) = input.fee_amount_minor.filter(|amount| *amount > 0) else {
            return Ok(Err(invalid_fee(command, "fee_amount_minor")));
        };
        let Some(currency) = input
            .fee_currency
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_uppercase)
        else {
            return Ok(Err(invalid_fee(command, "fee_currency")));
        };
        let Some(treatment) = input
            .fee_treatment
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
        else {
            return Ok(Err(invalid_fee(command, "fee_treatment")));
        };
        let Some(component_id) = input
            .fee_component_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(Err(invalid_fee(command, "fee_component_id")));
        };
        if !valid_currency(&currency) || !["capitalized", "separate"].contains(&treatment.as_str())
        {
            return Ok(Err(invalid_fee(command, "fee_treatment")));
        }
        let expense_id = input
            .fee_expense_transaction_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        if treatment == "capitalized" {
            if currency != cash_currency {
                return Ok(Err(investment_error(
                    command,
                    "capitalized_fee_currency_mismatch",
                    "A capitalized fee must use the contribution cost currency.",
                    "fee_currency",
                )));
            }
            if expense_id.is_some() {
                return Ok(Err(investment_error(
                    command,
                    "fee_double_count_conflict",
                    "A capitalized fee cannot link to an expense transaction.",
                    "fee_expense_transaction_id",
                )));
            }
            let linked_expense_exists = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM canonical_transactions WHERE investment_fee_component_id = ?1 AND transaction_kind = 'expense')",
                params![component_id],
                |row| row.get::<_, bool>(0),
            )?;
            if linked_expense_exists {
                return Ok(Err(investment_error(
                    command,
                    "fee_double_count_conflict",
                    "Fee component already belongs to a canonical expense and cannot be capitalized.",
                    "fee_component_id",
                )));
            }
        } else {
            let Some(expense_id) = expense_id else {
                return Ok(Err(invalid_fee(command, "fee_expense_transaction_id")));
            };
            let expense = connection
                .query_row(
                    "SELECT account_id, amount_minor, currency, transaction_kind, investment_fee_component_id FROM canonical_transactions WHERE id = ?1",
                    params![expense_id],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                        ))
                    },
                )
                .optional()?;
            let matches = expense.is_some_and(
                |(account_id, amount, expense_currency, expense_kind, expense_component_id)| {
                    account_id == contribution_account_id
                        && amount.checked_abs() == Some(fee_amount)
                        && amount < 0
                        && expense_currency == currency
                        && expense_kind.as_deref() == Some("expense")
                        && expense_component_id.as_deref() == Some(component_id)
                },
            );
            if !matches {
                return Ok(Err(investment_error(
                    command,
                    "fee_expense_mismatch",
                    "Separate fee must link to a matching canonical expense on the contribution account.",
                    "fee_expense_transaction_id",
                )));
            }
        }
        (
            Some(currency),
            Some(treatment),
            Some(component_id.to_owned()),
            expense_id.map(str::to_owned),
        )
    } else {
        (None, None, None, None)
    };
    let contribution_amount_minor = match signed_amount.checked_abs() {
        Some(value) => value,
        None => {
            return Ok(Err(investment_error(
                command,
                "invalid_contribution_amount",
                "Contribution amount is outside supported exact integer limits.",
                "contribution_id",
            )))
        }
    };
    Ok(Ok(ValidatedAllocation {
        contribution_amount_minor,
        instrument_id: input.instrument_id.clone(),
        cash_amount_minor: input.cash_amount_minor,
        cash_currency,
        quantity: quantity.canonical(),
        effective_date: input.effective_date.clone(),
        fee_amount_minor: input.fee_amount_minor,
        fee_currency,
        fee_treatment,
        fee_component_id,
        fee_expense_transaction_id,
    }))
}

fn invalid_fee(command: &'static str, path: &'static str) -> InvestmentResponse {
    investment_error(
        command,
        "invalid_fee",
        "Fee amount, currency, treatment, component id, and required expense link must form a complete fee component.",
        path,
    )
}

fn validate_fee_component_conflict(
    connection: &Connection,
    command: &'static str,
    allocation: &ValidatedAllocation,
    replacing_allocation_id: Option<&str>,
) -> Result<Option<InvestmentResponse>> {
    let Some(component_id) = allocation.fee_component_id.as_deref() else {
        return Ok(None);
    };
    let existing_component = connection
        .query_row(
            "SELECT allocation_id, fee_amount_minor, fee_currency, fee_treatment, fee_expense_transaction_id
             FROM investment_allocation_revisions
             WHERE fee_component_id = ?1
             ORDER BY rowid LIMIT 1",
            params![component_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?;
    if let Some((allocation_id, amount, currency, treatment, expense_id)) = existing_component {
        let same_immutable_component = replacing_allocation_id == Some(allocation_id.as_str())
            && amount == allocation.fee_amount_minor
            && currency == allocation.fee_currency
            && treatment == allocation.fee_treatment
            && expense_id == allocation.fee_expense_transaction_id;
        if !same_immutable_component {
            return Ok(Some(investment_error(
                command,
                "fee_double_count_conflict",
                "Fee component identity is immutable and cannot be reused with another allocation, treatment, or expense link.",
                "fee_component_id",
            )));
        }
    }
    if let Some(expense_id) = allocation.fee_expense_transaction_id.as_deref() {
        let conflicting_expense = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM investment_allocation_revisions
                WHERE fee_expense_transaction_id = ?1 AND fee_component_id <> ?2
            )",
            params![expense_id, component_id],
            |row| row.get::<_, bool>(0),
        )?;
        if conflicting_expense {
            return Ok(Some(investment_error(
                command,
                "fee_double_count_conflict",
                "Canonical fee expense is already linked to a different durable fee component.",
                "fee_expense_transaction_id",
            )));
        }
    }
    Ok(None)
}

fn insert_revision(
    tx: &Transaction<'_>,
    identity: &RevisionIdentity<'_>,
    allocation: &ValidatedAllocation,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO investment_allocation_revisions (
            id, allocation_id, revision, contribution_transaction_id, instrument_id,
            cash_amount_minor, cash_currency, acquired_quantity, effective_date, fee_amount_minor,
            fee_currency, fee_treatment, fee_component_id, fee_expense_transaction_id,
            provenance_source, correction_reason, replaces_revision_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'manual_entry', ?15, ?16)",
        params![
            identity.revision_id,
            identity.allocation_id,
            identity.revision,
            identity.contribution_id,
            allocation.instrument_id,
            allocation.cash_amount_minor,
            allocation.cash_currency,
            allocation.quantity,
            allocation.effective_date,
            allocation.fee_amount_minor,
            allocation.fee_currency,
            allocation.fee_treatment,
            allocation.fee_component_id,
            allocation.fee_expense_transaction_id,
            identity.correction_reason,
            identity.replaces_revision_id,
        ],
    )?;
    Ok(())
}

fn active_allocated_minor(
    connection: &Connection,
    contribution_id: &str,
    excluding_allocation_id: Option<&str>,
) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT COALESCE(SUM(r.cash_amount_minor), 0)
         FROM investment_allocation_heads h
         JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
         WHERE r.contribution_transaction_id = ?1
           AND (?2 IS NULL OR h.allocation_id <> ?2)",
        params![contribution_id, excluding_allocation_id],
        |row| row.get(0),
    )
}

fn active_allocation_by_id(
    connection: &Connection,
    allocation_id: &str,
) -> Result<Option<InvestmentAllocation>> {
    connection
        .query_row(
            "SELECT r.id, r.allocation_id, r.revision, r.contribution_transaction_id,
                    r.instrument_id, r.cash_amount_minor, r.cash_currency, r.acquired_quantity, r.effective_date,
                    r.fee_amount_minor, r.fee_currency, r.fee_treatment, r.fee_component_id,
                    r.fee_expense_transaction_id, r.provenance_source,
                    r.correction_reason, r.replaces_revision_id, r.created_at
             FROM investment_allocation_heads h
             JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
             WHERE h.allocation_id = ?1",
            params![allocation_id],
            allocation_from_row,
        )
        .optional()
        .map_err(Into::into)
}

fn contribution_response(
    connection: &Connection,
    command: &'static str,
    contribution_id: &str,
) -> Result<InvestmentResponse> {
    let contribution = connection
        .query_row(
            "SELECT amount_minor, currency, transaction_kind FROM canonical_transactions WHERE id = ?1",
            params![contribution_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?)),
        )
        .optional()?;
    let Some((signed_amount, currency, kind)) = contribution else {
        return Ok(investment_error(
            command,
            "contribution_not_found",
            "Canonical contribution was not found.",
            "contribution_id",
        ));
    };
    if kind.as_deref() != Some("investment_contribution") || signed_amount >= 0 {
        return Ok(investment_error(
            command,
            "transaction_not_investment_contribution",
            "Transaction is not an investment contribution.",
            "contribution_id",
        ));
    }
    let amount = signed_amount
        .checked_abs()
        .ok_or_else(|| anyhow::anyhow!("invalid contribution amount"))?;
    let allocations = allocations_for_contribution(connection, contribution_id, true)?;
    let allocation_history = allocations_for_contribution(connection, contribution_id, false)?;
    let allocated = allocations
        .iter()
        .try_fold(0_i64, |sum, allocation| {
            sum.checked_add(allocation.cash_amount_minor)
        })
        .ok_or_else(|| anyhow::anyhow!("allocation total overflow"))?;
    let remaining = amount
        .checked_sub(allocated)
        .ok_or_else(|| anyhow::anyhow!("persisted contribution is overallocated"))?;
    let status = if allocated == 0 {
        "pending_allocation"
    } else if remaining == 0 {
        "fully_allocated"
    } else {
        "partially_allocated"
    };
    Ok(InvestmentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command,
        ok: true,
        contribution_id: Some(contribution_id.to_owned()),
        contribution_amount_minor: Some(amount),
        contribution_currency: Some(currency),
        allocation_status: Some(status.to_owned()),
        unallocated_amount_minor: Some(remaining),
        allocations,
        allocation_history,
        positions: Vec::new(),
        errors: Vec::new(),
    })
}

fn allocations_for_contribution(
    connection: &Connection,
    contribution_id: &str,
    active_only: bool,
) -> Result<Vec<InvestmentAllocation>> {
    let sql = if active_only {
        "SELECT r.id, r.allocation_id, r.revision, r.contribution_transaction_id,
                r.instrument_id, r.cash_amount_minor, r.cash_currency, r.acquired_quantity, r.effective_date,
                r.fee_amount_minor, r.fee_currency, r.fee_treatment, r.fee_component_id,
                r.fee_expense_transaction_id, r.provenance_source,
                r.correction_reason, r.replaces_revision_id, r.created_at
         FROM investment_allocation_heads h
         JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
         WHERE r.contribution_transaction_id = ?1 ORDER BY r.rowid"
    } else {
        "SELECT id, allocation_id, revision, contribution_transaction_id,
                instrument_id, cash_amount_minor, cash_currency, acquired_quantity, effective_date,
                fee_amount_minor, fee_currency, fee_treatment, fee_component_id,
                fee_expense_transaction_id, provenance_source,
                correction_reason, replaces_revision_id, created_at
         FROM investment_allocation_revisions
         WHERE contribution_transaction_id = ?1 ORDER BY allocation_id, revision"
    };
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![contribution_id], allocation_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn allocation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<InvestmentAllocation> {
    let instrument_id: String = row.get(4)?;
    let cash_amount_minor: i64 = row.get(5)?;
    let cash_currency: String = row.get(6)?;
    let acquired_quantity: String = row.get(7)?;
    Ok(InvestmentAllocation {
        id: row.get(0)?,
        allocation_id: row.get(1)?,
        revision: row.get(2)?,
        contribution_id: row.get(3)?,
        instrument_id: instrument_id.clone(),
        cash_amount_minor,
        cash_currency: cash_currency.clone(),
        acquired_quantity: acquired_quantity.clone(),
        effective_date: row.get(8)?,
        fee_amount_minor: row.get(9)?,
        fee_currency: row.get(10)?,
        fee_treatment: row.get(11)?,
        fee_component_id: row.get(12)?,
        fee_expense_transaction_id: row.get(13)?,
        effective_rate: Some(EffectiveRate {
            cost_minor_numerator: cash_amount_minor,
            cost_currency: cash_currency,
            quantity_denominator: acquired_quantity,
            instrument_id,
        }),
        provenance_source: row.get(14)?,
        correction_reason: row.get(15)?,
        replaces_revision_id: row.get(16)?,
        created_at: row.get(17)?,
    })
}

fn investment_error(
    command: &'static str,
    code: &'static str,
    message: &'static str,
    path: &'static str,
) -> InvestmentResponse {
    InvestmentResponse {
        schema_version: INVESTMENTS_SCHEMA_VERSION,
        command,
        ok: false,
        contribution_id: None,
        contribution_amount_minor: None,
        contribution_currency: None,
        allocation_status: None,
        unallocated_amount_minor: None,
        allocations: Vec::new(),
        allocation_history: Vec::new(),
        positions: Vec::new(),
        errors: vec![ReviewError {
            category: "validation_failure",
            code,
            message: message.to_owned(),
            path,
            recoverable: true,
            details: serde_json::json!({}),
        }],
    }
}

fn unique_id(prefix: &str, material: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = Sha256::digest(format!("{prefix}|{material}|{now}").as_bytes());
    format!("{prefix}_{}", hex(&hash[..12]))
}

struct PositionAggregate {
    quantity: ExactDecimal,
    cost_minor: i64,
    latest_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExactDecimal {
    coefficient: i128,
    scale: u32,
}

impl ExactDecimal {
    fn zero() -> Self {
        Self {
            coefficient: 0,
            scale: 0,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::parse_with_zero(value, false)
    }

    fn parse_with_zero(value: &str, allow_zero: bool) -> Option<Self> {
        let value = value.trim();
        if value.is_empty() || value.starts_with(['-', '+']) || value.contains(['e', 'E']) {
            return None;
        }
        let mut parts = value.split('.');
        let integer = parts.next()?;
        let fraction = parts.next().unwrap_or("");
        if parts.next().is_some()
            || integer.is_empty()
            || !integer.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > 18
            || integer.len() + fraction.len() > 38
        {
            return None;
        }
        let digits = format!("{integer}{fraction}");
        let coefficient = digits.parse::<i128>().ok()?;
        if coefficient == 0 && !allow_zero {
            return None;
        }
        Some(
            Self {
                coefficient,
                scale: fraction.len() as u32,
            }
            .normalized(),
        )
    }

    fn normalized(mut self) -> Self {
        while self.scale > 0 && self.coefficient % 10 == 0 {
            self.coefficient /= 10;
            self.scale -= 1;
        }
        self
    }

    fn checked_add(&self, other: &Self) -> Option<Self> {
        let scale = self.scale.max(other.scale);
        let left = self
            .coefficient
            .checked_mul(10_i128.checked_pow(scale - self.scale)?)?;
        let right = other
            .coefficient
            .checked_mul(10_i128.checked_pow(scale - other.scale)?)?;
        Some(
            Self {
                coefficient: left.checked_add(right)?,
                scale,
            }
            .normalized(),
        )
    }

    fn canonical(&self) -> String {
        if self.scale == 0 {
            return self.coefficient.to_string();
        }
        let digits = self.coefficient.to_string();
        let scale = self.scale as usize;
        if digits.len() <= scale {
            format!("0.{}{}", "0".repeat(scale - digits.len()), digits)
        } else {
            let split = digits.len() - scale;
            format!("{}.{}", &digits[..split], &digits[split..])
        }
    }
}

pub(crate) fn canonical_exact_decimal(value: &str, allow_zero: bool) -> Option<String> {
    ExactDecimal::parse_with_zero(value, allow_zero).map(|decimal| decimal.canonical())
}
