BEGIN;
UPDATE canonical_transactions
SET posted_date = '2026-03-05'
WHERE id = 'cop-expense-feb-b';
INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind)
VALUES ('cop-expense-mar', 'cop-checking', '2026-03-10', 'Synthetic refreshed expense', -10000, 'COP', 'expense');
INSERT INTO transaction_lines(id, canonical_transaction_id, category_id, amount_minor, currency, line_kind)
VALUES ('line-mar-food', 'cop-expense-mar', 'food', -10000, 'COP', 'expense');
COMMIT;
