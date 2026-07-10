# 0024 — Restore strict Clippy cleanliness

Labels: `ready-for-agent`

## Parent

- Pre-TUI build audit on 2026-07-10.

## What to build

Make `cargo clippy --all-targets --locked -- -D warnings` pass before the TUI increases the codebase surface.

The current build, format check, release build, and full test suite pass, but strict Clippy reports four maintainability failures:

- `result_large_err` in `expense_lines_from_args`.
- `result_large_err` in `expense_line_from_arg`.
- `large_enum_variant` in `BatchActionMutation`.
- `too_many_arguments` in `transaction_ledger_success`.

Resolve the underlying shapes with small, behavior-preserving changes rather than broad lint suppression.

## Acceptance criteria

- [ ] `cargo clippy --all-targets --locked -- -D warnings` passes.
- [ ] The two expense-line parsing helpers no longer return a disproportionately large error variant by value.
- [ ] `BatchActionMutation` no longer makes every value pay for the full transfer-pair payload size.
- [ ] Transaction-ledger response construction no longer requires an eight-argument helper call.
- [ ] Stable JSON response shapes and CLI behavior remain unchanged.
- [ ] `cargo fmt --all -- --check`, `cargo build --all-targets --locked`, and `cargo test --all-targets --locked` still pass.
