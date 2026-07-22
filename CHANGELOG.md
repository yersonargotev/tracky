# Changelog

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

The dashboard is a supported Tracky feature, not a beta or preview. It remains
local-only and never mutates or migrates the selected database.
