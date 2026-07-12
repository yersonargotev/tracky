# PDF Extraction Spike

## Goal

Choose Tracky's first PDF extraction strategy using evidence from the real protected statement PDFs in `assets/`.

## Background

The initial sample PDFs are valid PDF files, but local `pdfinfo` and `pdftotext` fail with `Incorrect password`. PDF ingestion is therefore the highest-risk technical area and should be proven before building the full finance UI or import pipeline.

## Inputs

Evaluated these files:

- `assets/nequi-abril.pdf`
- `assets/nequi-mayo.pdf`
- `assets/nequi-junio.pdf`
- `assets/rappi-abril.pdf`
- `assets/rappi-mayo.pdf`
- `assets/rappi-junio.pdf`

Passwords were supplied at runtime from `.env` / environment variables:

- `TRACKY_NEQUI_PDF_PASSWORD`
- `TRACKY_RAPPI_PDF_PASSWORD`

Tracky must not store document passwords. The local `.env` file is ignored by git.

## Candidate Extractors

1. `pdf_oxide`
   - Attractive because it already has Rust, CLI, and MCP-oriented tooling for local AI workflows.
   - Supports password-protected PDFs through `PdfDocument::authenticate()` / password APIs.

2. `pdfium-render`
   - Robust Rust binding to PDFium with support for loading PDFs with passwords.
   - Requires a PDFium dynamic library at runtime; this spike used `pdfium-auto` for local binding/download.

3. Python fallback, only if needed
   - Consider PyMuPDF or pdfplumber only if Rust-first options cannot extract useful text/layout.
   - Not evaluated because both Rust-first options opened and extracted useful text from every protected sample PDF.

## Success Criteria

For each extractor, record whether it:

1. Opens password-protected PDFs.
2. Extracts useful text containing dates, amounts, descriptions, and balances when present.
3. Preserves enough layout or ordering to build deterministic parsers.
4. Works from Rust without painful setup.
5. Produces machine-readable output suitable for Codex, Claude Code, OpenCode, or future MCP tools.
6. Looks viable for future distribution through `cargo install`, Homebrew, or a packaged binary.

## Spike Runner

Implemented a minimal local diagnostic binary:

```bash
cargo run --example pdf_extraction_spike -- --pretty --no-prompt --output target/spike/pdf-extraction-results.json
```

Behavior:

- Loads `.env` through `dotenvy`.
- Resolves per-institution passwords from `TRACKY_NEQUI_PDF_PASSWORD` and `TRACKY_RAPPI_PDF_PASSWORD`.
- Supports month-specific overrides like `TRACKY_NEQUI_ABRIL_PDF_PASSWORD` before falling back to the institution-level key.
- Prompts interactively if no env var exists, unless `--no-prompt` is passed.
- Writes machine-readable JSON with per-file extractor status, counts, usefulness flags, bbox evidence, and redacted sample lines.
- Treats page-level `pdf_oxide` text/line extraction errors as extractor errors instead of silently returning partial page data.
- Redacts emails, cardholder/header names, long numbers, counterparties, addresses, card suffixes, and amounts from sample lines. It does not write passwords.

## Results

Run date: 2026-07-05 local session.

Aggregate metrics from `target/spike/pdf-extraction-results.json`:

| Extractor | Files attempted | Files opened | Useful text | Layout lines with coordinates | Total chars | Total lines | Avg elapsed/file |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `pdf_oxide` | 6 | 6 | 6 | 6 | 29,078 | 1,486 | ~150 ms |
| `pdfium-render` | 6 | 6 | 6 | 6 | 29,153 | 459 | ~99 ms |

Per-document summary:

| File | Extractor | Pages | Opened | Text chars | Lines | Dates | Amounts | Descriptions | Balances | Layout |
| --- | --- | ---: | --- | ---: | ---: | --- | --- | --- | --- | --- |
| `assets/nequi-abril.pdf` | `pdf_oxide` | 2 | yes | 2,421 | 127 | yes | yes | yes | yes | bbox lines |
| `assets/nequi-abril.pdf` | `pdfium-render` | 2 | yes | 2,465 | 44 | yes | yes | yes | yes | bbox lines |
| `assets/nequi-mayo.pdf` | `pdf_oxide` | 3 | yes | 3,196 | 168 | yes | yes | yes | yes | bbox lines |
| `assets/nequi-mayo.pdf` | `pdfium-render` | 3 | yes | 3,252 | 56 | yes | yes | yes | yes | bbox lines |
| `assets/nequi-junio.pdf` | `pdf_oxide` | 3 | yes | 3,682 | 181 | yes | yes | yes | yes | bbox lines |
| `assets/nequi-junio.pdf` | `pdfium-render` | 3 | yes | 3,745 | 64 | yes | yes | yes | yes | bbox lines |
| `assets/rappi-abril.pdf` | `pdf_oxide` | 4 | yes | 6,910 | 367 | yes | yes | yes | yes | bbox lines |
| `assets/rappi-abril.pdf` | `pdfium-render` | 4 | yes | 6,883 | 105 | yes | yes | yes | yes | bbox lines |
| `assets/rappi-mayo.pdf` | `pdf_oxide` | 3 | yes | 5,950 | 290 | yes | yes | yes | yes | bbox lines |
| `assets/rappi-mayo.pdf` | `pdfium-render` | 3 | yes | 5,922 | 89 | yes | yes | yes | yes | bbox lines |
| `assets/rappi-junio.pdf` | `pdf_oxide` | 4 | yes | 6,919 | 353 | yes | yes | yes | yes | bbox lines |
| `assets/rappi-junio.pdf` | `pdfium-render` | 4 | yes | 6,886 | 101 | yes | yes | yes | yes | bbox lines |

## Evaluation Table

| Extractor | Nequi | Rappi | Password | Layout | Rust DX | AI DX | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `pdf_oxide` | Opens and extracts all 3 samples | Opens and extracts all 3 samples | Works with `authenticate()`; reports encrypted/authenticated state | Produces text lines with bboxes; many granular lines, so parsers may need row reconstruction | Simple pure-Rust dependency; no native PDFium runtime | JSON wrapper is straightforward; ecosystem already has CLI/MCP crates | **Recommended first extractor** |
| `pdfium-render` | Opens and extracts all 3 samples | Opens and extracts all 3 samples | Works with `load_pdf_from_file(path, Some(password))` | Character bboxes allow custom visual line grouping; produced fewer, row-like lines in this spike | Good API, but needs PDFium library; `pdfium-auto` cached `libpdfium.dylib` under user cache | JSON wrapper is straightforward, but distribution story is heavier | Strong fallback / comparator |
| Python fallback | Not evaluated | Not evaluated | Not needed | Not needed | Would add non-Rust runtime | Not needed | Defer |

## Recommendation

Use `pdf_oxide` as Tracky's first PDF extraction backend.

Why:

- It opened every protected Nequi and Rappi PDF with the provided runtime passwords.
- It extracted dates, amounts, descriptions, and balance-related text from every sample.
- It returns line-level bounding boxes, enough to start deterministic institution parsers.
- It has the best initial distribution story for a Rust CLI because it avoids shipping or downloading a native PDFium library.
- It aligns well with Tracky's future agent workflows because `pdf_oxide` already has CLI/MCP-oriented crates, while Tracky can still expose its own stable JSON CLI contract.

Keep `pdfium-render` as an optional fallback/comparator, especially if real parser development shows `pdf_oxide` line granularity makes table row reconstruction too brittle. `pdfium-render` grouped this sample set into fewer visual rows and was faster after the PDFium library was available, but its native-library dependency makes it less attractive as the default extractor.

Do not add a Python fallback now. It would increase packaging complexity without solving a current blocker.

## Proposed Product Command

The future product command can remain a diagnostic command before full import:

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
  "sample_text": "<redacted sample>",
  "recommended_parser": "nequi"
}
```

## Non-goals

- Do not build the full transaction schema in this spike.
- Do not create canonical transactions from PDFs yet.
- Do not store PDF passwords.
- Do not add MCP until the JSON CLI contract is useful.

## Diagnostic Movement Parser Prototype

Added a first deterministic parser prototype on top of `pdf_oxide` line/bbox output. It is still part of the local spike runner, not the canonical import pipeline.

Command:

```bash
cargo run --example pdf_extraction_spike -- --pretty --no-prompt --output target/spike/pdf-parser-diagnostic.json
```

The JSON now includes `documents[].parsing` with:

- `extractor: "pdf_oxide"`
- institution-specific parser ids like `nequi_movement_rows_v0` and `rappi_movement_rows_v0`
- `candidate_count`
- `candidates[]` with page, row bbox, date, redacted description sample, amount, optional balance, confidence, and redacted row evidence
- `row_samples[]` for compact agent inspection, with `kind` values for `header`, `raw_table`, `near_miss`, and `candidate` rows so agents can inspect structure beyond successfully parsed candidates
- notes that the data is diagnostic/review-first only

Observed layout rules from the real protected samples:

- Nequi: `pdf_oxide` emits stable visual rows under `Fecha del movimiento`, `Descripción`, `Valor`, and `Saldo`; some rows combine amount and balance in one text line, so the parser splits money tokens by regex.
- Rappi: `pdf_oxide` emits smaller table cells; transaction rows have ISO dates and multiple monetary cells. The parser now chooses the visually leftmost non-zero money cell as the movement amount, because zero-valued cells are usually ancillary columns; descriptions may wrap to nearby cells. The Rappi statement table does not expose a per-row running balance, so `balance` is normally `null`.

Non-goals remain unchanged: these candidates are not canonical transactions, do not create the SQLite schema, and do not bypass review.
