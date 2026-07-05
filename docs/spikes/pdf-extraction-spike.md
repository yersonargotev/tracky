# PDF Extraction Spike

## Goal

Choose Tracky's first PDF extraction strategy using evidence from the real protected statement PDFs in `assets/`.

## Background

The initial sample PDFs are valid PDF files, but local `pdfinfo` and `pdftotext` fail with `Incorrect password`. PDF ingestion is therefore the highest-risk technical area and should be proven before building the full finance UI or import pipeline.

## Inputs

Evaluate these files:

- `assets/nequi-abril.pdf`
- `assets/nequi-mayo.pdf`
- `assets/nequi-junio.pdf`
- `assets/rappi-abril.pdf`
- `assets/rappi-mayo.pdf`
- `assets/rappi-junio.pdf`

Passwords should be supplied at runtime through CLI flags, interactive prompt, or environment variables loaded from `.env`-style configuration. Tracky should not store document passwords in the first version.

## Candidate Extractors

1. `pdf_oxide`
   - Attractive because it already has CLI and MCP-oriented tooling for local AI workflows.
   - Must be validated against password-protected PDFs.

2. `pdfium-render`
   - Robust Rust binding to PDFium with support for loading PDFs with passwords.
   - Requires validating distribution and Pdfium binding friction.

3. Python fallback, only if needed
   - Consider PyMuPDF or pdfplumber only if Rust-first options cannot extract useful text/layout.
   - Do not commit to Python before the Rust extractors are tested.

## Success Criteria

For each extractor, record whether it:

1. Opens password-protected PDFs.
2. Extracts useful text containing dates, amounts, descriptions, and balances when present.
3. Preserves enough layout or ordering to build deterministic parsers.
4. Works from Rust without painful setup.
5. Produces machine-readable output suitable for Codex, Claude Code, OpenCode, or future MCP tools.
6. Looks viable for future distribution through `cargo install`, Homebrew, or a packaged binary.

## Evaluation Table

| Extractor | Nequi | Rappi | Password | Layout | Rust DX | AI DX | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `pdf_oxide` | TBD | TBD | TBD | TBD | TBD | TBD | TBD |
| `pdfium-render` | TBD | TBD | TBD | TBD | TBD | TBD | TBD |
| Python fallback | Optional | Optional | Optional | Optional | Optional | Optional | Optional |

## Proposed Spike Command

The first command can be a diagnostic command rather than a full import:

```bash
tracky pdf inspect assets/nequi-junio.pdf --password-env TRACKY_NEQUI_PDF_PASSWORD --json
```

Expected shape:

```json
{
  "file": "assets/nequi-junio.pdf",
  "pages": 3,
  "encrypted": true,
  "text_extractable": true,
  "sample_text": "...",
  "recommended_parser": "nequi"
}
```

## Non-goals

- Do not build the full transaction schema in this spike.
- Do not create canonical transactions from PDFs yet.
- Do not store PDF passwords.
- Do not add MCP until the JSON CLI contract is useful.
