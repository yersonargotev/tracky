# PROTOTYPE — dashboard information architecture

Three deliberately different information architectures for Tracky's local,
read-only analytics dashboard. This is a disposable decision aid: synthetic data,
no persistence, no production dependencies.

## Decision

**Accepted: A — Monthly ledger.** It preserves the finance and investment MVP as
one scan-friendly overview while making a single native-currency scope explicit.
B's plain-language questions may inform copy, and C's strong freshness signals
may inform alerts, but neither alternative supplies the primary hierarchy.

The complete accepted and rejected rationale lives in
[Prototype the dashboard information architecture](../../issues/0048-prototype-dashboard-information-architecture.md).
These files remain only as temporary Wayfinder decision evidence and must not be
promoted directly into production.

## Run

```sh
./docs/prototypes/dashboard-information-architecture/serve.sh
```

Open <http://127.0.0.1:4173/?variant=A>. Use the floating switcher or the left and
right arrow keys to compare:

- **A — Monthly ledger:** one scrollable overview, organized from cash flow to
  spending to investments.
- **B — Question paths:** progressive disclosure through Cash flow, Spending,
  and Investments pages.
- **C — Signal desk:** trust and freshness first, with separate currency lanes.

Use the prototype controls to compare normal, stale-data, and empty states. Click
chart marks or breakdown rows to inspect the read-only transaction drill-down.

## Design plan

- **Question:** Which hierarchy gets a user to a trustworthy explanation of
  finance and investment state fastest without implying conversion or editing?
- **Palette:** ledger ink `#18231f`, chalk `#f4f5ef`, Tracky green `#1d684d`,
  signal amber `#c87822`, reconciliation red `#a3484f`, quiet slate `#68756f`.
- **Type:** Georgia for editorial orientation, system sans for prose, and system
  monospace for exact amounts and dates; no external assets or fonts.
- **Signature:** notched currency rails behave like separate physical ledger
  tabs, making the no-cross-currency boundary visible rather than explanatory
  fine print.
- **Structure:** the variants disagree about the organizing principle (time,
  user question, or data trust), not merely styling.

The visual risk is an accounting-workpaper texture and exposed table rules rather
than a conventional card grid. Decoration is intentionally limited to that one
device so dense financial content stays legible.
