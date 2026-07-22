-- A structurally compatible database can still contain invalid canonical domain data.
BEGIN;
INSERT INTO institutions(id, name) VALUES ('synthetic-bank', 'Synthetic Bank');
INSERT INTO accounts(id, institution_id, label, currency, kind, is_owned)
VALUES ('invalid-ledger', 'synthetic-bank', 'Invalid ledger', 'COP', 'checking', 1);
INSERT INTO income_sources(id, name) VALUES ('synthetic-income', 'Synthetic Income');
INSERT INTO canonical_transactions(
  id, account_id, posted_date, description, amount_minor, currency,
  transaction_kind, income_source_id, income_kind
) VALUES (
  'invalid-date', 'invalid-ledger', '2026-99-99', 'Synthetic invalid date',
  1000, 'COP', 'income', 'synthetic-income', 'other'
);
COMMIT;
