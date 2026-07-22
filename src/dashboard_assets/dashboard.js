(() => {
  "use strict";

  const dashboard = document.querySelector("[data-dashboard]");
  if (!dashboard) return;

  const filterPanel = dashboard.querySelector("[data-filter-panel]");
  const drawer = document.querySelector("[data-drawer]");
  const drawerContent = drawer?.querySelector("[data-drawer-content]");
  let snapshot = null;
  let returnFocus = null;
  let nextCursor = null;
  let activeDrill = null;
  let requestInFlight = false;
  let selectedCurrency = dashboard.querySelector("[data-currency][aria-pressed=\"true\"]")?.dataset.currency || "";

  // These selectors are also the public progressive-enhancement contract.
  const actions = {
    apply: '[data-action="apply-filters"]',
    open: '[data-action="open-drawer"]',
    more: '[data-action="load-more"]',
  };

  const element = (tag, text, className) => {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  };

  const region = (name) => dashboard.querySelector(`[data-region="${name}"]`);

  const exact = (value) => {
    if (value === null) return "null";
    if (value === undefined) return "";
    if (typeof value === "object") return JSON.stringify(value);
    return String(value);
  };

  const amount = (value, currency) =>
    currency ? `${exact(value)} ${exact(currency)}` : exact(value);

  const actionButton = (label, data) => {
    const button = element("button");
    button.type = "button";
    button.dataset.action = "open-drawer";
    button.setAttribute("data-action", "open-drawer");
    Object.entries(data).forEach(([key, value]) => {
      if (value !== null && value !== undefined) button.dataset[key] = String(value);
    });
    button.append(label);
    return button;
  };

  const heading = (name, text) => {
    const node = region(name);
    if (!node) return null;
    node.replaceChildren(element("h2", text));
    return node;
  };

  const empty = (parent) => parent.append(element("p", "No matching canonical activity.", "empty"));

  const renderSummary = (data) => {
    const parent = heading("summary", "Ledger summary");
    if (!parent) return;
    if (data.state === "filter_empty") {
      parent.replaceChildren(
        element("p", "Filter-empty ledger", "eyebrow"),
        element("h2", "No activity matches these filters"),
        element("p", "Review the selected accounts and expense categories in Filters.", "empty"),
      );
      return;
    }
    const list = element("ul");
    const metrics = [
      ["Income", "income", "income_minor"],
      ["Consumption expense", "consumption_expense", "consumption_expense_minor"],
      ["Savings / net cash flow", "net_cash_flow", "net_cash_flow_minor"],
      ["Investment contributions", "investment_contribution", "investment_contribution_minor"],
    ];
    metrics.forEach(([label, metric, field]) => {
      const item = element("li");
      const row = element("span", label);
      row.append(element("strong", amount(data.summary[field], data.summary.currency)));
      item.append(actionButton(row, { metric }));
      list.append(item);
    });
    parent.append(list);
  };

  const renderMonthly = (data) => {
    const parent = heading("monthly", "Monthly activity");
    if (!parent) return;
    if (!data.monthly.length) return empty(parent);
    const trend = element("div", undefined, "trend");
    trend.setAttribute("role", "group");
    trend.setAttribute("aria-label", "Monthly income and consumption expense trend");
    data.monthly.forEach((month) => {
      const label = element("span", exact(month.month));
      const income = element("span", `Income ${amount(month.income_minor, month.currency)}`, "trend-income");
      const expense = element("span", `Expense ${amount(month.consumption_expense_minor, month.currency)}`, "trend-expense");
      const button = actionButton(label, { metric: "activity", month: month.month });
      button.append(income, expense);
      trend.append(button);
    });
    parent.append(trend);
    const scroll = element("div", undefined, "table-scroll");
    const table = element("table");
    table.setAttribute("aria-label", "Monthly income and consumption expense trend");
    table.append(element("caption", "Exact monthly amounts in minor units"));
    const head = element("thead");
    const headRow = element("tr");
    ["Month", "Income", "Consumption expense", "Savings", "Investment contributions"].forEach((label) => {
      const cell = element("th", label);
      cell.scope = "col";
      headRow.append(cell);
    });
    head.append(headRow);
    const body = element("tbody");
    data.monthly.forEach((month) => {
      const row = element("tr");
      const monthCell = element("th");
      monthCell.scope = "row";
      monthCell.append(actionButton(exact(month.month), { metric: "activity", month: month.month }));
      row.append(monthCell);
      ["income_minor", "consumption_expense_minor", "net_cash_flow_minor", "investment_contribution_minor"].forEach((field) => {
        row.append(element("td", amount(month[field], month.currency)));
      });
      body.append(row);
    });
    table.append(head, body);
    scroll.append(table);
    parent.append(scroll);
  };

  const renderBreakdown = (name, title, rows, options) => {
    const parent = heading(name, title);
    if (!parent) return;
    if (!rows.length) return empty(parent);
    const list = element("ul", undefined, "ledger-list");
    rows.forEach((row) => {
      const item = element("li");
      const content = element("span", undefined, "ledger-row");
      content.append(
        element("span", exact(row[options.label])),
        element("strong", options.value(row)),
      );
      item.append(actionButton(content, options.data(row)));
      list.append(item);
    });
    parent.append(list);
  };

  const renderAlerts = (data) => {
    const parent = heading("alerts", "Alerts");
    if (!parent) return;
    if (!data.alerts.length) return empty(parent);
    const list = element("ul", undefined, "ledger-list");
    data.alerts.forEach((alert, index) => {
      const item = element("li", undefined, "alert");
      const content = element("span", undefined, "ledger-row");
      content.append(element("span", exact(alert.kind)), element("strong", exact(alert.severity)));
      const positionIndex = data.investments.closing_positions.findIndex((position) =>
        alert.account_id === position.account_id && (!alert.instrument_id || alert.instrument_id === position.instrument_id));
      item.append(actionButton(content, {
        detail: "alert",
        index,
        metric: "investment_contribution",
        account: alert.account_id,
        positionIndex: positionIndex >= 0 ? positionIndex : null,
      }));
      list.append(item);
    });
    parent.append(list);
  };

  const renderInvestments = (data) => {
    const parent = heading("investments", "Investments");
    if (!parent) return;
    parent.append(element("p", `State: ${exact(data.investments.state)}`, "meta"));
    parent.append(element("p", `Pending allocation: ${amount(data.investments.pending_allocation_minor, data.filters.currency)}`, "meta"));
    if (data.investments.flows.length) {
      const flows = element("ul", undefined, "ledger-list");
      data.investments.flows.forEach((flow) => {
        const item = element("li");
        item.append(
          element("span", exact(flow.account_id)),
          element("strong", amount(flow.amount_minor, flow.currency)),
        );
        flows.append(item);
      });
      parent.append(flows);
    }
    const list = element("ul", undefined, "ledger-list");
    data.investments.closing_positions.forEach((position, index) => {
      const item = element("li");
      const content = element("span", undefined, "ledger-row");
      content.append(
        element("span", exact(data.dimensions.instruments.find((instrument) => instrument.id === position.instrument_id)?.name ?? position.instrument_id ?? position.instrument_type ?? position.account_id)),
        element("strong", exact(position.freshness)),
      );
      item.append(actionButton(content, {
        detail: "position",
        index,
        metric: "investment_contribution",
        account: position.account_id,
      }));
      list.append(item);
    });
    if (list.children.length) parent.append(list);
    else if (!data.investments.flows.length) empty(parent);
  };

  const renderSnapshot = (data) => {
    snapshot = data;
    const scope = region("scope");
    if (scope) {
      const scopeNames = (ids, dimensions, empty) => {
        if (!ids.length) return empty;
        const names = ids.map((id) => dimensions.find((item) => item.id === id)?.name || id);
        return names.join(", ");
      };
      const accounts = scopeNames(data.filters.account_ids, data.dimensions.accounts, "No accounts selected");
      const categories = scopeNames(data.filters.category_ids, data.dimensions.categories, "No expense categories selected");
      scope.replaceChildren(
        element("p", "Local analytical ledger · read-only", "eyebrow"),
        element("h1", "Monthly ledger"),
        element("p", `${exact(data.filters.start_date)} through ${exact(data.filters.end_date)} · ${exact(data.filters.currency)} · Accounts: ${accounts} · Expense categories: ${categories} · read ${exact(data.read_at)}`, "scope"),
      );
    }
    const currency = region("currency");
    selectedCurrency = data.filters.currency === null ? "" : exact(data.filters.currency);
    if (currency) {
      currency.replaceChildren(element("span", "Native-currency ledger"));
      const choices = element("div");
      data.dimensions.currencies.forEach((value) => {
        const button = element("button", exact(value));
        button.type = "button";
        button.dataset.currency = exact(value);
        button.setAttribute("aria-pressed", String(value === data.filters.currency));
        choices.append(button);
      });
      currency.append(choices);
    }
    renderSummary(data);
    renderMonthly(data);
    renderBreakdown("categories", "Consumption categories", data.categories, {
      label: "category_name",
      value: (row) => amount(row.amount_minor, row.currency),
      data: (row) => ({ metric: "consumption_expense", category: row.category_id }),
    });
    renderBreakdown("accounts", "Accounts", data.accounts, {
      label: "account_name",
      value: (row) => amount(row.net_cash_flow_minor, row.currency),
      data: (row) => ({ metric: "activity", account: row.account_id }),
    });
    renderAlerts(data);
    renderInvestments(data);
    syncFilterControls(data);
  };

  const namedControls = (name) =>
    filterPanel ? Array.from(filterPanel.querySelectorAll(`[name="${name}"]`)) : [];

  const selectedValues = (name) => {
    if (name === "currency" && !namedControls(name).length) return selectedCurrency ? [selectedCurrency] : [];
    const controls = namedControls(name);
    if (!controls.length) return [];
    const values = [];
    controls.forEach((control) => {
      if (control instanceof HTMLSelectElement && control.multiple) {
        Array.from(control.selectedOptions).forEach((option) => values.push(option.value));
      } else if ((control.type === "checkbox" || control.type === "radio") && control.checked) {
        values.push(control.value);
      } else if (control.type !== "checkbox" && control.type !== "radio") {
        values.push(control.value);
      }
    });
    return values;
  };

  const filterParams = () => {
    const params = new URLSearchParams();
    ["start", "end", "currency"].forEach((name) => {
      const values = selectedValues(name);
      if (values.length) params.append(name, values[0]);
    });
    ["account", "category"].forEach((name) => {
      const values = selectedValues(name).filter(Boolean);
      if (values.length) values.forEach((value) => params.append(name, value));
      else params.append(name, "");
    });
    return params;
  };

  const syncFilterControls = (data) => {
    const selected = {
      account: new Set(data.filters.account_ids),
      category: new Set(data.filters.category_ids),
    };
    namedControls("start").forEach((control) => { control.value = data.filters.start_date; });
    namedControls("end").forEach((control) => { control.value = data.filters.end_date; });
    ["account", "category"].forEach((name) => {
      namedControls(name).forEach((control) => {
        if (control instanceof HTMLInputElement && (control.type === "checkbox" || control.type === "radio")) {
          control.checked = selected[name].has(control.value);
        }
      });
    });
  };

  const fetchJson = async (path, params) => {
    const query = params.toString();
    const response = await fetch(query ? `${path}?${query}` : path, {
      headers: { accept: "application/json" },
      credentials: "same-origin",
    });
    if (!response.ok) throw new Error(`Dashboard request failed: ${response.status}`);
    return response.json();
  };

  const applyFilters = async (trigger, params = filterParams()) => {
    if (requestInFlight) return;
    requestInFlight = true;
    const startedAt = performance.now();
    trigger.disabled = true;
    dashboard.setAttribute("aria-busy", "true");
    try {
      const data = await fetchJson("api/v1/dashboard", params);
      renderSnapshot(data);
      dashboard.dataset.lastInteractionMs = String(performance.now() - startedAt);
      if (filterPanel) filterPanel.hidden = true;
      const toggle = dashboard.querySelector('[data-action="toggle-filters"]');
      toggle?.setAttribute("aria-expanded", "false");
    } finally {
      requestInFlight = false;
      trigger.disabled = false;
      dashboard.removeAttribute("aria-busy");
    }
  };

  const detailList = (record) => {
    const list = element("dl");
    Object.entries(record).forEach(([key, value]) => {
      list.append(element("dt", key), element("dd", exact(value)));
    });
    return list;
  };

  const showDrawer = (title, content, trigger) => {
    if (!drawer || !drawerContent) return;
    returnFocus = trigger;
    drawerContent.replaceChildren(
      element("p", "Canonical detail", "eyebrow"),
      element("h2", title),
      content,
    );
    drawer.hidden = false;
    document.querySelector(".drawer-scrim")?.removeAttribute("hidden");
    if (typeof drawer.showModal === "function" && !drawer.open) drawer.showModal();
    drawer.querySelector('[data-action="close-drawer"]')?.focus();
  };

  const closeDrawer = () => {
    if (!drawer || drawer.hidden) return;
    if (typeof drawer.close === "function" && drawer.open) drawer.close();
    drawer.hidden = true;
    document.querySelector(".drawer-scrim")?.setAttribute("hidden", "");
    returnFocus?.focus();
    returnFocus = null;
    nextCursor = null;
    activeDrill = null;
  };

  const drillParams = (button, cursor) => {
    const params = filterParams();
    params.set("metric", button.dataset.metric);
    if (button.dataset.month) params.set("month", button.dataset.month);
    if (button.dataset.account) {
      params.delete("account");
      params.append("account", button.dataset.account);
    }
    if (button.dataset.category) {
      params.delete("category");
      params.append("category", button.dataset.category);
    }
    params.set("limit", "50");
    if (cursor) params.set("cursor", cursor);
    return params;
  };

  const rowsTable = (rows) => {
    const scroll = element("div", undefined, "table-scroll");
    const table = element("table");
    const head = element("thead");
    const headRow = element("tr");
    ["Posted", "Description", "Kind", "Amount"].forEach((label) => {
      const cell = element("th", label);
      cell.scope = "col";
      headRow.append(cell);
    });
    head.append(headRow);
    const body = element("tbody");
    rows.forEach((row) => {
      const tr = element("tr");
      tr.append(
        element("td", exact(row.posted_date)),
        element("td", exact(row.description)),
        element("td", exact(row.transaction_kind)),
        element("td", amount(row.amount_minor, row.currency)),
      );
      body.append(tr);
    });
    table.append(head, body);
    scroll.append(table);
    return { scroll, body };
  };

  const openDrill = async (button) => {
    if (requestInFlight) return;
    requestInFlight = true;
    try {
      const data = await fetchJson("api/v1/transactions", drillParams(button));
      const table = rowsTable(data.rows);
      const wrapper = element("div");
      wrapper.append(table.scroll);
      nextCursor = data.next_cursor;
      activeDrill = { button, body: table.body };
      if (nextCursor) {
        const more = element("button", "Load more");
        more.type = "button";
        more.dataset.action = "load-more";
        more.setAttribute("data-action", "load-more");
        wrapper.append(more);
      }
      showDrawer(exact(data.metric), wrapper, button);
    } finally {
      requestInFlight = false;
    }
  };

  const openRecord = async (button) => {
    if (requestInFlight) return;
    requestInFlight = true;
    try {
      if (!snapshot) snapshot = await fetchJson("api/v1/dashboard", filterParams());
      const index = Number(button.dataset.index);
      const record = button.dataset.detail === "alert"
        ? snapshot.alerts[index]
        : snapshot.investments.closing_positions[index];
      if (!record) return;
      const title = button.dataset.detail === "alert"
        ? exact(record.kind)
        : exact(snapshot.dimensions.instruments.find((instrument) => instrument.id === record.instrument_id)?.name ?? record.instrument_id ?? record.instrument_type ?? record.account_id);
      const wrapper = element("div");
      wrapper.append(detailList(record));
      const positionIndex = Number(button.dataset.positionIndex);
      if (button.dataset.detail === "alert" && Number.isInteger(positionIndex)) {
        const position = snapshot.investments.closing_positions[positionIndex];
        if (position) wrapper.append(element("h3", "Affected position"), detailList(position));
      }
      if (button.dataset.account && button.dataset.metric) {
        const data = await fetchJson("api/v1/transactions", drillParams(button));
        wrapper.append(element("h3", "Related canonical rows"));
        const table = rowsTable(data.rows);
        wrapper.append(table.scroll);
        nextCursor = data.next_cursor;
        activeDrill = { button, body: table.body };
        if (nextCursor) {
          const more = element("button", "Load more");
          more.type = "button";
          more.dataset.action = "load-more";
          more.setAttribute("data-action", "load-more");
          wrapper.append(more);
        }
      }
      showDrawer(title, wrapper, button);
    } finally {
      requestInFlight = false;
    }
  };

  const loadMore = async (button) => {
    if (requestInFlight || !activeDrill || !nextCursor) return;
    requestInFlight = true;
    button.disabled = true;
    try {
      const data = await fetchJson("api/v1/transactions", drillParams(activeDrill.button, nextCursor));
      const page = rowsTable(data.rows);
      Array.from(page.body.children).forEach((row) => activeDrill.body.append(row));
      nextCursor = data.next_cursor;
      if (!nextCursor) button.remove();
    } finally {
      requestInFlight = false;
      if (button.isConnected) button.disabled = false;
    }
  };

  document.addEventListener("click", (event) => {
    const button = event.target.closest("button, [data-action]");
    if (!(button instanceof HTMLElement)) return;
    const action = button.dataset.action;
    if (button.dataset.currency !== undefined) {
      selectedCurrency = button.dataset.currency;
      dashboard.querySelectorAll("[data-currency]").forEach((choice) => {
        choice.setAttribute("aria-pressed", String(choice === button));
      });
      const params = new URLSearchParams();
      const start = selectedValues("start")[0];
      const end = selectedValues("end")[0];
      if (start) params.set("start", start);
      if (end) params.set("end", end);
      params.set("currency", selectedCurrency);
      applyFilters(button, params).catch(() => {});
    } else if (action === "toggle-filters" && filterPanel) {
      filterPanel.hidden = !filterPanel.hidden;
      button.setAttribute("aria-expanded", String(!filterPanel.hidden));
      if (!filterPanel.hidden) filterPanel.querySelector("input, select")?.focus();
    } else if (action === "apply-filters") {
      event.preventDefault();
      applyFilters(button).catch(() => {});
    } else if (action === "close-drawer") {
      closeDrawer();
    } else if (action === "load-more") {
      loadMore(button).catch(() => {});
    } else if (action === "open-drawer") {
      if (button.dataset.detail) {
        openRecord(button).catch(() => {});
      } else if (button.dataset.metric) {
        openDrill(button).catch(() => {});
      }
    }
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && drawer && !drawer.hidden) {
      event.preventDefault();
      closeDrawer();
    }
  });

  drawer?.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeDrawer();
  });

})();
