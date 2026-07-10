# 0025 — Reconcile completed issue metadata

Labels: `ready-for-agent`

## Parent

- Pre-TUI tracker audit on 2026-07-10.

## What to build

Make the local Markdown tracker accurately distinguish implemented slices from remaining work before agents start the TUI.

Issues 0001–0007 and 0011 still have every acceptance criterion unchecked even though their commands, storage, contracts, and tests are present and passed the pre-TUI audit. Other completed issues use checked criteria, while all files retain the workflow label `ready-for-agent`. This makes dependency and next-slice discovery unreliable.

Verify historical completion from code, tests, docs, and commits, then update the tracker metadata without changing product behavior.

## Acceptance criteria

- [x] Acceptance criteria in issues 0001–0019 accurately reflect verified implementation state.
- [x] Any genuinely incomplete criterion remains unchecked and receives a focused follow-up issue instead of being marked complete optimistically.
- [x] The local tracker documents how completed issues are represented when status labels have no `done` value.
- [x] `docs/issues/README.md` clearly separates completed slices from the remaining queue or otherwise exposes current state unambiguously.
- [x] No product source, contract behavior, or sensitive asset data changes as part of this documentation-only cleanup.

## Reconciliation record

The audit checked each completed slice against its issue criteria, implementation files, focused tests, documentation, and the introducing/refining commits:

| Issues | Evidence reviewed |
| --- | --- |
| 0001–0003 | `docs/contracts/review-first-pdf-import-json.md`, `src/pdf.rs`, `src/cli.rs`, `tests/pdf_inspect_cli.rs`; commits `5d3b398`, `dc6a077`, `d6cf90e` and follow-up fixes. |
| 0004–0007 | `migrations/0001_review_first_schema.sql`, `src/storage.rs`, `src/cli.rs`, `tests/storage_migrations.rs`, `tests/import_pdf_persistence.rs`, `tests/candidate_review_cli.rs`; commits `92455a0`, `03ad58c`, `910f7de`, `da6a597`. |
| 0008–0010 | `docs/agents/pdf-import-workflow.md`, repository command reference, contract updates, and redacted parser/CLI tests; commits `e671123`, `e614efe`, `b71d34e`. |
| 0011–0014 | Account registry and typed transfer/income/expense review implementation plus `tests/account_registry_cli.rs`, `tests/import_pdf_persistence.rs`, and `tests/candidate_review_cli.rs`; commits `5fab36e`, `8409299`, `3233e62`, `e08bf4a`. |
| 0015–0019 | Split, manual ledger, transaction listing/editing, reports, and batch-review implementations with their focused integration tests; commits `5db697b`, `14c4510`, `63d84ee`, `9c5f9c5`, `4c46498`. |
| 0022 | Typed generic-accept guardrails, stable errors, non-mutation tests, and workflow/README examples; commit `2f702a4`. All seven criteria remain supported and checked. |
| 0023 | Cross-batch implementation and synthetic two-batch test in `tests/batch_review_cli.rs`; commit `96e1d39`. All seven criteria are now checked. |
| 0024 | Strict Clippy cleanup in `src/cli.rs` and `src/storage.rs`, verified at commit `5cde391`. All six criteria are now checked. |

No criterion in completed issues 0001–0019, 0022, 0023, or 0024 was found incomplete, so no new follow-up issue was warranted. Issues 0020 and 0021 remain the explicitly unchecked pending queue; 0020 was intentionally not started in this slice.
