use crate::investments::{canonical_exact_decimal, valid_currency};
use crate::storage::{is_valid_posted_date, ReviewError};
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CDT_SCHEMA_VERSION: &str = "tracky.cdts.v1";

#[derive(Debug, Clone, Deserialize)]
pub struct CdtTermsInput {
    pub maturity_date: String,
    pub agreed_rate: Option<String>,
    pub payment_mode: Option<String>,
    pub payment_periodicity: Option<String>,
    pub renewal_terms: Option<String>,
    pub contract_identifier: Option<String>,
    pub allows_partial_redemption: bool,
}

#[derive(Debug, Clone)]
pub struct CdtConstitutionInput {
    pub allocation_id: String,
    pub principal_minor: i64,
    pub currency: String,
    pub constitution_date: String,
    pub terms: CdtTermsInput,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CdtTerms {
    pub maturity_date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agreed_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_periodicity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewal_terms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_identifier: Option<String>,
    pub allows_partial_redemption: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CdtOperationReplacement {
    pub effective_date: String,
    pub principal_before_minor: i64,
    pub principal_after_minor: i64,
    pub principal_returned_minor: i64,
    pub external_capital_minor: i64,
    pub capitalized_interest_minor: i64,
    pub gross_interest_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_received_minor: i64,
    pub funding_allocation_id: Option<String>,
    pub terms: CdtTermsInput,
    pub deduction_component_id: Option<String>,
    pub deduction_expense_transaction_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CdtOperationReplacementInput {
    pub operation_id: String,
    pub reason: String,
    pub replacement: CdtOperationReplacement,
}

#[derive(Debug, Clone)]
pub struct CdtRenewalInput {
    pub position_id: String,
    pub effective_date: String,
    pub additional_allocation_id: Option<String>,
    pub external_capital_minor: i64,
    pub capitalized_interest_minor: i64,
    pub gross_interest_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_received_minor: i64,
    pub deduction_component_id: Option<String>,
    pub deduction_expense_transaction_id: Option<String>,
    pub terms: CdtTermsInput,
}

#[derive(Debug, Clone)]
pub struct CdtRedemptionInput {
    pub position_id: String,
    pub effective_date: String,
    pub principal_returned_minor: i64,
    pub gross_interest_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_received_minor: i64,
    pub deduction_component_id: Option<String>,
    pub deduction_expense_transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CdtOperation {
    pub id: String,
    pub operation_id: String,
    pub revision: i64,
    pub operation_type: String,
    pub effective_date: String,
    pub currency: String,
    pub principal_before_minor: i64,
    pub principal_after_minor: i64,
    pub principal_returned_minor: i64,
    pub external_capital_minor: i64,
    pub capitalized_interest_minor: i64,
    pub gross_interest_minor: i64,
    pub withholding_minor: i64,
    pub other_deductions_minor: i64,
    pub net_cash_received_minor: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_allocation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funding_contribution_id: Option<String>,
    pub terms: CdtTerms,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduction_component_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduction_expense_transaction_id: Option<String>,
    pub provenance_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correction_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaces_revision_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CdtPosition {
    pub id: String,
    pub instrument_id: String,
    pub institution_or_issuer: String,
    pub account_id: String,
    pub constitution_allocation_id: String,
    pub constitution_contribution_id: String,
    pub constitution_date: String,
    pub current_principal_minor: i64,
    pub currency: String,
    pub status: String,
    pub current_terms: CdtTerms,
    pub constituent_operation_id: String,
    pub latest_operation_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CdtResponse {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<CdtPosition>,
    pub positions: Vec<CdtPosition>,
    pub operations: Vec<CdtOperation>,
    pub operation_history: Vec<CdtOperation>,
    pub errors: Vec<ReviewError>,
}

pub fn constitute_cdt(
    connection: &mut Connection,
    input: CdtConstitutionInput,
) -> Result<CdtResponse> {
    let command = "cdts constitute";
    let currency = input.currency.trim().to_ascii_uppercase();
    if input.principal_minor <= 0 {
        return Ok(error(
            command,
            "invalid_principal",
            "Principal must be positive.",
            "principal_minor",
        ));
    }
    if !valid_currency(&currency) {
        return Ok(error(
            command,
            "invalid_currency",
            "Currency must be a three-letter uppercase code.",
            "currency",
        ));
    }
    let terms = match validate_terms(command, &input.constitution_date, input.terms) {
        Ok(terms) => terms,
        Err(response) => return Ok(*response),
    };
    let allocation = connection
        .query_row(
            "SELECT r.contribution_transaction_id, r.instrument_id, r.cash_amount_minor,
                    r.cash_currency, ct.account_id, ii.instrument_type,
                    ii.denomination_currency, ii.provider
             FROM investment_allocation_heads h
             JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
             JOIN canonical_transactions ct ON ct.id = r.contribution_transaction_id
             JOIN investment_instruments ii ON ii.id = r.instrument_id
             WHERE h.allocation_id = ?1",
            params![input.allocation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((
        contribution_id,
        instrument_id,
        allocated_minor,
        allocation_currency,
        account_id,
        instrument_type,
        denomination_currency,
        _provider,
    )) = allocation
    else {
        return Ok(error(
            command,
            "allocation_not_found",
            "Active investment allocation was not found.",
            "allocation_id",
        ));
    };
    let Some(account_id) = account_id else {
        return Ok(error(
            command,
            "allocation_account_required",
            "CDT funding allocation must belong to an account.",
            "allocation_id",
        ));
    };
    if instrument_type != "fixed_income" {
        return Ok(error(
            command,
            "instrument_not_fixed_income",
            "CDTs require a fixed_income instrument.",
            "allocation_id",
        ));
    }
    if currency != allocation_currency || currency != denomination_currency {
        return Ok(error(
            command,
            "currency_mismatch",
            "Principal, allocation, and instrument currencies must match.",
            "currency",
        ));
    }
    if input.principal_minor != allocated_minor {
        return Ok(error(
            command,
            "principal_allocation_mismatch",
            "Principal must exactly match the consumed allocation.",
            "principal_minor",
        ));
    }
    let already_consumed: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM cdt_positions WHERE constituent_allocation_id = ?1)",
        params![input.allocation_id],
        |row| row.get(0),
    )?;
    if already_consumed {
        return Ok(error(
            command,
            "allocation_already_consumed",
            "Allocation already constitutes a CDT position.",
            "allocation_id",
        ));
    }

    let position_id = unique_id("cdtpos", &input.allocation_id);
    let operation_id = unique_id("cdtop", &position_id);
    let revision_id = unique_id("cdtrev", &operation_id);
    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO cdt_positions (id, instrument_id, account_id, constituent_allocation_id)
         VALUES (?1, ?2, ?3, ?4)",
        params![position_id, instrument_id, account_id, input.allocation_id],
    )?;
    if !claim_allocation(
        &tx,
        &input.allocation_id,
        "cdt_constitution",
        &position_id,
        &operation_id,
    )? {
        return Ok(error(
            command,
            "allocation_already_consumed",
            "Allocation is already consumed by a CDT lifecycle.",
            "allocation_id",
        ));
    }
    let operation = CdtOperation {
        id: revision_id.clone(),
        operation_id: operation_id.clone(),
        revision: 1,
        operation_type: "constitution".to_owned(),
        effective_date: input.constitution_date.clone(),
        currency: currency.clone(),
        principal_before_minor: 0,
        principal_after_minor: input.principal_minor,
        principal_returned_minor: 0,
        external_capital_minor: input.principal_minor,
        capitalized_interest_minor: 0,
        gross_interest_minor: 0,
        withholding_minor: 0,
        other_deductions_minor: 0,
        net_cash_received_minor: 0,
        funding_allocation_id: Some(input.allocation_id.clone()),
        funding_contribution_id: Some(contribution_id),
        terms,
        deduction_component_id: None,
        deduction_expense_transaction_id: None,
        provenance_source: "manual_entry".to_owned(),
        correction_reason: None,
        replaces_revision_id: None,
        created_at: String::new(),
    };
    insert_operation(&tx, &revision_id, &position_id, &operation)?;
    tx.execute(
        "INSERT INTO cdt_operation_heads (operation_id, current_revision_id) VALUES (?1, ?2)",
        params![operation_id, revision_id],
    )?;
    tx.commit()?;
    inspect_cdt(connection, &position_id, &input.constitution_date, command)
}

pub fn renew_cdt(connection: &mut Connection, input: CdtRenewalInput) -> Result<CdtResponse> {
    let command = "cdts renew";
    let Some(current) = current_operation(connection, &input.position_id)? else {
        return Ok(error(
            command,
            "cdt_position_not_found",
            "CDT position was not found.",
            "position_id",
        ));
    };
    if current.principal_after_minor == 0 {
        return Ok(error(
            command,
            "cdt_already_redeemed",
            "A redeemed CDT cannot be renewed.",
            "position_id",
        ));
    }
    if !is_valid_posted_date(&input.effective_date)
        || input.effective_date < current.terms.maturity_date
    {
        return Ok(error(
            command,
            "renewal_before_maturity",
            "Renewal date must be on or after current maturity.",
            "effective_date",
        ));
    }
    let terms = match validate_terms(command, &input.effective_date, input.terms) {
        Ok(terms) => terms,
        Err(response) => return Ok(*response),
    };
    if [
        input.external_capital_minor,
        input.capitalized_interest_minor,
        input.gross_interest_minor,
        input.withholding_minor,
        input.other_deductions_minor,
        input.net_cash_received_minor,
    ]
    .iter()
    .any(|amount| *amount < 0)
    {
        return Ok(error(
            command,
            "negative_component",
            "Renewal monetary components cannot be negative.",
            "renewal",
        ));
    }
    let funding = match validate_additional_funding(
        connection,
        command,
        &input.position_id,
        input.additional_allocation_id.as_deref(),
        input.external_capital_minor,
        &current.currency,
    )? {
        Ok(funding) => funding,
        Err(response) => return Ok(response),
    };
    if let Some(response) = validate_interest_reconciliation(
        command,
        input.gross_interest_minor,
        input.capitalized_interest_minor,
        input.withholding_minor,
        input.other_deductions_minor,
        input.net_cash_received_minor,
    ) {
        return Ok(response);
    }
    if let Some(response) = validate_deduction(
        connection,
        command,
        input.other_deductions_minor,
        &current.currency,
        input.deduction_component_id.as_deref(),
        input.deduction_expense_transaction_id.as_deref(),
    )? {
        return Ok(response);
    }
    let Some(principal_after) = current
        .principal_after_minor
        .checked_add(input.external_capital_minor)
        .and_then(|value| value.checked_add(input.capitalized_interest_minor))
    else {
        return Ok(error(
            command,
            "principal_overflow",
            "Renewed principal exceeds exact integer limits.",
            "principal_minor",
        ));
    };
    let operation_id = unique_id("cdtop", &input.position_id);
    let revision_id = unique_id("cdtrev", &operation_id);
    let tx = connection.transaction()?;
    if let Some(allocation_id) = funding.as_deref() {
        if !claim_allocation(
            &tx,
            allocation_id,
            "cdt_additional_capital",
            &input.position_id,
            &operation_id,
        )? {
            return Ok(error(
                command,
                "allocation_already_consumed",
                "Funding allocation is already consumed by a CDT lifecycle.",
                "additional_allocation_id",
            ));
        }
    }
    let operation = CdtOperation {
        id: revision_id.clone(),
        operation_id: operation_id.clone(),
        revision: 1,
        operation_type: "renewal".to_owned(),
        effective_date: input.effective_date.clone(),
        currency: current.currency.clone(),
        principal_before_minor: current.principal_after_minor,
        principal_after_minor: principal_after,
        principal_returned_minor: 0,
        external_capital_minor: input.external_capital_minor,
        capitalized_interest_minor: input.capitalized_interest_minor,
        gross_interest_minor: input.gross_interest_minor,
        withholding_minor: input.withholding_minor,
        other_deductions_minor: input.other_deductions_minor,
        net_cash_received_minor: input.net_cash_received_minor,
        funding_allocation_id: funding,
        funding_contribution_id: None,
        terms,
        deduction_component_id: input.deduction_component_id.clone(),
        deduction_expense_transaction_id: input.deduction_expense_transaction_id.clone(),
        provenance_source: "manual_entry".to_owned(),
        correction_reason: None,
        replaces_revision_id: None,
        created_at: String::new(),
    };
    insert_operation(&tx, &revision_id, &input.position_id, &operation)?;
    tx.execute(
        "INSERT INTO cdt_operation_heads (operation_id, current_revision_id) VALUES (?1, ?2)",
        params![operation_id, revision_id],
    )?;
    tx.commit()?;
    inspect_cdt(
        connection,
        &input.position_id,
        &input.effective_date,
        command,
    )
}

pub fn redeem_cdt(connection: &mut Connection, input: CdtRedemptionInput) -> Result<CdtResponse> {
    let command = "cdts redeem";
    let Some(current) = current_operation(connection, &input.position_id)? else {
        return Ok(error(
            command,
            "cdt_position_not_found",
            "CDT position was not found.",
            "position_id",
        ));
    };
    if current.principal_after_minor == 0 {
        return Ok(error(
            command,
            "duplicate_redemption",
            "The CDT is already fully redeemed.",
            "position_id",
        ));
    }
    if !is_valid_posted_date(&input.effective_date)
        || input.effective_date < current.terms.maturity_date
    {
        return Ok(error(
            command,
            "redemption_before_maturity",
            "Redemption date must be on or after current maturity.",
            "effective_date",
        ));
    }
    if [
        input.gross_interest_minor,
        input.withholding_minor,
        input.other_deductions_minor,
        input.net_cash_received_minor,
    ]
    .iter()
    .any(|amount| *amount < 0)
    {
        return Ok(error(
            command,
            "negative_component",
            "Redemption monetary components cannot be negative.",
            "redemption",
        ));
    }
    let principal_after = match reconcile_redemption(
        current.principal_after_minor,
        input.principal_returned_minor,
        input.gross_interest_minor,
        input.withholding_minor,
        input.other_deductions_minor,
        input.net_cash_received_minor,
    ) {
        Ok(principal_after) => principal_after,
        Err(RedemptionReconciliationError::InvalidPrincipal) => {
            return Ok(error(
                command,
                "invalid_principal_returned",
                "Returned principal must be positive and cannot exceed current principal.",
                "principal_returned_minor",
            ))
        }
        Err(RedemptionReconciliationError::NetCashMismatch) => {
            return Ok(error(command, "net_cash_mismatch", "Net cash must equal returned principal plus gross interest minus withholding and other deductions.", "net_cash_received_minor"))
        }
    };
    if principal_after > 0 && !current.terms.allows_partial_redemption {
        return Ok(error(
            command,
            "partial_redemption_not_allowed",
            "Current contract terms do not allow partial redemption.",
            "principal_returned_minor",
        ));
    }
    if let Some(response) = validate_deduction(
        connection,
        command,
        input.other_deductions_minor,
        &current.currency,
        input.deduction_component_id.as_deref(),
        input.deduction_expense_transaction_id.as_deref(),
    )? {
        return Ok(response);
    }
    let duplicate: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM cdt_operation_heads h
            JOIN cdt_operation_revisions r ON r.id = h.current_revision_id
            WHERE r.cdt_position_id = ?1 AND r.operation_type = 'redemption'
              AND r.effective_date = ?2 AND r.principal_returned_minor = ?3
              AND r.gross_interest_minor = ?4 AND r.withholding_minor = ?5
              AND r.other_deductions_minor = ?6 AND r.net_cash_received_minor = ?7
        )",
        params![
            input.position_id,
            input.effective_date,
            input.principal_returned_minor,
            input.gross_interest_minor,
            input.withholding_minor,
            input.other_deductions_minor,
            input.net_cash_received_minor
        ],
        |row| row.get(0),
    )?;
    if duplicate {
        return Ok(error(
            command,
            "duplicate_redemption",
            "An identical redemption is already recorded.",
            "redemption",
        ));
    }
    let operation_id = unique_id("cdtop", &input.position_id);
    let revision_id = unique_id("cdtrev", &operation_id);
    let tx = connection.transaction()?;
    let operation = CdtOperation {
        id: revision_id.clone(),
        operation_id: operation_id.clone(),
        revision: 1,
        operation_type: "redemption".to_owned(),
        effective_date: input.effective_date.clone(),
        currency: current.currency.clone(),
        principal_before_minor: current.principal_after_minor,
        principal_after_minor: principal_after,
        principal_returned_minor: input.principal_returned_minor,
        external_capital_minor: 0,
        capitalized_interest_minor: 0,
        gross_interest_minor: input.gross_interest_minor,
        withholding_minor: input.withholding_minor,
        other_deductions_minor: input.other_deductions_minor,
        net_cash_received_minor: input.net_cash_received_minor,
        funding_allocation_id: None,
        funding_contribution_id: None,
        terms: current.terms.clone(),
        deduction_component_id: input.deduction_component_id.clone(),
        deduction_expense_transaction_id: input.deduction_expense_transaction_id.clone(),
        provenance_source: "manual_entry".to_owned(),
        correction_reason: None,
        replaces_revision_id: None,
        created_at: String::new(),
    };
    insert_operation(&tx, &revision_id, &input.position_id, &operation)?;
    tx.execute(
        "INSERT INTO cdt_operation_heads (operation_id, current_revision_id) VALUES (?1, ?2)",
        params![operation_id, revision_id],
    )?;
    tx.commit()?;
    inspect_cdt(
        connection,
        &input.position_id,
        &input.effective_date,
        command,
    )
}

pub fn replace_cdt_operation(
    connection: &mut Connection,
    input: CdtOperationReplacementInput,
) -> Result<CdtResponse> {
    let command = "cdts replace-operation";
    if input.reason.trim().is_empty() {
        return Ok(error(
            command,
            "correction_reason_required",
            "A correction reason is required.",
            "reason",
        ));
    }
    let position_id = connection
        .query_row(
            "SELECT r.cdt_position_id FROM cdt_operation_heads h
             JOIN cdt_operation_revisions r ON r.id = h.current_revision_id
             WHERE h.operation_id = ?1",
            params![input.operation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(position_id) = position_id else {
        return Ok(error(
            command,
            "cdt_operation_not_found",
            "Active CDT operation was not found.",
            "operation_id",
        ));
    };
    let mut operations = operations_for_position(connection, &position_id, true)?;
    let Some(index) = operations
        .iter()
        .position(|operation| operation.operation_id == input.operation_id)
    else {
        return Ok(error(
            command,
            "cdt_operation_not_found",
            "Active CDT operation was not found.",
            "operation_id",
        ));
    };
    let current = operations[index].clone();
    let replacement = input.replacement;
    let terms = match validate_terms(command, &replacement.effective_date, replacement.terms) {
        Ok(terms) => terms,
        Err(response) => return Ok(*response),
    };
    if replacement.funding_allocation_id != current.funding_allocation_id
        || replacement.external_capital_minor != current.external_capital_minor
        || replacement.deduction_component_id != current.deduction_component_id
        || replacement.deduction_expense_transaction_id != current.deduction_expense_transaction_id
        || replacement.other_deductions_minor != current.other_deductions_minor
    {
        return Ok(error(
            command,
            "immutable_component_identity",
            "Funding and durable deduction identities, including deduction amount, are immutable across revisions.",
            "replacement",
        ));
    }
    let candidate = CdtOperation {
        id: String::new(),
        operation_id: current.operation_id.clone(),
        revision: current.revision + 1,
        operation_type: current.operation_type.clone(),
        effective_date: replacement.effective_date,
        currency: current.currency.clone(),
        principal_before_minor: replacement.principal_before_minor,
        principal_after_minor: replacement.principal_after_minor,
        principal_returned_minor: replacement.principal_returned_minor,
        external_capital_minor: replacement.external_capital_minor,
        capitalized_interest_minor: replacement.capitalized_interest_minor,
        gross_interest_minor: replacement.gross_interest_minor,
        withholding_minor: replacement.withholding_minor,
        other_deductions_minor: replacement.other_deductions_minor,
        net_cash_received_minor: replacement.net_cash_received_minor,
        funding_allocation_id: replacement.funding_allocation_id,
        funding_contribution_id: current.funding_contribution_id.clone(),
        terms,
        deduction_component_id: replacement.deduction_component_id,
        deduction_expense_transaction_id: replacement.deduction_expense_transaction_id,
        provenance_source: "manual_entry".to_owned(),
        correction_reason: Some(input.reason.trim().to_owned()),
        replaces_revision_id: Some(current.id.clone()),
        created_at: String::new(),
    };
    operations[index] = candidate.clone();
    operations.sort_by(|left, right| left.effective_date.cmp(&right.effective_date));
    if let Some(response) = validate_operation_sequence(command, &operations) {
        return Ok(response);
    }
    let response_as_of = operations
        .last()
        .expect("validated CDT lifecycle is non-empty")
        .effective_date
        .clone();
    let revision_id = unique_id("cdtrev", &input.operation_id);
    let tx = connection.transaction()?;
    insert_operation(&tx, &revision_id, &position_id, &candidate)?;
    tx.execute(
        "UPDATE cdt_operation_heads SET current_revision_id = ?1 WHERE operation_id = ?2",
        params![revision_id, input.operation_id],
    )?;
    tx.commit()?;
    inspect_cdt(connection, &position_id, &response_as_of, command)
}

fn validate_operation_sequence(
    command: &'static str,
    operations: &[CdtOperation],
) -> Option<CdtResponse> {
    let mut principal = 0_i64;
    let mut maturity: Option<&str> = None;
    let mut allows_partial = false;
    for (index, operation) in operations.iter().enumerate() {
        if operation.principal_before_minor < 0
            || operation.principal_after_minor < 0
            || operation.principal_returned_minor < 0
            || operation.external_capital_minor < 0
            || operation.capitalized_interest_minor < 0
            || operation.gross_interest_minor < 0
            || operation.withholding_minor < 0
            || operation.other_deductions_minor < 0
            || operation.net_cash_received_minor < 0
        {
            return Some(error(
                command,
                "negative_component",
                "CDT operation components cannot be negative.",
                "replacement",
            ));
        }
        if index == 0 {
            if operation.operation_type != "constitution"
                || operation.principal_before_minor != 0
                || operation.principal_after_minor <= 0
                || operation.external_capital_minor != operation.principal_after_minor
                || operation.funding_allocation_id.is_none()
            {
                return Some(error(
                    command,
                    "invalid_constitution_sequence",
                    "Lifecycle must begin with one reconciled positive constitution.",
                    "replacement",
                ));
            }
        } else {
            if operation.effective_date.as_str() < maturity.unwrap_or_default()
                || operation.principal_before_minor != principal
                || principal == 0
            {
                return Some(error(
                    command,
                    "inconsistent_lifecycle_sequence",
                    "Replacement must preserve chronological maturity and principal continuity.",
                    "replacement",
                ));
            }
            match operation.operation_type.as_str() {
                "renewal" => {
                    let expected = principal
                        .checked_add(operation.external_capital_minor)
                        .and_then(|value| value.checked_add(operation.capitalized_interest_minor));
                    if expected != Some(operation.principal_after_minor)
                        || validate_interest_reconciliation(
                            command,
                            operation.gross_interest_minor,
                            operation.capitalized_interest_minor,
                            operation.withholding_minor,
                            operation.other_deductions_minor,
                            operation.net_cash_received_minor,
                        )
                        .is_some()
                    {
                        return Some(error(
                            command,
                            "inconsistent_renewal",
                            "Replacement renewal does not reconcile principal or interest cash.",
                            "replacement",
                        ));
                    }
                }
                "redemption" => {
                    let reconciliation = reconcile_redemption(
                        principal,
                        operation.principal_returned_minor,
                        operation.gross_interest_minor,
                        operation.withholding_minor,
                        operation.other_deductions_minor,
                        operation.net_cash_received_minor,
                    );
                    if reconciliation != Ok(operation.principal_after_minor)
                        || (operation.principal_after_minor > 0 && !allows_partial)
                    {
                        return Some(error(
                            command,
                            "inconsistent_redemption",
                            "Replacement redemption does not reconcile principal, contract policy, or net cash.",
                            "replacement",
                        ));
                    }
                }
                _ => {
                    return Some(error(
                        command,
                        "invalid_operation_sequence",
                        "Constitution can appear only once at the beginning.",
                        "replacement",
                    ))
                }
            }
        }
        principal = operation.principal_after_minor;
        maturity = Some(operation.terms.maturity_date.as_str());
        allows_partial = operation.terms.allows_partial_redemption;
    }
    None
}

fn current_operation(connection: &Connection, position_id: &str) -> Result<Option<CdtOperation>> {
    Ok(operations_for_position(connection, position_id, true)?.pop())
}

fn claim_allocation(
    connection: &Connection,
    allocation_id: &str,
    consumer_kind: &str,
    position_id: &str,
    operation_id: &str,
) -> rusqlite::Result<bool> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO investment_allocation_consumptions (
            allocation_id, consumer_kind, cdt_position_id, consumer_operation_id
         ) VALUES (?1, ?2, ?3, ?4)",
        params![allocation_id, consumer_kind, position_id, operation_id],
    )? == 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RedemptionReconciliationError {
    InvalidPrincipal,
    NetCashMismatch,
}

fn reconcile_redemption(
    principal_before: i64,
    principal_returned: i64,
    gross_interest: i64,
    withholding: i64,
    other_deductions: i64,
    net_cash: i64,
) -> std::result::Result<i64, RedemptionReconciliationError> {
    let principal_after = principal_before
        .checked_sub(principal_returned)
        .filter(|value| principal_returned > 0 && *value >= 0)
        .ok_or(RedemptionReconciliationError::InvalidPrincipal)?;
    let expected_cash = principal_returned
        .checked_add(gross_interest)
        .and_then(|value| value.checked_sub(withholding))
        .and_then(|value| value.checked_sub(other_deductions));
    if expected_cash != Some(net_cash) {
        return Err(RedemptionReconciliationError::NetCashMismatch);
    }
    Ok(principal_after)
}

fn validate_interest_reconciliation(
    command: &'static str,
    gross: i64,
    capitalized: i64,
    withholding: i64,
    deductions: i64,
    net_cash: i64,
) -> Option<CdtResponse> {
    let Some(expected) = gross
        .checked_sub(capitalized)
        .and_then(|value| value.checked_sub(withholding))
        .and_then(|value| value.checked_sub(deductions))
    else {
        return Some(error(
            command,
            "net_cash_mismatch",
            "Renewal interest components exceed exact integer limits.",
            "net_cash_received_minor",
        ));
    };
    (expected != net_cash).then(|| error(
        command, "net_cash_mismatch",
        "Renewal net cash must equal gross interest minus capitalized interest, withholding, and other deductions.",
        "net_cash_received_minor",
    ))
}

fn validate_additional_funding(
    connection: &Connection,
    command: &'static str,
    position_id: &str,
    allocation_id: Option<&str>,
    external_capital: i64,
    currency: &str,
) -> Result<Result<Option<String>, CdtResponse>> {
    if external_capital == 0 {
        if allocation_id.is_some() {
            return Ok(Err(error(
                command,
                "unexpected_funding_allocation",
                "A funding allocation requires positive external capital.",
                "additional_allocation_id",
            )));
        }
        return Ok(Ok(None));
    }
    let Some(allocation_id) = allocation_id else {
        return Ok(Err(error(
            command,
            "additional_capital_not_reconciled",
            "Additional principal requires a confirmed allocation.",
            "additional_allocation_id",
        )));
    };
    let expected = connection
        .query_row(
            "SELECT r.instrument_id, r.cash_amount_minor, r.cash_currency, ct.account_id
         FROM investment_allocation_heads h
         JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
         JOIN canonical_transactions ct ON ct.id = r.contribution_transaction_id
         WHERE h.allocation_id = ?1",
            params![allocation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((instrument, amount, allocation_currency, account)) = expected else {
        return Ok(Err(error(
            command,
            "additional_allocation_not_found",
            "Active additional-capital allocation was not found.",
            "additional_allocation_id",
        )));
    };
    let (position_instrument, position_account) = connection.query_row(
        "SELECT instrument_id, account_id FROM cdt_positions WHERE id = ?1",
        params![position_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    if amount != external_capital
        || allocation_currency != currency
        || instrument != position_instrument
        || account.as_deref() != Some(position_account.as_str())
    {
        return Ok(Err(error(command, "additional_capital_mismatch", "Additional allocation must exactly match the CDT instrument, account, currency, and capital.", "additional_allocation_id")));
    }
    let used: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM investment_allocation_consumptions WHERE allocation_id = ?1)",
        params![allocation_id],
        |row| row.get(0),
    )?;
    if used {
        return Ok(Err(error(
            command,
            "allocation_already_consumed",
            "Funding allocation is already consumed by a CDT lifecycle.",
            "additional_allocation_id",
        )));
    }
    Ok(Ok(Some(allocation_id.to_owned())))
}

fn validate_deduction(
    connection: &Connection,
    command: &'static str,
    amount: i64,
    currency: &str,
    component_id: Option<&str>,
    expense_id: Option<&str>,
) -> Result<Option<CdtResponse>> {
    let component_id = component_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let expense_id = expense_id.map(str::trim).filter(|value| !value.is_empty());
    if amount == 0 {
        return Ok((component_id.is_some() || expense_id.is_some()).then(|| {
            error(
                command,
                "unexpected_deduction_identity",
                "Deduction identity requires a positive other deduction.",
                "deduction_component_id",
            )
        }));
    }
    let Some(component_id) = component_id else {
        return Ok(Some(error(
            command,
            "deduction_component_required",
            "Other deductions require a durable component identity.",
            "deduction_component_id",
        )));
    };
    let used: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM investment_allocation_revisions WHERE fee_component_id = ?1)
             OR EXISTS(SELECT 1 FROM cdt_operation_revisions WHERE deduction_component_id = ?1)",
        params![component_id],
        |row| row.get(0),
    )?;
    if used {
        return Ok(Some(error(
            command,
            "deduction_double_count_conflict",
            "Deduction component identity is already used.",
            "deduction_component_id",
        )));
    }
    let canonical_expense = connection
        .query_row(
            "SELECT id, amount_minor, currency, transaction_kind FROM canonical_transactions
         WHERE investment_fee_component_id = ?1",
            params![component_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    match (expense_id, canonical_expense) {
        (None, None) => Ok(None),
        (Some(expected_id), Some((id, signed_amount, expense_currency, kind)))
            if id == expected_id && signed_amount == -amount && expense_currency == currency
                && kind.as_deref() == Some("expense") => Ok(None),
        _ => Ok(Some(error(command, "deduction_expense_conflict", "A linked deduction expense must match component, amount, currency, and expense treatment exactly.", "deduction_expense_transaction_id"))),
    }
}

pub fn list_cdts(connection: &Connection, as_of: &str) -> Result<CdtResponse> {
    let command = "cdts list";
    if !is_valid_posted_date(as_of) {
        return Ok(error(
            command,
            "invalid_as_of_date",
            "As-of date must use YYYY-MM-DD.",
            "as_of",
        ));
    }
    let mut statement = connection.prepare(
        "SELECT p.id FROM cdt_positions p
         WHERE EXISTS(
            SELECT 1 FROM cdt_operation_heads h
            JOIN cdt_operation_revisions r ON r.id = h.current_revision_id
            WHERE r.cdt_position_id = p.id AND r.operation_type = 'constitution'
              AND r.effective_date <= ?1
         )
         ORDER BY p.created_at, p.id",
    )?;
    let ids = statement
        .query_map(params![as_of], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let positions = ids
        .iter()
        .map(|id| load_position(connection, id, as_of))
        .collect::<Result<Vec<_>>>()?;
    Ok(CdtResponse {
        schema_version: CDT_SCHEMA_VERSION,
        command,
        ok: true,
        position: None,
        positions,
        operations: Vec::new(),
        operation_history: Vec::new(),
        errors: Vec::new(),
    })
}

pub fn inspect_cdt(
    connection: &Connection,
    position_id: &str,
    as_of: &str,
    command: &'static str,
) -> Result<CdtResponse> {
    if !is_valid_posted_date(as_of) {
        return Ok(error(
            command,
            "invalid_as_of_date",
            "As-of date must use YYYY-MM-DD.",
            "as_of",
        ));
    }
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM cdt_positions WHERE id = ?1)",
        params![position_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(error(
            command,
            "cdt_position_not_found",
            "CDT position was not found.",
            "position_id",
        ));
    }
    let effective_as_of: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM cdt_operation_heads h
            JOIN cdt_operation_revisions r ON r.id = h.current_revision_id
            WHERE r.cdt_position_id = ?1 AND r.operation_type = 'constitution'
              AND r.effective_date <= ?2
        )",
        params![position_id, as_of],
        |row| row.get(0),
    )?;
    if !effective_as_of {
        return Ok(error(
            command,
            "cdt_not_effective_as_of",
            "CDT position was not yet constituted on the as-of date.",
            "as_of",
        ));
    }
    let position = load_position(connection, position_id, as_of)?;
    let operations = operations_for_position(connection, position_id, true)?;
    let operations = operations
        .into_iter()
        .filter(|operation| operation.effective_date.as_str() <= as_of)
        .collect::<Vec<_>>();
    let operation_history = operations_for_position(connection, position_id, false)?;
    Ok(CdtResponse {
        schema_version: CDT_SCHEMA_VERSION,
        command,
        ok: true,
        position: Some(position),
        positions: Vec::new(),
        operations,
        operation_history,
        errors: Vec::new(),
    })
}

fn load_position(connection: &Connection, position_id: &str, as_of: &str) -> Result<CdtPosition> {
    let (instrument_id, provider, account_id, allocation_id, contribution_id) = connection
        .query_row(
            "SELECT p.instrument_id, i.provider, p.account_id, p.constituent_allocation_id,
                r.contribution_transaction_id
         FROM cdt_positions p
         JOIN investment_instruments i ON i.id = p.instrument_id
         JOIN investment_allocation_heads h ON h.allocation_id = p.constituent_allocation_id
         JOIN investment_allocation_revisions r ON r.id = h.current_revision_id
         WHERE p.id = ?1",
            params![position_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )?;
    let operations = operations_for_position(connection, position_id, true)?;
    let operations = operations
        .into_iter()
        .filter(|operation| operation.effective_date.as_str() <= as_of)
        .collect::<Vec<_>>();
    let first = operations
        .first()
        .expect("CDT position has constituent operation");
    let latest = operations
        .last()
        .expect("CDT position has latest operation");
    let status = if latest.operation_type == "redemption" && latest.principal_after_minor == 0 {
        "redeemed"
    } else if as_of >= latest.terms.maturity_date.as_str() {
        "matured"
    } else if latest.operation_type == "redemption" {
        "active"
    } else if latest.operation_type == "renewal" {
        "renewed"
    } else {
        "active"
    };
    Ok(CdtPosition {
        id: position_id.to_owned(),
        instrument_id,
        institution_or_issuer: provider,
        account_id,
        constitution_allocation_id: allocation_id,
        constitution_contribution_id: contribution_id,
        constitution_date: first.effective_date.clone(),
        current_principal_minor: latest.principal_after_minor,
        currency: latest.currency.clone(),
        status: status.to_owned(),
        current_terms: latest.terms.clone(),
        constituent_operation_id: first.operation_id.clone(),
        latest_operation_id: latest.operation_id.clone(),
    })
}

fn operations_for_position(
    connection: &Connection,
    position_id: &str,
    active_only: bool,
) -> Result<Vec<CdtOperation>> {
    let sql = if active_only {
        format!(
            "{} FROM cdt_operation_heads h
             JOIN cdt_operation_revisions r ON r.id = h.current_revision_id
             WHERE r.cdt_position_id = ?1 ORDER BY r.effective_date, r.rowid",
            operation_select_columns("r")
        )
    } else {
        format!(
            "{} FROM cdt_operation_revisions r
             WHERE r.cdt_position_id = ?1 ORDER BY r.operation_id, r.revision",
            operation_select_columns("r")
        )
    };
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![position_id], operation_from_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn operation_select_columns(alias: &str) -> String {
    format!(
        "SELECT {alias}.id, {alias}.operation_id, {alias}.revision, {alias}.operation_type,
                {alias}.effective_date, {alias}.currency, {alias}.principal_before_minor,
                {alias}.principal_after_minor, {alias}.principal_returned_minor,
                {alias}.external_capital_minor, {alias}.capitalized_interest_minor,
                {alias}.gross_interest_minor, {alias}.withholding_minor,
                {alias}.other_deductions_minor, {alias}.net_cash_received_minor,
                {alias}.funding_allocation_id,
                (SELECT ar.contribution_transaction_id FROM investment_allocation_heads ah
                 JOIN investment_allocation_revisions ar ON ar.id = ah.current_revision_id
                 WHERE ah.allocation_id = {alias}.funding_allocation_id),
                {alias}.maturity_date, {alias}.agreed_rate, {alias}.payment_mode,
                {alias}.payment_periodicity, {alias}.renewal_terms, {alias}.contract_identifier,
                {alias}.allows_partial_redemption, {alias}.deduction_component_id,
                {alias}.deduction_expense_transaction_id, {alias}.provenance_source,
                {alias}.correction_reason, {alias}.replaces_revision_id, {alias}.created_at"
    )
}

fn operation_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CdtOperation> {
    Ok(CdtOperation {
        id: row.get(0)?,
        operation_id: row.get(1)?,
        revision: row.get(2)?,
        operation_type: row.get(3)?,
        effective_date: row.get(4)?,
        currency: row.get(5)?,
        principal_before_minor: row.get(6)?,
        principal_after_minor: row.get(7)?,
        principal_returned_minor: row.get(8)?,
        external_capital_minor: row.get(9)?,
        capitalized_interest_minor: row.get(10)?,
        gross_interest_minor: row.get(11)?,
        withholding_minor: row.get(12)?,
        other_deductions_minor: row.get(13)?,
        net_cash_received_minor: row.get(14)?,
        funding_allocation_id: row.get(15)?,
        funding_contribution_id: row.get(16)?,
        terms: CdtTerms {
            maturity_date: row.get(17)?,
            agreed_rate: row.get(18)?,
            payment_mode: row.get(19)?,
            payment_periodicity: row.get(20)?,
            renewal_terms: row.get(21)?,
            contract_identifier: row.get(22)?,
            allows_partial_redemption: row.get(23)?,
        },
        deduction_component_id: row.get(24)?,
        deduction_expense_transaction_id: row.get(25)?,
        provenance_source: row.get(26)?,
        correction_reason: row.get(27)?,
        replaces_revision_id: row.get(28)?,
        created_at: row.get(29)?,
    })
}

fn insert_operation(
    connection: &Connection,
    revision_id: &str,
    position_id: &str,
    operation: &CdtOperation,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO cdt_operation_revisions (
            id, operation_id, revision, cdt_position_id, operation_type, effective_date, currency,
            principal_before_minor, principal_after_minor, principal_returned_minor,
            external_capital_minor, capitalized_interest_minor, gross_interest_minor,
            withholding_minor, other_deductions_minor, net_cash_received_minor,
            funding_allocation_id, maturity_date, agreed_rate, payment_mode, payment_periodicity,
            renewal_terms, contract_identifier, allows_partial_redemption, deduction_component_id,
            deduction_expense_transaction_id, provenance_source, correction_reason, replaces_revision_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                   ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, 'manual_entry', ?27, ?28)",
        params![revision_id, operation.operation_id, operation.revision, position_id,
            operation.operation_type, operation.effective_date, operation.currency,
            operation.principal_before_minor, operation.principal_after_minor,
            operation.principal_returned_minor, operation.external_capital_minor,
            operation.capitalized_interest_minor, operation.gross_interest_minor,
            operation.withholding_minor, operation.other_deductions_minor,
            operation.net_cash_received_minor, operation.funding_allocation_id,
            operation.terms.maturity_date, operation.terms.agreed_rate,
            operation.terms.payment_mode, operation.terms.payment_periodicity,
            operation.terms.renewal_terms, operation.terms.contract_identifier,
            operation.terms.allows_partial_redemption, operation.deduction_component_id,
            operation.deduction_expense_transaction_id, operation.correction_reason,
            operation.replaces_revision_id],
    )?;
    Ok(())
}

fn validate_terms(
    command: &'static str,
    effective_date: &str,
    input: CdtTermsInput,
) -> Result<CdtTerms, Box<CdtResponse>> {
    if !is_valid_posted_date(effective_date) {
        return Err(Box::new(error(
            command,
            "invalid_effective_date",
            "Effective date must use YYYY-MM-DD.",
            "effective_date",
        )));
    }
    if !is_valid_posted_date(&input.maturity_date) || input.maturity_date.as_str() <= effective_date
    {
        return Err(Box::new(error(
            command,
            "invalid_maturity_date",
            "Maturity date must be a valid date after the effective date.",
            "maturity_date",
        )));
    }
    let agreed_rate = match input.agreed_rate {
        Some(rate) => match canonical_exact_decimal(&rate, true) {
            Some(rate) => Some(rate),
            None => return Err(Box::new(error(command, "invalid_agreed_rate", "Agreed rate must be a non-negative plain decimal with at most 38 digits and 18 fractional places.", "agreed_rate"))),
        },
        None => None,
    };
    Ok(CdtTerms {
        maturity_date: input.maturity_date,
        agreed_rate,
        payment_mode: nonempty(input.payment_mode),
        payment_periodicity: nonempty(input.payment_periodicity),
        renewal_terms: nonempty(input.renewal_terms),
        contract_identifier: nonempty(input.contract_identifier),
        allows_partial_redemption: input.allows_partial_redemption,
    })
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}

fn error(
    command: &'static str,
    code: &'static str,
    message: &'static str,
    path: &'static str,
) -> CdtResponse {
    CdtResponse {
        schema_version: CDT_SCHEMA_VERSION,
        command,
        ok: false,
        position: None,
        positions: Vec::new(),
        operations: Vec::new(),
        operation_history: Vec::new(),
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

pub fn cdt_error(
    command: &'static str,
    code: &'static str,
    message: &'static str,
    path: &'static str,
) -> CdtResponse {
    error(command, code, message, path)
}

fn unique_id(prefix: &str, material: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let hash = Sha256::digest(format!("{prefix}|{material}|{now}").as_bytes());
    format!(
        "{prefix}_{}",
        hash[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
