(() => {
  "use strict";

  const dashboard = document.querySelector("[data-dashboard]");
  if (!dashboard) return;

  const filterPanel = dashboard.querySelector("[data-filter-panel]");
  const drawer = document.querySelector("[data-drawer]");
  const drawerContent = drawer?.querySelector("[data-drawer-content]");
  const refreshStatus = dashboard.querySelector("[data-refresh-status]");
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

  const announce = (message, state = "ready") => {
    if (!refreshStatus) return;
    refreshStatus.dataset.state = state;
    refreshStatus.replaceChildren(document.createTextNode(message));
    if (state === "stale") {
      const retry = element("button", "Retry");
      retry.type = "button";
      retry.dataset.action = "refresh";
      refreshStatus.append(" ", retry);
    }
  };

  const exact = (value) => {
    if (value === null) return "null";
    if (value === undefined) return "";
    if (typeof value === "object") return JSON.stringify(value);
    return String(value);
  };

  const amount = (value, currency) => {
    const raw = exact(value);
    const code = exact(currency);
    if (!["COP", "USD"].includes(code) || !/^-?\d+$/.test(raw)) {
      return code ? `${raw} ${code}` : raw;
    }
    const sign = raw.startsWith("-") ? "-" : "";
    const digits = (sign ? raw.slice(1) : raw).padStart(3, "0");
    const major = digits.slice(0, -2);
    const fraction = digits.slice(-2);
    const group = code === "COP" ? "." : ",";
    const decimal = code === "COP" ? "," : ".";
    const grouped = major.replace(/\B(?=(\d{3})+(?!\d))/g, group);
    return `${code}\u00a0${sign}$${grouped}${decimal}${fraction}`;
  };

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
    const description = element("p", "Each month shows exact income and consumption expense amounts. The exact table follows the chart.", "visually-hidden");
    description.id = "trend-description";
    parent.append(description);
    const trend = element("div", undefined, "trend");
    trend.setAttribute("role", "group");
    trend.setAttribute("aria-label", "Monthly income and consumption expense trend");
    trend.setAttribute("aria-describedby", "trend-description");
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
    scroll.tabIndex = 0;
    scroll.setAttribute("role", "region");
    scroll.setAttribute("aria-label", "Exact monthly amounts by month and currency; horizontally scrollable when needed");
    const table = element("table");
    table.append(element("caption", "Exact monthly amounts"));
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
        recordId: alert.id,
        index,
        metric: "investment_contribution",
        account: alert.account_id,
        instrument: alert.instrument_id,
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
        instrument: position.instrument_id,
      }));
      list.append(item);
    });
    if (list.children.length) parent.append(list);
    else if (!data.investments.flows.length) empty(parent);
  };

  const renderValidEmpty = () => {
    const summary = region("summary");
    summary?.replaceChildren(
      element("p", "Valid empty ledger", "eyebrow"),
      element("h2", "No currency activity"),
      element("p", "Add canonical activity with the Tracky CLI, then refresh the dashboard.", "empty"),
    );
    ["monthly", "categories", "accounts", "alerts", "investments"].forEach((name) => {
      region(name)?.replaceChildren();
    });
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
    if (data.state === "empty") {
      renderValidEmpty();
      syncFilterControls(data);
      return;
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
      announce("Filters applied. Dashboard updated.");
    } catch (_) {
      announce("Dashboard could not apply those filters. The previous results remain available.", "stale");
    } finally {
      requestInFlight = false;
      trigger.disabled = false;
      dashboard.removeAttribute("aria-busy");
    }
  };

  const refreshSnapshot = async (trigger) => {
    if (requestInFlight) return;
    requestInFlight = true;
    trigger.disabled = true;
    dashboard.setAttribute("aria-busy", "true");
    announce("Refreshing dashboard…", "loading");
    const openDrawerTrigger = drawer && drawer.open ? returnFocus : null;
    let refreshedTrigger = null;
    try {
      const data = await fetchJson("api/v1/dashboard/refresh", filterParams());
      if (openDrawerTrigger) closeDrawer(false);
      renderSnapshot(data);
      if (openDrawerTrigger) refreshedTrigger = matchingDrawerTrigger(openDrawerTrigger);
      announce("Dashboard refreshed. Updated snapshot is ready.");
      if (!refreshedTrigger) dashboard.querySelector('.masthead [data-action="refresh"]')?.focus();
    } catch (_) {
      announce("Refresh failed. Showing the last good snapshot. Retry when ready.", "stale");
      if (openDrawerTrigger && drawer?.open && drawerContent) {
        drawerContent.querySelector("[data-drawer-refresh-error]")?.remove();
        const error = element("div", undefined, "drawer-refresh-error");
        error.dataset.drawerRefreshError = "";
        error.setAttribute("role", "status");
        error.setAttribute("aria-live", "polite");
        error.append(element("p", "Refresh failed. Showing the last good detail."));
        const retry = element("button", "Retry");
        retry.type = "button";
        retry.dataset.action = "refresh";
        error.append(retry);
        drawerContent.prepend(error);
        retry.focus();
      } else {
        refreshStatus?.querySelector('[data-action="refresh"]')?.focus();
      }
    } finally {
      requestInFlight = false;
      trigger.disabled = false;
      dashboard.removeAttribute("aria-busy");
    }
    if (refreshedTrigger) {
      if (refreshedTrigger.dataset.detail) await openRecord(refreshedTrigger);
      else await openDrill(refreshedTrigger);
      announce("Dashboard refreshed. Updated snapshot is ready.");
    }
  };

  const matchingDrawerTrigger = (previous) => {
    const keys = previous.dataset.detail
      ? ["detail", "recordId", "account", "instrument"]
      : ["metric", "month", "account", "category"];
    return Array.from(dashboard.querySelectorAll('[data-action="open-drawer"]')).find((candidate) =>
      keys.every((key) => (previous.dataset[key] || "") === (candidate.dataset[key] || ""))) || null;
  };

  const detailValue = (key, value, record) => {
    if (!key.endsWith("_minor")) return exact(value);
    const currency = key === "historical_cost_minor"
      ? record.cost_currency
      : key === "observed_value_minor"
        ? record.valuation_currency
        : record.currency;
    return amount(value, currency);
  };

  const detailList = (record) => {
    const list = element("dl");
    Object.entries(record).forEach(([key, value]) => {
      list.append(element("dt", key), element("dd", detailValue(key, value, record)));
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
    document.querySelector(".drawer-scrim")?.removeAttribute("hidden");
    if (typeof drawer.showModal === "function" && !drawer.open) drawer.showModal();
    drawer.querySelector('[data-action="close-drawer"]')?.focus();
  };

  const closeDrawer = (restoreFocus = true) => {
    if (!drawer || !drawer.open) return;
    if (typeof drawer.close === "function" && drawer.open) drawer.close();
    document.querySelector(".drawer-scrim")?.setAttribute("hidden", "");
    if (restoreFocus) returnFocus?.focus();
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
    scroll.tabIndex = 0;
    scroll.setAttribute("role", "region");
    scroll.setAttribute("aria-label", "Canonical rows; horizontally scrollable when needed");
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
      announce("Canonical detail opened.");
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
      announce("Canonical detail opened.");
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
      announce(nextCursor ? "More canonical rows loaded." : "All canonical rows loaded.");
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
    } else if (action === "refresh") {
      event.preventDefault();
      refreshSnapshot(button).catch(() => {});
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
    if (event.key === "Tab" && drawer?.open) {
      const focusable = Array.from(drawer.querySelectorAll("button, input, select, textarea, [tabindex]:not([tabindex='-1'])"))
        .filter((node) => !node.disabled && node.getClientRects().length > 0);
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (first && last && (event.shiftKey ? document.activeElement === first : document.activeElement === last)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      }
    }
    if (event.key === "Escape" && drawer?.open) {
      event.preventDefault();
      closeDrawer();
    }
  });

  drawer?.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeDrawer();
  });

})();
