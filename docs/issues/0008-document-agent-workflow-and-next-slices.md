# 0008 — Document the agent workflow for PDF imports

Labels: `ready-for-agent`

## Parent

- PRD: `docs/prd/review-first-pdf-import.md`

## What to build

Document how an agent or human should run the new PDF inspect/import/review commands, interpret JSON output, and avoid sensitive-data mistakes. Include the current limits and the next likely slices after the CLI path works.

## Acceptance criteria

- [ ] Documentation explains the happy path from `pdf inspect` to `import pdf` to candidate review.
- [ ] Documentation explains password handling and confirms Tracky does not store PDF passwords.
- [ ] Documentation explains candidate statuses, provenance, duplicate source documents, and possible duplicates.
- [ ] Documentation includes examples of JSON-oriented usage for agents.
- [ ] Documentation calls out non-goals: no TUI, no MCP, no AI fallback, no password storage in this milestone.

## Blocked by

- `0007-add-candidate-review-cli.md`
