BEGIN;
INSERT INTO institutions(id, name) VALUES ('synthetic-broker', 'Synthetic Broker');
INSERT INTO accounts(id, institution_id, label, currency, kind, is_owned) VALUES ('broker-cop', 'synthetic-broker', 'COP Brokerage', 'COP', 'investment', 1);
INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind, investment_allocation_status) VALUES ('pending-capital', 'broker-cop', '2026-02-01', 'Synthetic pending capital', -50000, 'COP', 'investment_contribution', 'pending_allocation');
COMMIT;
