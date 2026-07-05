# Review-first import

Tracky imports from PDFs and AI-assisted extractors into candidate transactions first, not directly into canonical finance records. The user must review, accept, reject, or resolve duplicates before candidates become canonical transactions because financial records need auditability and AI/PDF extraction can be wrong.

## Consequences

- Every import records provenance: source document, extraction method, confidence, raw text or evidence, and import batch.
- CLI JSON, TUI review, and future MCP tools must expose candidate state instead of bypassing review.
- Importers may be aggressive about extraction as long as uncertain results remain candidates.
