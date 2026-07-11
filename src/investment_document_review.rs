use crate::investment_documents::{
    digest, err, now, ok, AcceptedSnapshotAudit, AcceptedSnapshotPositionAudit, AuditChain,
    AuditTarget, Error, EventType, MatchStatus, ProviderEvent, ReconciliationCandidate,
    ReconciliationKind, Response, ReviewDecision, ReviewStatus,
};
use crate::reconciliation;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

pub fn list(connection: &Connection) -> Result<Response> {
    load(connection, None, "investment-documents list")
}

pub fn inspect_event(connection: &Connection, id: &str) -> Result<Response> {
    let mut response = load(connection, Some(id), "investment-documents inspect-event")?;
    if response.ok {
        response.audit_chain = Some(load_audit_chain(connection, &response.events[0])?);
    }
    Ok(response)
}

fn load_audit_chain(connection: &Connection, event: &ProviderEvent) -> Result<AuditChain> {
    let reconciled_target=match (event.reconciled_kind,event.reconciled_id.as_deref()) {
        (Some(ReconciliationKind::CanonicalTransaction),Some(id))=>connection.query_row("SELECT id,account_id,posted_date,amount_minor,currency FROM canonical_transactions WHERE id=?1",[id],|r|Ok(AuditTarget{kind:ReconciliationKind::CanonicalTransaction,id:r.get(0)?,account_id:r.get(1)?,effective_date:r.get(2)?,amount_minor:r.get(3)?,currency:r.get(4)?})).optional()?,
        (Some(ReconciliationKind::ProviderEvent),Some(id))=>connection.query_row("SELECT id,account_id,provider_effective_date,amount_minor,currency FROM investment_document_events WHERE id=?1",[id],|r|Ok(AuditTarget{kind:ReconciliationKind::ProviderEvent,id:r.get(0)?,account_id:r.get(1)?,effective_date:r.get(2)?,amount_minor:r.get(3)?,currency:r.get(4)?})).optional()?,
        _=>None,
    };
    let accepted_snapshot = if let Some(id) = event.accepted_snapshot_id.as_deref() {
        let mut audit=connection.query_row("SELECT s.id,s.provider_effective_date,(SELECT count(*) FROM investment_snapshot_positions p WHERE p.snapshot_id=s.id),(SELECT count(*) FROM investment_snapshot_baselines b WHERE b.snapshot_id=s.id) FROM investment_snapshots s WHERE s.id=?1",[id],|r|Ok(AcceptedSnapshotAudit{id:r.get(0)?,provider_effective_date:r.get(1)?,position_count:r.get::<_,i64>(2)? as usize,baseline_count:r.get::<_,i64>(3)? as usize,positions:vec![]})).optional()?;
        if let Some(snapshot) = audit.as_mut() {
            let mut statement=connection.prepare("SELECT account_id,instrument_id,quantity,currency,observed_value_minor FROM investment_snapshot_positions WHERE snapshot_id=?1 ORDER BY account_id,instrument_id")?;
            snapshot.positions = statement
                .query_map([id], |r| {
                    Ok(AcceptedSnapshotPositionAudit {
                        account_id: r.get(0)?,
                        instrument_id: r.get(1)?,
                        quantity: r.get(2)?,
                        currency: r.get(3)?,
                        observed_value_minor: r.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
        audit
    } else {
        None
    };
    Ok(AuditChain {
        source_document_id: event.source_document_id.clone(),
        import_batch_id: event.import_batch_id.clone().unwrap_or_default(),
        provenance_id: event.provenance_id.clone().unwrap_or_default(),
        event_account_id: event.account_id.clone(),
        parser_id: event.parser_id.clone(),
        parser_version: event.parser_version.clone(),
        page_number: event.page_number,
        row_index: event.row_index,
        evidence_redaction: event.evidence_redaction.clone(),
        decision: event.decision,
        reconciled_target,
        accepted_snapshot,
    })
}

pub fn reconciliation_candidates(
    connection: &Connection,
    id: &str,
    event_account_id: &str,
    counterpart_account_id: &str,
) -> Result<Response> {
    let mut response = load(connection, Some(id), "investment-documents candidates")?;
    if !response.ok {
        return Ok(response);
    }
    let event = &response.events[0];
    let event_account_compatible: bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM accounts a JOIN institutions i ON i.id=a.institution_id WHERE a.id=?1 AND a.is_owned=1 AND a.currency=?2 AND instr(lower(i.name),?3)>0)",params![event_account_id,event.currency,event.provider.as_str()],|row|row.get(0))?;
    if !event_account_compatible {
        response.candidates.push(ReconciliationCandidate {
            kind: ReconciliationKind::Reconciliation,
            target_kind: None,
            target_id: None,
            status: MatchStatus::Incompatible,
            reason:
                "The selected event account is not an owned account for this provider and currency."
                    .into(),
        });
        return Ok(response);
    }
    let account_compatible: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND is_owned=1 AND currency=?2)",
        params![counterpart_account_id, event.currency],
        |row| row.get(0),
    )?;
    if !account_compatible {
        response.candidates.push(ReconciliationCandidate {
            kind: ReconciliationKind::Reconciliation,
            target_kind: None,
            target_id: None,
            status: MatchStatus::Incompatible,
            reason: "The selected owned counterpart account is missing or uses another currency."
                .into(),
        });
        return Ok(response);
    }
    if event.status != ReviewStatus::PendingReview {
        response.candidates.push(ReconciliationCandidate {
            kind: ReconciliationKind::Reconciliation,
            target_kind: event.reconciled_kind,
            target_id: event.reconciled_id.clone(),
            status: MatchStatus::AlreadyReconciled,
            reason: "The provider event has already received a decision.".into(),
        });
        return Ok(response);
    }
    if event.event_type == EventType::ObservedPosition {
        response.candidates.push(ReconciliationCandidate {
            kind: ReconciliationKind::Reconciliation,
            target_kind: None,
            target_id: None,
            status: MatchStatus::Incompatible,
            reason: "Observed positions are snapshots, not monetary movements.".into(),
        });
        return Ok(response);
    }
    let mut matches = Vec::new();
    if matches!(event.event_type, EventType::Deposit | EventType::Withdrawal) {
        let required_sign = if event.event_type == EventType::Deposit {
            -1
        } else {
            1
        };
        let mut statement=connection.prepare("SELECT id FROM canonical_transactions WHERE account_id=?1 AND posted_date=?2 AND currency=?3 AND abs(amount_minor)=abs(?4) AND ((?5=-1 AND amount_minor<0 AND transaction_kind='investment_contribution') OR (?5=1 AND amount_minor>0 AND transaction_kind='own_account_transfer')) AND (external_reference IS ?6 OR (?6 IS NULL AND external_reference IS NULL)) ORDER BY id")?;
        for target in statement.query_map(
            params![
                counterpart_account_id,
                event.provider_effective_date,
                event.currency,
                event.amount_minor,
                required_sign,
                event.external_reference,
            ],
            |r| r.get::<_, String>(0),
        )? {
            matches.push((ReconciliationKind::CanonicalTransaction, target?));
        }
        let counterpart = if event.event_type == EventType::Deposit {
            "withdrawal"
        } else {
            "deposit"
        };
        let mut statement=connection.prepare("SELECT e.id FROM investment_document_events e JOIN accounts a ON a.id=?7 JOIN institutions i ON i.id=a.institution_id WHERE e.id<>?1 AND e.status='pending_review' AND e.event_type=?2 AND e.provider_effective_date=?3 AND e.currency=?4 AND e.amount_minor=-?5 AND (e.external_reference IS ?6 OR (?6 IS NULL AND e.external_reference IS NULL)) AND a.is_owned=1 AND a.currency=e.currency AND instr(lower(i.name),e.provider)>0 ORDER BY e.id")?;
        for target in statement.query_map(
            params![
                event.id,
                counterpart,
                event.provider_effective_date,
                event.currency,
                event.amount_minor,
                event.external_reference,
                counterpart_account_id,
            ],
            |r| r.get::<_, String>(0),
        )? {
            matches.push((ReconciliationKind::ProviderEvent, target?));
        }
    }
    let status = match matches.len() {
        0 => MatchStatus::Unmatched,
        1 => MatchStatus::UniqueMatch,
        _ => MatchStatus::AmbiguousMatch,
    };
    if matches.is_empty() {
        response.candidates.push(ReconciliationCandidate {
            kind: ReconciliationKind::Reconciliation,
            target_kind: None,
            target_id: None,
            status,
            reason: "No compatible account-direction-semantics-date-amount-currency target exists."
                .into(),
        });
    } else {
        response
            .candidates
            .extend(
                matches
                    .into_iter()
                    .map(|(kind, target)| ReconciliationCandidate {
                        kind: ReconciliationKind::Reconciliation,
                        target_kind: Some(kind),
                        target_id: Some(target),
                        status,
                        reason: "Exact direction-aware compatible target.".into(),
                    }),
            );
    }
    Ok(response)
}

pub fn accept_snapshot(
    connection: &mut Connection,
    id: &str,
    account_id: &str,
    instrument_id: &str,
) -> Result<Response> {
    let command = "investment-documents accept-snapshot";
    let event = load(connection, Some(id), command)?
        .events
        .into_iter()
        .next();
    let Some(event) = event else {
        return Ok(err(
            command,
            Error {
                code: "event_not_found",
                path: "event_id",
                message: "Provider event was not found.".into(),
            },
        ));
    };
    if event.status != ReviewStatus::PendingReview {
        return Ok(err(
            command,
            Error {
                code: "event_not_pending",
                path: "event_id",
                message: "Provider event was already reviewed.".into(),
            },
        ));
    }
    if event.event_type != EventType::ObservedPosition
        || event.quantity.is_none()
        || event.amount_minor.is_none()
        || event.instrument_hint.is_none()
    {
        return Ok(err(
            command,
            Error {
                code: "incomplete_snapshot",
                path: "event",
                message: "Only a complete observed position can become a dated snapshot.".into(),
            },
        ));
    }
    let compatible: bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM accounts a JOIN investment_instruments i ON i.id=?2 WHERE a.id=?1 AND a.is_owned=1 AND a.currency=?3 AND lower(i.provider)=?4 AND i.provider_identifier=?5)",params![account_id,instrument_id,event.currency,event.provider,event.instrument_hint],|r|r.get(0))?;
    if !compatible {
        return Ok(err(
            command,
            Error {
                code: "snapshot_target_incompatible",
                path: "instrument_id",
                message: "Account and instrument are not compatible with the provider observation."
                    .into(),
            },
        ));
    }
    let snapshot_id = format!("snapshot_{}", &digest(&event.fingerprint)[..24]);
    let tx = connection.transaction()?;
    if tx.execute("INSERT INTO investment_snapshots(id,observed_at,provider_effective_date,source,external_reference,provenance_source) VALUES(?1,?2||'T00:00:00Z',?2,?3,?4,'provider_document')",params![snapshot_id,event.provider_effective_date,event.provider,event.id]).is_err() {
        return Ok(err(command,Error{code:"duplicate_snapshot",path:"event_id",message:"This provider observation already has a canonical snapshot.".into()}));
    }
    tx.execute("INSERT INTO investment_snapshot_positions(snapshot_id,account_id,instrument_id,quantity,currency,observed_value_minor,valuation_currency) VALUES(?1,?2,?3,?4,?5,?6,?5)",params![snapshot_id,account_id,instrument_id,event.quantity,event.currency,event.amount_minor])?;
    tx.execute(
        "UPDATE provenance SET investment_snapshot_id=?1 WHERE investment_document_event_id=?2",
        params![snapshot_id, id],
    )?;
    let baseline =
        reconciliation::capture_baseline(&tx, &snapshot_id, &event.provider_effective_date)?;
    if !baseline.ok {
        return Ok(err(
            command,
            Error {
                code: "snapshot_baseline_failed",
                path: "event_id",
                message: "The snapshot baseline could not be captured.".into(),
            },
        ));
    }
    let changed=tx.execute("UPDATE investment_document_events SET status='accepted',decision='accept_snapshot',reconciled_kind='investment_snapshot',reconciled_id=?1,accepted_snapshot_id=?1,account_id=?2,reviewed_at=?3 WHERE id=?4 AND status='pending_review'",params![snapshot_id,account_id,now(),id])?;
    if changed != 1 {
        return Ok(err(
            command,
            Error {
                code: "event_not_pending",
                path: "event_id",
                message: "Provider event was already reviewed.".into(),
            },
        ));
    }
    tx.commit()?;
    inspect_event(connection, id).map(|mut r| {
        r.command = command;
        r
    })
}

fn load(connection: &Connection, id: Option<&str>, command: &'static str) -> Result<Response> {
    let mut s=connection.prepare("SELECT e.id,e.provider,e.parser_id,e.parser_version,e.event_type,e.provider_effective_date,e.currency,e.amount_minor,e.instrument_hint,e.quantity,e.external_reference,e.page_number,e.row_index,e.evidence_redaction,e.fingerprint,e.status,e.decision,e.reconciled_kind,e.reconciled_id,e.source_document_id,e.import_batch_id,p.id,e.accepted_snapshot_id,e.account_id FROM investment_document_events e LEFT JOIN provenance p ON p.investment_document_event_id=e.id WHERE (?1 IS NULL OR e.id=?1) ORDER BY e.created_at,e.id")?;
    let events = s
        .query_map(params![id], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if id.is_some() && events.is_empty() {
        return Ok(err(
            command,
            Error {
                code: "event_not_found",
                path: "event_id",
                message: "Provider event was not found.".into(),
            },
        ));
    }
    Ok(ok(command, None, None, None, events))
}

pub fn reconcile_deposit(
    connection: &mut Connection,
    id: &str,
    event_account_id: &str,
    counterpart_account_id: &str,
    target_kind: ReconciliationKind,
    target_id: &str,
) -> Result<Response> {
    reconcile_movement(
        connection,
        MovementReconciliation {
            id,
            event_account_id,
            counterpart_account_id,
            expected_type: EventType::Deposit,
            decision: ReviewDecision::ReconcileDeposit,
            target_kind,
            target_id,
        },
    )
}

pub fn reconcile_withdrawal(
    connection: &mut Connection,
    id: &str,
    event_account_id: &str,
    counterpart_account_id: &str,
    target_kind: ReconciliationKind,
    target_id: &str,
) -> Result<Response> {
    reconcile_movement(
        connection,
        MovementReconciliation {
            id,
            event_account_id,
            counterpart_account_id,
            expected_type: EventType::Withdrawal,
            decision: ReviewDecision::ReconcileWithdrawal,
            target_kind,
            target_id,
        },
    )
}

struct MovementReconciliation<'a> {
    id: &'a str,
    event_account_id: &'a str,
    counterpart_account_id: &'a str,
    expected_type: EventType,
    decision: ReviewDecision,
    target_kind: ReconciliationKind,
    target_id: &'a str,
}
fn reconcile_movement(
    connection: &mut Connection,
    request: MovementReconciliation<'_>,
) -> Result<Response> {
    let MovementReconciliation {
        id,
        event_account_id,
        counterpart_account_id,
        expected_type,
        decision,
        target_kind,
        target_id,
    } = request;
    let command = match decision {
        ReviewDecision::ReconcileDeposit => "investment-documents reconcile-deposit",
        ReviewDecision::ReconcileWithdrawal => "investment-documents reconcile-withdrawal",
        _ => unreachable!("typed movement reconciliation decision"),
    };
    if !matches!(
        target_kind,
        ReconciliationKind::CanonicalTransaction | ReconciliationKind::ProviderEvent
    ) {
        return Ok(err(
            command,
            Error {
                code: "invalid_reconciliation_kind",
                path: "target",
                message: "A movement target must be a canonical transaction or provider event."
                    .into(),
            },
        ));
    }
    let event = load(connection, Some(id), command)?
        .events
        .into_iter()
        .next();
    if event
        .as_ref()
        .is_none_or(|event| event.event_type != expected_type)
    {
        return Ok(err(
            command,
            Error {
                code: "incompatible_event_type",
                path: "event_id",
                message: "The typed action does not match the provider event semantics.".into(),
            },
        ));
    }
    let candidates =
        reconciliation_candidates(connection, id, event_account_id, counterpart_account_id)?;
    let selected = candidates.candidates.iter().any(|candidate| {
        candidate.status == MatchStatus::UniqueMatch
            && candidate.target_kind == Some(target_kind)
            && candidate.target_id.as_deref() == Some(target_id)
    });
    if !selected {
        return Ok(err(command, Error { code:"reconciliation_mismatch", path:"target_id", message:"The selected target is not the unique account/reference/direction-aware candidate.".into() }));
    }
    let tx = connection.transaction()?;
    if target_kind == ReconciliationKind::ProviderEvent {
        let paired = tx.execute(
            "UPDATE investment_document_events SET status='accepted',decision=?1,reconciled_kind='provider_event',reconciled_id=?2,account_id=?3,reviewed_at=?4 WHERE id=?5 AND status='pending_review'",
            params![decision, id, counterpart_account_id,now(), target_id],
        )?;
        if paired != 1 {
            return Ok(err(
                command,
                Error {
                    code: "reconciliation_already_consumed",
                    path: "target_id",
                    message: "The provider counterpart was already consumed.".into(),
                },
            ));
        }
    }
    let changed = tx.execute(
        "UPDATE investment_document_events SET status='accepted',decision=?1,reconciled_kind=?2,reconciled_id=?3,account_id=?4,reviewed_at=?5 WHERE id=?6 AND status='pending_review'",
        params![decision, target_kind, target_id,event_account_id, now(), id],
    );
    if changed.unwrap_or(0) != 1 {
        return Ok(err(
            command,
            Error {
                code: "reconciliation_already_consumed",
                path: "target_id",
                message: "The event or target was already consumed.".into(),
            },
        ));
    }
    if target_kind == ReconciliationKind::CanonicalTransaction {
        tx.execute("UPDATE provenance SET canonical_transaction_id=?1 WHERE investment_document_event_id=?2", params![target_id,id])?;
    }
    tx.commit()?;
    inspect_event(connection, id).map(|mut response| {
        response.command = command;
        response
    })
}

pub fn reject(connection: &mut Connection, id: &str) -> Result<Response> {
    let command = "investment-documents reject";
    let tx = connection.transaction()?;
    let changed=tx.execute("UPDATE investment_document_events SET status='rejected',decision='reject',reviewed_at=?1 WHERE id=?2 AND status='pending_review'",params![now(),id])?;
    if changed != 1 {
        return Ok(err(
            command,
            Error {
                code: "event_not_pending",
                path: "event_id",
                message: "Provider event does not exist or was already reviewed.".into(),
            },
        ));
    }
    tx.commit()?;
    inspect_event(connection, id).map(|mut response| {
        response.command = command;
        response
    })
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderEvent> {
    Ok(ProviderEvent {
        id: r.get(0)?,
        provider: r.get(1)?,
        parser_id: r.get(2)?,
        parser_version: r.get(3)?,
        event_type: r.get(4)?,
        provider_effective_date: r.get(5)?,
        currency: r.get(6)?,
        amount_minor: r.get(7)?,
        instrument_hint: r.get(8)?,
        quantity: r.get(9)?,
        external_reference: r.get(10)?,
        page_number: r.get::<_, i64>(11)? as usize,
        row_index: r.get::<_, i64>(12)? as usize,
        evidence_redaction: r.get(13)?,
        fingerprint: r.get(14)?,
        status: r.get(15)?,
        decision: r.get(16)?,
        reconciled_kind: r.get(17)?,
        reconciled_id: r.get(18)?,
        source_document_id: r.get(19)?,
        import_batch_id: r.get(20)?,
        provenance_id: r.get(21)?,
        accepted_snapshot_id: r.get(22)?,
        account_id: r.get(23)?,
    })
}
