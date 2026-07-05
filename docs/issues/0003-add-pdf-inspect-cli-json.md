# 0003 — Add `tracky pdf inspect` JSON command

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Add the first product CLI command for read-only PDF inspection. It should inspect one protected PDF, use runtime-only password input, and emit stable machine-readable JSON matching the contract. It must not write SQLite data or create canonical transactions.

Example user-facing shape:

```bash
tracky pdf inspect assets/nequi-junio.pdf --password-env TRACKY_NEQUI_PDF_PASSWORD --json
```

## Acceptance criteria

- [ ] `tracky pdf inspect <PDF> --json` emits contract-compatible JSON for a supported PDF.
- [ ] The command supports password lookup from an env var without storing the password.
- [ ] The command reports source document hash/prefix, extractor status, parser status, candidate count, candidates, and provenance/evidence.
- [ ] The command exits successfully when extraction/parsing succeeds and returns stable JSON error output when it fails.
- [ ] The command is read-only and does not create or modify a database.
- [ ] CLI-level tests verify external JSON behavior at the command seam.

## Blocked by

- `0002-extract-pdf-inspection-core.md`
