# Brokerage lifecycle JSON contract

All `brokerages` actions require `--json` and return schema `tracky.brokerage.v1`. Stable actions are `open`, `deposit`, `buy`, `sell`, `dividend`, `withdraw`, `replace-operation`, `list`, and `inspect`.

`accounts[].cash` separates available cash, external capital, withdrawals, gross dividends, gross sale proceeds, realized results, fees, withholding, and other deductions by currency. `accounts[].positions` contains exact quantity and historical cost by security. `active_operations` exposes current heads; inspection also exposes full `operation_history`, including correction reason, replaced revision, and provenance.

Amounts are signed only where direction is intrinsic (`net_cash_minor`); component fields are non-negative minor units. Quantities are canonical decimal strings with at most nine fractional digits. Sales and dividends reconcile net cash exactly from gross less fee, withholding, and other deductions. Deposits consume confirmed allocation identity and count as external capital; purchases and reinvestments do not. Full sale proceeds and withdrawals are not income; gross dividends are investment income.
