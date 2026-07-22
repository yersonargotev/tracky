BEGIN;
INSERT INTO institutions(id, name) VALUES ('synthetic-bank', 'Synthetic Bank');
INSERT INTO accounts(id, institution_id, label, currency, kind, is_owned) VALUES
  ('cop-checking', 'synthetic-bank', 'COP Checking', 'COP', 'checking', 1),
  ('cop-savings', 'synthetic-bank', 'COP Savings', 'COP', 'savings', 1),
  ('cop-investment', 'synthetic-bank', 'COP Investment', 'COP', 'investment', 1),
  ('usd-checking', 'synthetic-bank', 'USD Checking', 'USD', 'checking', 1);
INSERT INTO categories(id, name) VALUES ('food', 'Food'), ('housing', 'Housing');
INSERT INTO income_sources(id, name) VALUES ('salary', 'Synthetic Salary');
INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind, income_source_id, income_kind) VALUES
  ('cop-income-jan', 'cop-checking', '2026-01-05', 'Synthetic income', 500000, 'COP', 'income', 'salary', 'salary'),
  ('cop-expense-jan', 'cop-checking', '2026-01-10', 'Synthetic split expense', -120000, 'COP', 'expense', NULL, NULL),
  ('cop-expense-feb-a', 'cop-checking', '2026-02-03', 'Synthetic food expense A', -30000, 'COP', 'expense', NULL, NULL),
  ('cop-expense-feb-b', 'cop-savings', '2026-02-03', 'Synthetic food expense B', -20000, 'COP', 'expense', NULL, NULL),
  ('cop-invest-feb', 'cop-investment', '2026-02-20', 'Synthetic investment contribution', -100000, 'COP', 'investment_contribution', NULL, NULL),
  ('usd-income-jan', 'usd-checking', '2026-01-07', 'Synthetic USD income', 10000, 'USD', 'income', 'salary', 'salary');
INSERT INTO transaction_lines(id, canonical_transaction_id, category_id, amount_minor, currency, line_kind) VALUES
  ('line-jan-food', 'cop-expense-jan', 'food', -70000, 'COP', 'expense'),
  ('line-jan-housing', 'cop-expense-jan', 'housing', -50000, 'COP', 'expense'),
  ('line-feb-food-a', 'cop-expense-feb-a', 'food', -30000, 'COP', 'expense'),
  ('line-feb-food-b', 'cop-expense-feb-b', 'food', -20000, 'COP', 'expense');
COMMIT;
