# Changelog

## 0.2.2

- Render COP and USD dashboard amounts in major currency units with familiar
  locale-specific thousands and decimal separators.
- Apply the readable format consistently to progressive HTML, live filters and
  refreshes, breakdowns, investments, alerts, and canonical drill-down rows.
- Preserve exact minor-unit integers in dashboard API transport and semantic
  `data-minor` attributes.

## 0.2.1

- Add content-based detection for Nu credit-card statements without relying on
  filenames.
- Preserve explicit card charge, payment, credit, reversal, and refund
  semantics through the review-first PDF workflow.
- Keep imported statement evidence redacted, migrate existing SQLite databases
  compatibly, and suggest card-payment transfers only when the destination
  account resolves uniquely.

## 0.2.0

- Add the supported `tracky dashboard` command for read-only local finance,
  investment, alert, and Monthly-ledger analysis.
- Add exact filters and drill-downs, explicit refresh with last-good recovery,
  progressive semantic HTML, keyboard/focus behavior, and automated browser and
  accessibility coverage.
- Keep large dashboard snapshots within the release memory and latency budgets
  by aggregating in SQLite and creating read-only snapshots out of process.
- Bind release publication to complete approved packaged-dashboard evidence for
  every supported Cargo Dist target and attach its JSON and Markdown renderings.
- Ship prebuilt packages for Apple Silicon macOS and x86-64 glibc Linux; macOS
  Intel is no longer a supported build target.

The dashboard is a supported Tracky feature, not a beta or preview. It remains
local-only and never mutates or migrates the selected database.
