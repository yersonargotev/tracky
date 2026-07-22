const variants = {
  A: "Monthly ledger",
  B: "Question paths",
  C: "Signal desk",
};

const model = {
  view: new URLSearchParams(location.search).get("variant") || "A",
  state: new URLSearchParams(location.search).get("state") || "normal",
  currency: "COP",
  section: "cashflow",
  filterOpen: false,
  drawer: null,
  refreshedAt: "21 Jul · 9:42 PM",
};

const money = {
  COP: { income: "$ 12,800,000", expenses: "$ 7,460,000", savings: "$ 5,340,000", contribution: "$ 1,500,000" },
  USD: { income: "$ 2,100.00", expenses: "$ 1,284.00", savings: "$ 816.00", contribution: "$ 400.00" },
};

const months = [
  { label: "Feb", income: 58, expense: 38, save: 20 },
  { label: "Mar", income: 66, expense: 44, save: 22 },
  { label: "Apr", income: 62, expense: 52, save: 10 },
  { label: "May", income: 78, expense: 48, save: 30 },
  { label: "Jun", income: 72, expense: 46, save: 26 },
  { label: "Jul", income: 83, expense: 49, save: 34 },
];

const categories = [
  ["Home", 34, "$ 2,536,000"],
  ["Food", 22, "$ 1,641,200"],
  ["Transport", 14, "$ 1,044,400"],
  ["Health", 9, "$ 671,400"],
];

function setQuery(key, value) {
  const query = new URLSearchParams(location.search);
  query.set(key, value);
  history.replaceState({}, "", `${location.pathname}?${query}`);
}

function currencyTabs() {
  return `<div class="currency-tabs" aria-label="Currency">
    ${["COP", "USD"].map(code => `<button class="${model.currency === code ? "active" : ""}" data-currency="${code}">${code}<span>separate ledger</span></button>`).join("")}
  </div>`;
}

function globalHeader(compact = false) {
  return `<header class="global-header ${compact ? "compact" : ""}">
    <a class="brand" href="#" aria-label="Tracky home"><span>t/</span> tracky</a>
    <div class="scope"><span class="read-only">Read only</span><span>1 Jan–21 Jul 2026</span><span class="account-scope">All accounts</span></div>
    <div class="header-actions">
      <button class="quiet" data-action="filters">Filters <b>4</b></button>
      <button class="refresh" data-action="refresh">↻ Refresh</button>
    </div>
  </header>${model.filterOpen ? filterPanel() : ""}`;
}

function filterPanel() {
  return `<aside class="filter-panel" aria-label="Dashboard filters">
    <div><small>Date range</small><strong>1 Jan 2026 → 21 Jul 2026</strong></div>
    <div><small>Currency</small><strong>${model.currency} only</strong></div>
    <div><small>Accounts</small><strong>All accounts</strong></div>
    <div><small>Categories</small><strong>All categories</strong><em>Applies to expenses only</em></div>
    <button data-action="filters">Close</button>
  </aside>`;
}

function stateControls() {
  return `<div class="state-controls"><span>Test state</span>${["normal", "stale", "empty"].map(state => `<button class="${model.state === state ? "active" : ""}" data-state="${state}">${state}</button>`).join("")}</div>`;
}

function metricStrip() {
  const m = money[model.currency];
  return `<section class="metric-strip" aria-label="Period totals">
    <article><small>Income</small><strong>${m.income}</strong><span>+8% from prior period</span></article>
    <article><small>Expenses</small><strong>${m.expenses}</strong><span>58% of income</span></article>
    <article class="primary"><small>Savings · cash flow</small><strong>${m.savings}</strong><span>Income minus expenses</span></article>
    <article><small>Invested</small><strong>${m.contribution}</strong><span>Separate from expenses</span></article>
  </section>`;
}

function monthlyChart(title = "Monthly cash flow") {
  return `<section class="chart-block">
    <div class="section-heading"><div><small>CASH FLOW · ${model.currency}</small><h2>${title}</h2></div><div class="legend"><i class="income"></i>Income <i class="expense"></i>Expense</div></div>
    <div class="bar-chart" aria-label="Monthly income and expense bars">
      ${months.map(m => `<button class="month" data-drill="${m.label} cash flow" aria-label="Open ${m.label} transactions"><span class="bars"><i class="income" style="height:${m.income}%"></i><i class="expense" style="height:${m.expense}%"></i></span><b>${m.label}</b></button>`).join("")}
    </div>
    <p class="chart-note">Select a month to see the canonical transactions behind the marks.</p>
  </section>`;
}

function categoryRows() {
  return `<div class="category-list">${categories.map(([name, share, amount]) => `<button data-drill="${name} expenses"><span><b>${name}</b><small>${share}% of expenses</small></span><i><em style="width:${share * 2.3}%"></em></i><strong>${model.currency === "COP" ? amount : "$ " + Math.round(parseInt(amount.replace(/\D/g, "")) / 4000).toLocaleString() + ".00"}</strong><span>›</span></button>`).join("")}</div>`;
}

function alerts() {
  if (model.state === "empty") return `<section class="empty-state"><span>∅</span><h2>No activity in this range</h2><p>Try a wider date range or clear account and category filters. Nothing was changed.</p><button data-action="filters">Review filters</button></section>`;
  const stale = model.state === "stale";
  return `<section class="alerts ${stale ? "urgent" : ""}">
    <div><small>${stale ? "VALUATION ATTENTION" : "RECONCILIATION"}</small><h2>${stale ? "2 positions need a newer snapshot" : "1 item needs attention"}</h2><p>${stale ? "Brokerage observations are 12 and 19 days old. Historical cost remains available." : "One brokerage position differs from its latest provider observation."}</p></div>
    <button data-drill="${stale ? "stale positions" : "reconciliation alert"}">Inspect ${stale ? "positions" : "difference"} →</button>
  </section>`;
}

function positionsTable() {
  const rows = model.state === "empty" ? [] : [
    ["Tyba · CDT 2026", "12,000,000 COP", "12,724,000 COP", model.state === "stale" ? "12d · stale" : "2d · fresh", "Matched"],
    ["IBKR · VTI", "4.250000000", "1,171.48 USD", model.state === "stale" ? "19d · stale" : "4d · fresh", "Difference"],
    ["Broker cash", "400.00 USD", "—", "Unavailable", "No snapshot"],
  ];
  return `<section class="positions"><div class="section-heading"><div><small>AS OF 21 JUL 2026</small><h2>Investment positions</h2></div><span>Historical cost ≠ market value</span></div>
    ${rows.length ? `<div class="table-wrap"><table><thead><tr><th>Position</th><th>Historical cost / qty</th><th>Observed value</th><th>Freshness</th><th>Reconciliation</th></tr></thead><tbody>${rows.map(row => `<tr data-drill="${row[0]}">${row.map((cell, index) => `<td${index === 3 && cell.includes("stale") ? ' class="bad"' : ""}>${cell}</td>`).join("")}</tr>`).join("")}</tbody></table></div>` : `<p class="inline-empty">No investment positions match this range and account selection.</p>`}
  </section>`;
}

function variantA() {
  return `<div class="variant-a">${globalHeader()}<main>
    <div class="page-intro"><div><p class="eyebrow">PERSONAL LEDGER · UPDATED ${model.refreshedAt}</p><h1>Where the year is going.</h1><p>Cash flow, spending, and investments in one read-only workpaper.</p></div>${currencyTabs()}</div>
    ${model.state === "empty" ? alerts() : `${metricStrip()}${monthlyChart()}<div class="two-column"><section><div class="section-heading"><div><small>EXPENSES · ${model.currency}</small><h2>What took the most</h2></div><button class="text-button" data-section="spending">See all →</button></div>${categoryRows()}</section>${alerts()}</div>${positionsTable()}`}
    ${stateControls()}
  </main></div>`;
}

function sectionNav() {
  return `<nav class="question-nav" aria-label="Dashboard questions">${[
    ["cashflow", "01", "Am I saving?"], ["spending", "02", "Where did it go?"], ["investments", "03", "Are positions current?"]
  ].map(([key, number, label]) => `<button class="${model.section === key ? "active" : ""}" data-section="${key}"><span>${number}</span>${label}</button>`).join("")}</nav>`;
}

function variantBContent() {
  if (model.state === "empty") return alerts();
  if (model.section === "spending") return `<div class="focus-layout"><div><p class="eyebrow">WHERE DID IT GO? · ${model.currency}</p><h1>${money[model.currency].expenses}</h1><p class="focus-copy">Consumption expenses across 86 canonical transactions. Transfers and ${money[model.currency].contribution} of investment contributions stay outside this number.</p></div><section><div class="section-heading"><h2>Expense paths</h2><span>Choose one to explain</span></div>${categoryRows()}</section></div>`;
  if (model.section === "investments") return `<div class="investment-focus"><div class="focus-lead"><p class="eyebrow">ARE POSITIONS CURRENT?</p><h1>${model.state === "stale" ? "Not entirely." : "Mostly."}</h1><p>${model.state === "stale" ? "Two observations are older than seven days." : "Two positions are fresh; one has no provider valuation."}</p>${alerts()}</div>${positionsTable()}</div>`;
  return `<div class="focus-layout"><div><p class="eyebrow">AM I SAVING? · ${model.currency}</p><h1>${money[model.currency].savings}</h1><p class="focus-copy">Income minus consumption expenses for this range. Transfers and investments are shown separately, never silently netted.</p><dl><div><dt>Income</dt><dd>${money[model.currency].income}</dd></div><div><dt>Expenses</dt><dd>− ${money[model.currency].expenses}</dd></div><div><dt>Invested separately</dt><dd>${money[model.currency].contribution}</dd></div></dl></div>${monthlyChart("How savings formed")}</div>`;
}

function variantB() {
  return `<div class="variant-b">${globalHeader(true)}<main><aside>${currencyTabs()}${sectionNav()}<div class="aside-note"><b>No combined balance</b><p>COP and USD are separate ledgers. Tracky applies no exchange rate.</p></div></aside><div class="question-content">${variantBContent()}${stateControls()}</div></main></div>`;
}

function currencyLane(code) {
  const m = money[code];
  return `<section class="currency-lane ${model.currency === code ? "selected" : ""}"><button class="lane-tab" data-currency="${code}"><b>${code}</b><span>open ledger</span></button><div class="lane-metrics"><div><small>Cash flow</small><strong>${m.savings}</strong></div><div><small>Expenses</small><strong>${m.expenses}</strong></div><div><small>Invested</small><strong>${m.contribution}</strong></div></div><div class="spark-bars">${months.map(mo => `<button data-drill="${mo.label} ${code} cash flow" style="height:${mo.save * 2}px" aria-label="${mo.label} ${code}"></button>`).join("")}</div></section>`;
}

function variantC() {
  const empty = model.state === "empty";
  return `<div class="variant-c">${globalHeader(true)}<main>
    <section class="desk-heading"><div><p class="eyebrow">SIGNAL DESK · AS OF 21 JUL 2026</p><h1>${empty ? "The selected ledger is quiet." : model.state === "stale" ? "Start with two stale positions." : "One difference deserves a look."}</h1></div><p>Trust signals come before trends. Every amount stays inside its native currency lane.</p></section>
    ${alerts()}
    ${empty ? "" : `<section class="lanes">${currencyLane("COP")}${currencyLane("USD")}</section><div class="desk-grid"><section><div class="section-heading"><div><small>SELECTED LANE · ${model.currency}</small><h2>Largest expense signals</h2></div></div>${categoryRows()}</section>${positionsTable()}</div>`}
    ${stateControls()}
  </main></div>`;
}

function renderDrawer() {
  const root = document.querySelector("#drawer-root");
  if (!model.drawer) { root.innerHTML = ""; return; }
  root.innerHTML = `<div class="drawer-scrim" data-action="close-drawer"></div><aside class="drill-drawer" aria-label="Read-only drill-down"><button class="drawer-close" data-action="close-drawer" aria-label="Close">×</button><p class="eyebrow">READ-ONLY DRILL-DOWN</p><h2>${model.drawer}</h2><p>Canonical rows behind this value. Sorted by posted date and transaction ID.</p><div class="drawer-total"><small>Selected ledger</small><strong>${model.currency}</strong></div><ul>
    <li><time>18 Jul</time><span><b>Mercado Central</b><small>Food · Main account</small></span><strong>− ${model.currency === "COP" ? "$ 184,200" : "$ 46.05"}</strong></li>
    <li><time>12 Jul</time><span><b>Monthly income</b><small>Income · Main account</small></span><strong>+ ${model.currency === "COP" ? "$ 6,400,000" : "$ 1,050.00"}</strong></li>
    <li><time>04 Jul</time><span><b>Home services</b><small>Home · Main account</small></span><strong>− ${model.currency === "COP" ? "$ 312,000" : "$ 78.00"}</strong></li>
  </ul><div class="drawer-foot"><span>3 of 24 rows</span><button>Load next 21</button></div><p class="immutable-note">To correct canonical data, return to the Tracky CLI.</p></aside>`;
}

function bind() {
  document.querySelectorAll("[data-currency]").forEach(el => el.addEventListener("click", () => { model.currency = el.dataset.currency; render(); }));
  document.querySelectorAll("[data-section]").forEach(el => el.addEventListener("click", () => { model.section = el.dataset.section; render(); }));
  document.querySelectorAll("[data-state]").forEach(el => el.addEventListener("click", () => { model.state = el.dataset.state; setQuery("state", model.state); render(); }));
  document.querySelectorAll("[data-drill]").forEach(el => el.addEventListener("click", () => { model.drawer = el.dataset.drill; renderDrawer(); bindDrawer(); }));
  document.querySelectorAll('[data-action="filters"]').forEach(el => el.addEventListener("click", () => { model.filterOpen = !model.filterOpen; render(); }));
  document.querySelectorAll('[data-action="refresh"]').forEach(el => el.addEventListener("click", () => { model.refreshedAt = "just now"; render(); }));
}

function bindDrawer() {
  document.querySelectorAll('[data-action="close-drawer"]').forEach(el => el.addEventListener("click", () => { model.drawer = null; renderDrawer(); }));
}

function cycle(direction) {
  const keys = Object.keys(variants);
  const index = keys.indexOf(model.view);
  model.view = keys[(index + direction + keys.length) % keys.length];
  setQuery("variant", model.view);
  render();
}

function render() {
  const renderVariant = { A: variantA, B: variantB, C: variantC }[model.view] || variantA;
  document.querySelector("#app").innerHTML = renderVariant();
  document.querySelector("#variant-label").textContent = `${model.view} — ${variants[model.view]}`;
  bind();
  renderDrawer();
}

document.querySelector("#previous-variant").addEventListener("click", () => cycle(-1));
document.querySelector("#next-variant").addEventListener("click", () => cycle(1));
document.addEventListener("keydown", event => {
  if (["INPUT", "TEXTAREA"].includes(document.activeElement.tagName) || document.activeElement.isContentEditable) return;
  if (event.key === "ArrowLeft") cycle(-1);
  if (event.key === "ArrowRight") cycle(1);
  if (event.key === "Escape" && model.drawer) { model.drawer = null; renderDrawer(); }
});

render();
