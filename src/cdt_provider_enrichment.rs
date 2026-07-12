use crate::cdt::{
    self, CdtConstitutionInput, CdtProviderDecision, CdtProviderEnrichmentInput,
    CdtRedemptionInput, CdtRenewalInput, CdtTermsInput,
};
use crate::investment_documents::{self, EventType, ProviderEvent, ReviewStatus};
use anyhow::Result;
use rusqlite::{backup::Backup, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const SCHEMA_VERSION: &str = "tracky.cdt-provider-enrichment.v1";

#[derive(Debug, Serialize)]
pub struct ActionPreview {
    pub action: LifecycleAction,
    pub status: ActionStatus,
    pub required_reviewer_fields: Vec<&'static str>,
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleAction {
    Constitution,
    Renewal,
    Redemption,
    LinkExisting,
}
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    RequiresExplicitData,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub schema_version: &'static str,
    pub command: &'static str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_evidence: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ActionPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<ProviderEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrichment: Option<Value>,
    pub errors: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    LinkExisting {
        operation_revision_id: String,
    },
    Constitution {
        allocation_id: String,
        maturity_date: String,
        agreed_rate: Option<String>,
        payment_mode: Option<String>,
        payment_periodicity: Option<String>,
        renewal_terms: Option<String>,
        contract_identifier: Option<String>,
        allows_partial_redemption: bool,
    },
    Renewal {
        position_id: String,
        additional_allocation_id: Option<String>,
        external_capital_minor: i64,
        capitalized_interest_minor: i64,
        gross_interest_minor: i64,
        withholding_minor: i64,
        other_deductions_minor: i64,
        maturity_date: String,
        agreed_rate: Option<String>,
        payment_mode: Option<String>,
        payment_periodicity: Option<String>,
        renewal_terms: Option<String>,
        contract_identifier: Option<String>,
        allows_partial_redemption: bool,
        deduction_component_id: Option<String>,
        deduction_expense_transaction_id: Option<String>,
    },
    Redemption {
        position_id: String,
        principal_returned_minor: i64,
        gross_interest_minor: i64,
        withholding_minor: i64,
        other_deductions_minor: i64,
        deduction_component_id: Option<String>,
        deduction_expense_transaction_id: Option<String>,
    },
}

fn error(
    command: &'static str,
    code: &'static str,
    path: &'static str,
    message: &'static str,
) -> Response {
    Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: false,
        mode: None,
        event_evidence: None,
        actions: vec![],
        event: None,
        operation: None,
        enrichment: None,
        errors: vec![json!({"code":code,"path":path,"message":message})],
    }
}
fn evidence(event: &ProviderEvent) -> Value {
    json!({"effective_date":event.provider_effective_date,"currency":event.currency,"amount_minor":event.amount_minor})
}

pub fn preview(c: &Connection, id: &str) -> Result<Response> {
    let command = "investment-documents cdt-actions";
    let inspected = investment_documents::inspect_event(c, id)?;
    let Some(event) = inspected.events.into_iter().next() else {
        return Ok(error(
            command,
            "event_not_found",
            "event_id",
            "Provider event was not found.",
        ));
    };
    if event.status != ReviewStatus::PendingReview {
        return Ok(error(
            command,
            "event_not_pending",
            "event_id",
            "Provider event was already reviewed.",
        ));
    }
    let actions = match event.event_type {
        EventType::CdtOpening => vec![
            ActionPreview {
                action: LifecycleAction::Constitution,
                status: ActionStatus::RequiresExplicitData,
                required_reviewer_fields: vec![
                    "allocation_id",
                    "maturity_date",
                    "allows_partial_redemption",
                ],
            },
            ActionPreview {
                action: LifecycleAction::LinkExisting,
                status: ActionStatus::RequiresExplicitData,
                required_reviewer_fields: vec!["operation_revision_id"],
            },
        ],
        EventType::CdtReturn => vec![
            ActionPreview {
                action: LifecycleAction::Renewal,
                status: ActionStatus::RequiresExplicitData,
                required_reviewer_fields: vec![
                    "position_id",
                    "external_capital_minor",
                    "capitalized_interest_minor",
                    "gross_interest_minor",
                    "withholding_minor",
                    "other_deductions_minor",
                    "maturity_date",
                    "allows_partial_redemption",
                ],
            },
            ActionPreview {
                action: LifecycleAction::Redemption,
                status: ActionStatus::RequiresExplicitData,
                required_reviewer_fields: vec![
                    "position_id",
                    "principal_returned_minor",
                    "gross_interest_minor",
                    "withholding_minor",
                    "other_deductions_minor",
                ],
            },
            ActionPreview {
                action: LifecycleAction::LinkExisting,
                status: ActionStatus::RequiresExplicitData,
                required_reviewer_fields: vec!["operation_revision_id"],
            },
        ],
        _ => {
            return Ok(error(
                command,
                "incompatible_event_type",
                "event_id",
                "Only CDT opening or return evidence supports lifecycle enrichment.",
            ))
        }
    };
    Ok(Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: true,
        mode: None,
        event_evidence: Some(evidence(&event)),
        actions,
        event: Some(event),
        operation: None,
        enrichment: None,
        errors: vec![],
    })
}

pub fn parse_request(raw: &str) -> std::result::Result<Request, Box<Response>> {
    serde_json::from_str(raw).map_err(|_| {
        Box::new(error(
            "investment-documents enrich-cdt",
            "incomplete_reviewer_terms",
            "request_json",
            "The typed request is invalid or omits required reviewer terms.",
        ))
    })
}

pub fn enrich(c: &mut Connection, id: &str, request: Request, dry_run: bool) -> Result<Response> {
    if dry_run {
        let mut memory = Connection::open_in_memory()?;
        {
            let backup = Backup::new(c, &mut memory)?;
            backup.run_to_completion(64, std::time::Duration::from_millis(1), None)?;
        }
        return enrich(&mut memory, id, request, false).map(|mut r| {
            r.mode = Some("dry_run");
            r.event = None;
            r.enrichment = None;
            r
        });
    }
    let command = "investment-documents enrich-cdt";
    let previewed = preview(c, id)?;
    if !previewed.ok {
        let mut r = previewed;
        r.command = command;
        return Ok(r);
    }
    let event = previewed.event.expect("successful preview has event");
    let compatible = matches!(
        (&request, event.event_type),
        (
            Request::LinkExisting { .. },
            EventType::CdtOpening | EventType::CdtReturn
        ) | (Request::Constitution { .. }, EventType::CdtOpening)
            | (Request::Renewal { .. }, EventType::CdtReturn)
            | (Request::Redemption { .. }, EventType::CdtReturn)
    );
    if !compatible {
        return Ok(error(
            command,
            "incompatible_lifecycle_action",
            "request_json.action",
            "The lifecycle action is incompatible with the provider event semantics.",
        ));
    }
    let provider_evidence = evidence(&event);
    let reviewer_terms = serde_json::to_value(&request)?;
    let target_compatible = match &request {
        Request::Constitution { allocation_id, .. } => c.query_row("SELECT EXISTS(SELECT 1 FROM investment_allocation_heads h JOIN investment_allocation_revisions a ON a.id=h.current_revision_id JOIN investment_instruments i ON i.id=a.instrument_id WHERE h.allocation_id=?1 AND lower(i.provider)=?2)",rusqlite::params![allocation_id,event.provider.as_str()],|r|r.get::<_,bool>(0))?,
        Request::Renewal { position_id, .. } | Request::Redemption { position_id, .. } => c.query_row("SELECT EXISTS(SELECT 1 FROM cdt_positions p JOIN investment_instruments i ON i.id=p.instrument_id WHERE p.id=?1 AND lower(i.provider)=?2 AND (?3 IS NULL OR p.account_id=?3))",rusqlite::params![position_id,event.provider.as_str(),event.account_id],|r|r.get::<_,bool>(0))?,
        Request::LinkExisting { operation_revision_id } => c.query_row("SELECT EXISTS(SELECT 1 FROM cdt_operation_heads h JOIN cdt_operation_revisions r ON r.id=h.current_revision_id JOIN cdt_positions p ON p.id=r.cdt_position_id JOIN investment_instruments i ON i.id=p.instrument_id WHERE r.id=?1 AND lower(i.provider)=?2 AND (?3 IS NULL OR p.account_id=?3))",rusqlite::params![operation_revision_id,event.provider.as_str(),event.account_id],|r|r.get::<_,bool>(0))?,
    };
    if !target_compatible {
        return Ok(error(
            command,
            "cdt_target_incompatible",
            "request_json",
            "Selected allocation, position, or operation is incompatible with provider evidence.",
        ));
    }
    let enrichment = |decision| CdtProviderEnrichmentInput {
        event_id: id.to_owned(),
        reviewer_terms_json: reviewer_terms.to_string(),
        provider_evidence_json: provider_evidence.to_string(),
        decision,
    };
    let cdt_response = match request {
        Request::LinkExisting {
            operation_revision_id,
        } => {
            let stored=c.query_row("SELECT r.operation_type,r.effective_date,r.currency,r.principal_after_minor,r.net_cash_received_minor FROM cdt_operation_heads h JOIN cdt_operation_revisions r ON r.id=h.current_revision_id WHERE r.id=?1",[&operation_revision_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?))).optional()?;
            let Some((operation_type, effective_date, currency, principal_after, net_cash)) =
                stored
            else {
                return Ok(error(
                    command,
                    "cdt_operation_not_found",
                    "request_json.operation_revision_id",
                    "Canonical CDT operation was not found.",
                ));
            };
            let semantics = match event.event_type {
                EventType::CdtOpening => {
                    operation_type == "constitution" && event.amount_minor == Some(principal_after)
                }
                EventType::CdtReturn => {
                    matches!(operation_type.as_str(), "renewal" | "redemption")
                        && event.amount_minor == Some(net_cash)
                }
                _ => false,
            };
            if !semantics
                || effective_date != event.provider_effective_date
                || currency != event.currency
            {
                return Ok(error(command,"existing_operation_incompatible","request_json.operation_revision_id","Existing operation does not exactly match provider event semantics, date, currency, and amount."));
            }
            let decision = match operation_type.as_str() {
                "constitution" => CdtProviderDecision::Constitution,
                "renewal" => CdtProviderDecision::Renewal,
                _ => CdtProviderDecision::Redemption,
            };
            cdt::link_provider_event_to_existing_operation(
                c,
                enrichment(decision),
                &operation_revision_id,
            )?
        }
        Request::Constitution {
            allocation_id,
            maturity_date,
            agreed_rate,
            payment_mode,
            payment_periodicity,
            renewal_terms,
            contract_identifier,
            allows_partial_redemption,
        } => cdt::constitute_cdt(
            c,
            CdtConstitutionInput {
                allocation_id,
                principal_minor: event.amount_minor.unwrap_or(0),
                currency: event.currency.clone(),
                constitution_date: event.provider_effective_date.clone(),
                terms: CdtTermsInput {
                    maturity_date,
                    agreed_rate,
                    payment_mode,
                    payment_periodicity,
                    renewal_terms,
                    contract_identifier,
                    allows_partial_redemption,
                },
                provider_enrichment: Some(enrichment(CdtProviderDecision::Constitution)),
            },
        )?,
        Request::Renewal {
            position_id,
            additional_allocation_id,
            external_capital_minor,
            capitalized_interest_minor,
            gross_interest_minor,
            withholding_minor,
            other_deductions_minor,
            maturity_date,
            agreed_rate,
            payment_mode,
            payment_periodicity,
            renewal_terms,
            contract_identifier,
            allows_partial_redemption,
            deduction_component_id,
            deduction_expense_transaction_id,
        } => cdt::renew_cdt(
            c,
            CdtRenewalInput {
                position_id,
                effective_date: event.provider_effective_date.clone(),
                additional_allocation_id,
                external_capital_minor,
                capitalized_interest_minor,
                gross_interest_minor,
                withholding_minor,
                other_deductions_minor,
                net_cash_received_minor: event.amount_minor.unwrap_or(0),
                deduction_component_id,
                deduction_expense_transaction_id,
                terms: CdtTermsInput {
                    maturity_date,
                    agreed_rate,
                    payment_mode,
                    payment_periodicity,
                    renewal_terms,
                    contract_identifier,
                    allows_partial_redemption,
                },
                provider_enrichment: Some(enrichment(CdtProviderDecision::Renewal)),
            },
        )?,
        Request::Redemption {
            position_id,
            principal_returned_minor,
            gross_interest_minor,
            withholding_minor,
            other_deductions_minor,
            deduction_component_id,
            deduction_expense_transaction_id,
        } => cdt::redeem_cdt(
            c,
            CdtRedemptionInput {
                position_id,
                effective_date: event.provider_effective_date.clone(),
                principal_returned_minor,
                gross_interest_minor,
                withholding_minor,
                other_deductions_minor,
                net_cash_received_minor: event.amount_minor.unwrap_or(0),
                deduction_component_id,
                deduction_expense_transaction_id,
                provider_enrichment: Some(enrichment(CdtProviderDecision::Redemption)),
            },
        )?,
    };
    if !cdt_response.ok {
        return Ok(Response {
            schema_version: SCHEMA_VERSION,
            command,
            ok: false,
            mode: Some("apply"),
            event_evidence: Some(provider_evidence),
            actions: vec![],
            event: Some(event),
            operation: None,
            enrichment: None,
            errors: cdt_response
                .errors
                .into_iter()
                .map(|e| serde_json::to_value(e).unwrap())
                .collect(),
        });
    }
    let reviewed = investment_documents::inspect_event(c, id)?
        .events
        .into_iter()
        .next();
    let operation = cdt_response
        .operations
        .last()
        .map(serde_json::to_value)
        .transpose()?;
    let audit=c.query_row("SELECT provider_evidence_json,reviewer_terms_json FROM cdt_provider_enrichments WHERE event_id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?.map(|(provider_evidence_json,reviewer_terms_json)|json!({"provider_evidence":serde_json::from_str::<Value>(&provider_evidence_json).unwrap(),"reviewer_terms":serde_json::from_str::<Value>(&reviewer_terms_json).unwrap()}));
    Ok(Response {
        schema_version: SCHEMA_VERSION,
        command,
        ok: true,
        mode: Some("apply"),
        event_evidence: Some(provider_evidence),
        actions: vec![],
        event: reviewed,
        operation,
        enrichment: audit,
        errors: vec![],
    })
}
