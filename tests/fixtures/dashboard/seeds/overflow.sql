BEGIN;
INSERT INTO institutions(id, name) VALUES ('synthetic-bank', 'Synthetic Bank');
INSERT INTO accounts(id, institution_id, label, currency, kind, is_owned) VALUES ('cop-checking', 'synthetic-bank', 'COP Checking', 'COP', 'checking', 1);
INSERT INTO income_sources(id, name) VALUES ('synthetic-income', 'Synthetic Income');
INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind, income_source_id, income_kind) VALUES
  ('max-income', 'cop-checking', '2026-01-01', 'Synthetic maximum', 9223372036854775807, 'COP', 'income', 'synthetic-income', 'other'),
  ('one-income', 'cop-checking', '2026-01-02', 'Synthetic overflow addend', 1, 'COP', 'income', 'synthetic-income', 'other');
COMMIT;
