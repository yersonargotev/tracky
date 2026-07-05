# 0009 — Add user-facing command reference

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`
- Follow-up from: `docs/issues/0008-document-agent-workflow-and-next-slices.md`

## What to build

Add a small user-facing command reference for the implemented review-first PDF inspect/import/review path. The reference should make the CLI workflow discoverable from the repository root while keeping the agent-specific details in `docs/agents/pdf-import-workflow.md`.

This is a documentation-only slice. It must not add new parser behavior, storage behavior, fixtures, PDFs, TUI flows, MCP wrappers, or AI fallback behavior.

## Acceptance criteria

- [x] A repository-level command reference exists and links to the detailed agent workflow and JSON contract.
- [x] The reference documents the path from `pdf inspect` to `import pdf` to `candidates list/accept/reject`.
- [x] The reference states that `pdf inspect` is read-only and writes no storage.
- [x] The reference states that `import pdf` persists source document, import batch, provenance, duplicate markers, and candidate transactions only.
- [x] The reference states that only `candidates accept` creates or links a canonical transaction and preserves provenance/auditability.
- [x] The reference avoids real PDF names, document credentials, sensitive account data, or unredacted examples.

## Blocked by

- `0008-document-agent-workflow-and-next-slices.md`
