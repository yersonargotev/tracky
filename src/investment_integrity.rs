use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Broken,
    Invariant,
}
pub(crate) struct Finding {
    pub kind: Kind,
    pub code: &'static str,
    pub entity: &'static str,
    pub count: i64,
}
struct Check {
    kind: Kind,
    code: &'static str,
    entity: &'static str,
    sql: &'static str,
}

pub(crate) fn findings(c: &Connection) -> Result<Vec<Finding>> {
    let checks = [
      Check { kind: Kind::Broken, code:"allocation_reference_missing", entity:"investment_allocation_revisions", sql:"SELECT count(*) FROM investment_allocation_revisions r LEFT JOIN canonical_transactions t ON t.id=r.contribution_transaction_id LEFT JOIN investment_instruments i ON i.id=r.instrument_id WHERE t.id IS NULL OR i.id IS NULL" },
      Check { kind: Kind::Invariant, code:"allocation_contribution_incompatible", entity:"investment_allocation_revisions", sql:"SELECT count(*) FROM investment_allocation_revisions r JOIN canonical_transactions t ON t.id=r.contribution_transaction_id WHERE t.transaction_kind<>'investment_contribution' OR t.currency<>r.cash_currency" },
      Check { kind: Kind::Broken, code:"allocation_head_missing_or_incorrect", entity:"investment_allocation_heads", sql:"SELECT count(*) FROM investment_allocation_heads h LEFT JOIN investment_allocation_revisions r ON r.id=h.current_revision_id WHERE r.id IS NULL OR r.allocation_id<>h.allocation_id" },
      Check { kind: Kind::Invariant, code:"allocation_fee_incompatible", entity:"investment_allocation_revisions", sql:"SELECT count(*) FROM investment_allocation_revisions r LEFT JOIN canonical_transactions e ON e.id=r.fee_expense_transaction_id WHERE (r.fee_treatment='capitalized' AND r.fee_currency<>r.cash_currency) OR (r.fee_treatment='separate' AND (e.id IS NULL OR e.transaction_kind<>'expense' OR e.currency<>r.fee_currency OR e.investment_fee_component_id<>r.fee_component_id))" },
      Check { kind: Kind::Broken, code:"allocation_consumption_orphaned", entity:"investment_allocation_consumptions", sql:"SELECT count(*) FROM investment_allocation_consumptions c LEFT JOIN investment_allocation_heads h ON h.allocation_id=c.allocation_id LEFT JOIN cdt_positions p ON p.id=c.cdt_position_id LEFT JOIN cdt_operation_revisions cr ON cr.operation_id=c.consumer_operation_id LEFT JOIN brokerage_operation_revisions br ON br.operation_id=c.consumer_operation_id WHERE h.allocation_id IS NULL OR (c.consumer_kind LIKE 'cdt_%' AND (p.id IS NULL OR cr.id IS NULL)) OR (c.consumer_kind='brokerage_deposit' AND br.id IS NULL)" },
      Check { kind: Kind::Broken, code:"cdt_position_reference_missing", entity:"cdt_positions", sql:"SELECT count(*) FROM cdt_positions p LEFT JOIN investment_instruments i ON i.id=p.instrument_id LEFT JOIN accounts a ON a.id=p.account_id LEFT JOIN investment_allocation_heads h ON h.allocation_id=p.constituent_allocation_id WHERE i.id IS NULL OR a.id IS NULL OR h.allocation_id IS NULL" },
      Check { kind: Kind::Invariant, code:"cdt_instrument_incompatible", entity:"cdt_positions", sql:"SELECT count(*) FROM cdt_positions p JOIN investment_instruments i ON i.id=p.instrument_id WHERE i.instrument_type<>'fixed_income'" },
      Check { kind: Kind::Broken, code:"cdt_operation_head_missing_or_incorrect", entity:"cdt_operation_heads", sql:"SELECT count(*) FROM cdt_operation_heads h LEFT JOIN cdt_operation_revisions r ON r.id=h.current_revision_id WHERE r.id IS NULL OR r.operation_id<>h.operation_id" },
      Check { kind: Kind::Broken, code:"brokerage_account_reference_missing", entity:"brokerage_accounts", sql:"SELECT count(*) FROM brokerage_accounts b LEFT JOIN accounts a ON a.id=b.account_id WHERE a.id IS NULL" },
      Check { kind: Kind::Broken, code:"brokerage_operation_head_missing_or_incorrect", entity:"brokerage_operation_heads", sql:"SELECT count(*) FROM brokerage_operation_heads h LEFT JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id WHERE r.id IS NULL OR r.operation_id<>h.operation_id" },
      Check { kind: Kind::Invariant, code:"brokerage_operation_incompatible", entity:"brokerage_operation_heads", sql:"SELECT count(*) FROM brokerage_operation_heads h JOIN brokerage_operation_revisions r ON r.id=h.current_revision_id LEFT JOIN investment_instruments i ON i.id=r.instrument_id WHERE (r.operation_type IN ('buy','sell') AND (i.id IS NULL OR i.instrument_type<>'security')) OR (r.operation_type NOT IN ('buy','sell','dividend') AND r.instrument_id IS NOT NULL)" },
      Check { kind: Kind::Broken, code:"snapshot_reference_missing", entity:"investment_snapshot_positions", sql:"SELECT count(*) FROM investment_snapshot_positions p LEFT JOIN investment_snapshots s ON s.id=p.snapshot_id LEFT JOIN accounts a ON a.id=p.account_id LEFT JOIN investment_instruments i ON i.id=p.instrument_id WHERE s.id IS NULL OR a.id IS NULL OR (p.instrument_id IS NOT NULL AND i.id IS NULL)" },
      Check { kind: Kind::Broken, code:"snapshot_baseline_orphaned", entity:"investment_snapshot_baselines", sql:"SELECT count(*) FROM investment_snapshot_baselines b LEFT JOIN investment_snapshots s ON s.id=b.snapshot_id LEFT JOIN accounts a ON a.id=b.account_id LEFT JOIN investment_instruments i ON i.id=b.instrument_id WHERE s.id IS NULL OR a.id IS NULL OR (b.instrument_id IS NOT NULL AND i.id IS NULL)" },
      Check { kind: Kind::Broken, code:"adjustment_reference_missing", entity:"investment_adjustment_revisions", sql:"SELECT count(*) FROM investment_adjustment_revisions r LEFT JOIN investment_snapshots s ON s.id=r.snapshot_id LEFT JOIN accounts a ON a.id=r.account_id LEFT JOIN investment_instruments i ON i.id=r.instrument_id WHERE s.id IS NULL OR a.id IS NULL OR (r.instrument_id IS NOT NULL AND i.id IS NULL)" },
      Check { kind: Kind::Broken, code:"adjustment_head_missing_or_incorrect", entity:"investment_adjustment_heads", sql:"SELECT count(*) FROM investment_adjustment_heads h LEFT JOIN investment_adjustment_revisions r ON r.id=h.current_revision_id WHERE r.id IS NULL OR r.adjustment_id<>h.adjustment_id" },
      Check { kind: Kind::Broken, code:"investment_provenance_orphaned", entity:"provenance", sql:"SELECT count(*) FROM provenance p LEFT JOIN investment_document_events e ON e.id=p.investment_document_event_id LEFT JOIN investment_snapshots s ON s.id=p.investment_snapshot_id LEFT JOIN cdt_operation_revisions c ON c.id=p.cdt_operation_revision_id WHERE (p.investment_document_event_id IS NOT NULL AND e.id IS NULL) OR (p.investment_snapshot_id IS NOT NULL AND s.id IS NULL) OR (p.cdt_operation_revision_id IS NOT NULL AND c.id IS NULL)" },
      Check { kind: Kind::Broken, code:"provider_event_target_missing", entity:"investment_document_events", sql:"SELECT count(*) FROM investment_document_events e LEFT JOIN canonical_transactions t ON e.reconciled_kind='canonical_transaction' AND t.id=e.reconciled_id LEFT JOIN investment_document_events p ON e.reconciled_kind='provider_event' AND p.id=e.reconciled_id LEFT JOIN investment_snapshots s ON e.reconciled_kind='investment_snapshot' AND s.id=e.reconciled_id LEFT JOIN cdt_operation_revisions c ON e.reconciled_kind='cdt_operation' AND c.id=e.reconciled_id WHERE (e.reconciled_kind='canonical_transaction' AND t.id IS NULL) OR (e.reconciled_kind='provider_event' AND p.id IS NULL) OR (e.reconciled_kind='investment_snapshot' AND s.id IS NULL) OR (e.reconciled_kind='cdt_operation' AND c.id IS NULL)" },
      Check { kind: Kind::Broken, code:"cdt_provider_enrichment_link_missing", entity:"cdt_provider_enrichments", sql:"SELECT count(*) FROM cdt_provider_enrichments x LEFT JOIN investment_document_events e ON e.id=x.event_id LEFT JOIN cdt_operation_revisions c ON c.id=x.operation_revision_id LEFT JOIN provenance p ON p.investment_document_event_id=x.event_id WHERE e.id IS NULL OR c.id IS NULL OR p.id IS NULL OR p.cdt_operation_revision_id IS NOT x.operation_revision_id OR e.reconciled_kind<>'cdt_operation' OR e.reconciled_id<>x.operation_revision_id" },
      Check { kind: Kind::Invariant, code:"allocation_consumption_exceeds_available", entity:"investment_allocation_consumptions", sql:"SELECT count(*) FROM investment_allocation_consumptions c JOIN investment_allocation_heads h ON h.allocation_id=c.allocation_id JOIN investment_allocation_revisions a ON a.id=h.current_revision_id LEFT JOIN cdt_operation_heads ch ON ch.operation_id=c.consumer_operation_id LEFT JOIN cdt_operation_revisions cr ON cr.id=ch.current_revision_id LEFT JOIN brokerage_operation_heads bh ON bh.operation_id=c.consumer_operation_id LEFT JOIN brokerage_operation_revisions br ON br.id=bh.current_revision_id WHERE (c.consumer_kind LIKE 'cdt_%' AND cr.external_capital_minor>a.cash_amount_minor) OR (c.consumer_kind='brokerage_deposit' AND br.gross_amount_minor>a.cash_amount_minor)" },
      Check { kind: Kind::Invariant, code:"cdt_duplicate_active_lifecycle", entity:"cdt_operation_heads", sql:"SELECT count(*) FROM (SELECT r.cdt_position_id,r.operation_type,count(*) n FROM cdt_operation_heads h JOIN cdt_operation_revisions r ON r.id=h.current_revision_id WHERE r.operation_type IN ('constitution','redemption') GROUP BY r.cdt_position_id,r.operation_type HAVING n>1)" },
      Check { kind: Kind::Broken, code:"cdt_cash_or_funding_link_missing", entity:"cdt_operation_revisions", sql:"SELECT count(*) FROM cdt_operation_revisions r LEFT JOIN investment_allocation_heads a ON a.allocation_id=r.funding_allocation_id LEFT JOIN canonical_transactions e ON e.id=r.deduction_expense_transaction_id WHERE (r.funding_allocation_id IS NOT NULL AND a.allocation_id IS NULL) OR (r.deduction_expense_transaction_id IS NOT NULL AND (e.id IS NULL OR e.transaction_kind<>'expense' OR e.currency<>r.currency))" },
      Check { kind: Kind::Broken, code:"brokerage_cash_or_funding_link_missing", entity:"brokerage_operation_revisions", sql:"SELECT count(*) FROM brokerage_operation_revisions r LEFT JOIN investment_allocation_heads a ON a.allocation_id=r.funding_allocation_id LEFT JOIN accounts d ON d.id=r.destination_account_id LEFT JOIN canonical_transactions t ON t.id=r.linked_transaction_id WHERE (r.funding_allocation_id IS NOT NULL AND a.allocation_id IS NULL) OR (r.destination_account_id IS NOT NULL AND d.id IS NULL) OR (r.linked_transaction_id IS NOT NULL AND t.id IS NULL)" },
      Check { kind: Kind::Invariant, code:"snapshot_source_invalid", entity:"investment_snapshots", sql:"SELECT count(*) FROM investment_snapshots s LEFT JOIN provenance p ON p.investment_snapshot_id=s.id LEFT JOIN investment_document_events e ON e.accepted_snapshot_id=s.id WHERE trim(s.source)='' OR trim(s.provenance_source)='' OR (s.provenance_source<>'manual_entry' AND p.id IS NULL AND e.id IS NULL)" },
      Check { kind: Kind::Invariant, code:"adjustment_snapshot_incompatible", entity:"investment_adjustment_revisions", sql:"SELECT count(*) FROM investment_adjustment_revisions r WHERE NOT EXISTS(SELECT 1 FROM investment_snapshot_baselines b WHERE b.snapshot_id=r.snapshot_id AND b.account_id=r.account_id AND b.instrument_id IS r.instrument_id AND b.currency=r.currency)" },
      Check { kind: Kind::Broken, code:"allocation_active_head_missing", entity:"investment_allocation_revisions", sql:"SELECT count(*) FROM (SELECT DISTINCT allocation_id FROM investment_allocation_revisions) r LEFT JOIN investment_allocation_heads h ON h.allocation_id=r.allocation_id WHERE h.allocation_id IS NULL" },
      Check { kind: Kind::Broken, code:"cdt_operation_active_head_missing", entity:"cdt_operation_revisions", sql:"SELECT count(*) FROM (SELECT DISTINCT operation_id FROM cdt_operation_revisions) r LEFT JOIN cdt_operation_heads h ON h.operation_id=r.operation_id WHERE h.operation_id IS NULL" },
      Check { kind: Kind::Broken, code:"brokerage_operation_active_head_missing", entity:"brokerage_operation_revisions", sql:"SELECT count(*) FROM (SELECT DISTINCT operation_id FROM brokerage_operation_revisions) r LEFT JOIN brokerage_operation_heads h ON h.operation_id=r.operation_id WHERE h.operation_id IS NULL" },
      Check { kind: Kind::Broken, code:"snapshot_account_missing", entity:"investment_snapshots", sql:"SELECT count(*) FROM investment_snapshots s WHERE NOT EXISTS(SELECT 1 FROM investment_snapshot_positions p WHERE p.snapshot_id=s.id)" },
    ];
    let mut out = Vec::new();
    for check in checks {
        let count = c.query_row(check.sql, [], |r| r.get::<_, i64>(0))?;
        if count > 0 {
            out.push(Finding {
                kind: check.kind,
                code: check.code,
                entity: check.entity,
                count,
            });
        }
    }
    validate_exact_quantities(c, &mut out)?;
    validate_checked_totals(c, &mut out)?;
    for (code, entity, replay) in [
        (
            "brokerage_replay_impossible",
            "brokerage_operation_heads",
            crate::brokerage::list(c).map(|_| ()),
        ),
        (
            "cdt_replay_impossible",
            "cdt_operation_heads",
            crate::cdt::list_cdts(c, "9999-12-31").map(|_| ()),
        ),
        (
            "reconciliation_state_impossible",
            "investment_adjustment_heads",
            crate::reconciliation::validate_integrity(c),
        ),
    ] {
        if replay.is_err() {
            out.push(Finding {
                kind: Kind::Invariant,
                code,
                entity,
                count: 1,
            });
        }
    }
    Ok(out)
}

fn validate_exact_quantities(c: &Connection, out: &mut Vec<Finding>) -> Result<()> {
    let mut invalid = 0_i64;
    for (table, column) in [
        ("investment_allocation_revisions", "acquired_quantity"),
        ("brokerage_operation_revisions", "quantity"),
        ("investment_snapshot_positions", "quantity"),
        ("investment_snapshot_positions", "observed_price"),
    ] {
        let mut q = c.prepare(&format!(
            "SELECT {column} FROM {table} WHERE {column} IS NOT NULL"
        ))?;
        for value in q.query_map([], |r| r.get::<_, String>(0))? {
            let value = value?;
            if crate::investments::canonical_exact_decimal(&value, false).as_deref()
                != Some(value.as_str())
            {
                invalid = invalid
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("quantity finding overflow"))?;
            }
        }
    }
    let mut rates =
        c.prepare("SELECT agreed_rate FROM cdt_operation_revisions WHERE agreed_rate IS NOT NULL")?;
    for value in rates.query_map([], |r| r.get::<_, String>(0))? {
        let value = value?;
        if crate::investments::canonical_exact_decimal(&value, true).as_deref()
            != Some(value.as_str())
        {
            invalid = invalid
                .checked_add(1)
                .ok_or_else(|| anyhow!("quantity finding overflow"))?;
        }
    }
    for (table, column) in [
        ("investment_snapshot_baselines", "quantity_difference"),
        ("investment_adjustment_revisions", "quantity_delta"),
    ] {
        let mut query = c.prepare(&format!(
            "SELECT {column} FROM {table} WHERE {column} IS NOT NULL"
        ))?;
        for value in query.query_map([], |r| r.get::<_, String>(0))? {
            let value = value?;
            if crate::reconciliation::signed_decimal(&value).as_deref() != Some(value.as_str()) {
                invalid = invalid
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("quantity finding overflow"))?;
            }
        }
    }
    if invalid > 0 {
        out.push(Finding {
            kind: Kind::Invariant,
            code: "investment_exact_decimal_invalid",
            entity: "investment_quantities",
            count: invalid,
        });
    }
    Ok(())
}

fn validate_checked_totals(c: &Connection, out: &mut Vec<Finding>) -> Result<()> {
    let mut contributions: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    let mut q=c.prepare("SELECT t.id,t.amount_minor,r.cash_amount_minor FROM canonical_transactions t JOIN investment_allocation_revisions r ON r.contribution_transaction_id=t.id JOIN investment_allocation_heads h ON h.current_revision_id=r.id WHERE t.transaction_kind='investment_contribution' ORDER BY t.id,h.allocation_id")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (id, signed_amount, part) = row?;
        let available = match signed_amount.checked_neg() {
            Some(value) => value,
            None => {
                out.push(Finding {
                    kind: Kind::Invariant,
                    code: "contribution_amount_overflow",
                    entity: "canonical_transactions",
                    count: 1,
                });
                continue;
            }
        };
        let x = contributions.entry(id).or_insert((available, 0));
        match x.1.checked_add(part) {
            Some(total) => x.1 = total,
            None => out.push(Finding {
                kind: Kind::Invariant,
                code: "allocation_total_overflow",
                entity: "investment_allocation_heads",
                count: 1,
            }),
        }
    }
    let over = contributions
        .values()
        .filter(|(available, total)| total > available)
        .count() as i64;
    if over > 0 {
        out.push(Finding {
            kind: Kind::Invariant,
            code: "active_allocation_exceeds_contribution",
            entity: "investment_allocation_heads",
            count: over,
        });
    }
    struct FundingAttribution {
        external_capital_minor: i64,
        existing_cash_minor: i64,
        reinvested_minor: i64,
        investment_income_minor: i64,
        unattributed_minor: i64,
        historical_cost_minor: Option<i64>,
        operation_type: Option<String>,
    }
    let mut inconsistent = 0_i64;
    let mut q=c.prepare("SELECT a.external_capital_minor,a.existing_cash_minor,a.reinvested_minor,a.investment_income_minor,a.unattributed_minor,r.historical_cost_minor,r.operation_type FROM brokerage_buy_funding_attributions a LEFT JOIN brokerage_operation_revisions r ON r.id=a.operation_revision_id ORDER BY a.operation_revision_id")?;
    for row in q.query_map([], |r| {
        Ok(FundingAttribution {
            external_capital_minor: r.get(0)?,
            existing_cash_minor: r.get(1)?,
            reinvested_minor: r.get(2)?,
            investment_income_minor: r.get(3)?,
            unattributed_minor: r.get(4)?,
            historical_cost_minor: r.get(5)?,
            operation_type: r.get(6)?,
        })
    })? {
        let funding = row?;
        let sum = funding
            .external_capital_minor
            .checked_add(funding.existing_cash_minor)
            .and_then(|x| x.checked_add(funding.reinvested_minor))
            .and_then(|x| x.checked_add(funding.investment_income_minor))
            .and_then(|x| x.checked_add(funding.unattributed_minor));
        if sum != funding.historical_cost_minor || funding.operation_type.as_deref() != Some("buy")
        {
            inconsistent = inconsistent
                .checked_add(1)
                .ok_or_else(|| anyhow!("funding finding overflow"))?;
        }
    }
    if inconsistent > 0 {
        out.push(Finding {
            kind: Kind::Invariant,
            code: "brokerage_funding_attribution_inconsistent",
            entity: "brokerage_buy_funding_attributions",
            count: inconsistent,
        });
    }
    Ok(())
}
