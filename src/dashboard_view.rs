use crate::dashboard_finance::{ClosingPosition, DashboardAlert, DashboardResponse, Measures};
use std::fmt::Write;

pub(crate) fn render(snapshot: &DashboardResponse) -> String {
    let mut html = String::with_capacity(32 * 1024);
    html.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Tracky Monthly ledger</title><link rel=\"stylesheet\" href=\"app.css\"></head><body data-dashboard>");
    render_header(&mut html, snapshot);
    html.push_str("<main id=\"ledger\">");
    render_currency(&mut html, snapshot);
    render_filters(&mut html, snapshot);

    if let Some(currency) = snapshot.filters.currency.as_deref() {
        render_summary(&mut html, &snapshot.summary, currency);
        render_monthly(&mut html, snapshot, currency);
        render_categories(&mut html, snapshot, currency);
        render_accounts(&mut html, snapshot, currency);
        render_alerts(&mut html, snapshot);
        render_investments(&mut html, snapshot);
    } else {
        html.push_str("<section class=\"empty-state\" data-region=\"summary\"><p class=\"eyebrow\">Valid empty ledger</p><h2>No currency activity</h2><p>Add canonical activity with the Tracky CLI, then reopen the dashboard.</p></section><div data-region=\"monthly\"></div><div data-region=\"categories\"></div><div data-region=\"accounts\"></div><div data-region=\"alerts\"></div><div data-region=\"investments\"></div>");
    }

    html.push_str("<p class=\"immutable-note\">This view is read-only. Use the Tracky CLI to review or correct canonical data.</p></main>");
    html.push_str("<div class=\"drawer-scrim\" data-action=\"close-drawer\" hidden></div><dialog class=\"drawer\" data-drawer aria-label=\"Read-only canonical drawer\"><div class=\"drawer-actions\"><button type=\"button\" data-action=\"refresh\">Refresh</button><button class=\"drawer-close\" type=\"button\" data-action=\"close-drawer\" aria-label=\"Close canonical drawer\">Close</button></div><div data-drawer-content><p class=\"eyebrow\">Canonical detail</p><h2>Read-only rows</h2></div></dialog>");
    html.push_str("<noscript><p class=\"noscript\">JavaScript is optional. Exact tables, periods, currencies, freshness, and alerts remain available above; filters and the canonical drawer require enhancement.</p></noscript><script src=\"app.js\"></script></body></html>");
    html
}

fn render_header(html: &mut String, snapshot: &DashboardResponse) {
    let accounts = scope_names(
        &snapshot.filters.account_ids,
        snapshot
            .dimensions
            .accounts
            .iter()
            .map(|account| (&account.id, &account.name)),
        "No accounts selected",
    );
    let categories = scope_names(
        &snapshot.filters.category_ids,
        snapshot
            .dimensions
            .categories
            .iter()
            .map(|category| (&category.id, &category.name)),
        "No expense categories selected",
    );
    write!(
        html,
        "<header class=\"masthead\"><div data-region=\"scope\"><p class=\"eyebrow\">Local analytical ledger · read-only</p><h1>Monthly ledger</h1><p class=\"scope\"><time>{}</time> through <time>{}</time> · Accounts: {} · Expense categories: {} · read {}</p></div><div class=\"masthead-actions\"><div class=\"refresh-status\" data-refresh-status role=\"status\" aria-live=\"polite\" aria-atomic=\"true\">Snapshot ready.</div><button type=\"button\" data-action=\"refresh\">Refresh</button><button type=\"button\" data-action=\"toggle-filters\" aria-expanded=\"false\">Filters</button></div></header>",
        escape(&snapshot.filters.start_date),
        escape(&snapshot.filters.end_date),
        escape(&accounts),
        escape(&categories),
        escape(&snapshot.read_at)
    )
    .expect("writing to a String cannot fail");
}

fn scope_names<'a>(
    selected: &[String],
    dimensions: impl Iterator<Item = (&'a String, &'a String)>,
    empty: &str,
) -> String {
    if selected.is_empty() {
        return empty.to_string();
    }
    let names = dimensions
        .filter(|(id, _)| selected.contains(id))
        .map(|(_, name)| name.as_str())
        .collect::<Vec<_>>();
    if names.is_empty() {
        selected.join(", ")
    } else {
        names.join(", ")
    }
}

fn render_currency(html: &mut String, snapshot: &DashboardResponse) {
    html.push_str("<nav class=\"currency-rail\" data-region=\"currency\" aria-label=\"Currency selector\"><span>Native-currency ledger</span><div>");
    for currency in &snapshot.dimensions.currencies {
        let selected = snapshot.filters.currency.as_deref() == Some(currency.as_str());
        write!(
            html,
            "<button type=\"button\" data-currency=\"{}\" aria-pressed=\"{}\">{}</button>",
            escape(currency),
            selected,
            escape(currency)
        )
        .expect("writing to a String cannot fail");
    }
    html.push_str("</div></nav>");
}

fn render_filters(html: &mut String, snapshot: &DashboardResponse) {
    html.push_str("<section class=\"filter-panel\" data-filter-panel hidden><div aria-label=\"Ledger filters\">");
    write!(html, "<label>From<input name=\"start\" type=\"date\" value=\"{}\"></label><label>Through<input name=\"end\" type=\"date\" value=\"{}\"></label>", escape(&snapshot.filters.start_date), escape(&snapshot.filters.end_date)).expect("writing to a String cannot fail");
    html.push_str("<fieldset><legend>Accounts</legend>");
    for account in &snapshot.dimensions.accounts {
        let checked = snapshot.filters.account_ids.contains(&account.id);
        write!(html, "<label data-compatible-currency=\"{}\"><input name=\"account\" type=\"checkbox\" value=\"{}\" {}> {} <small>{}</small></label>", escape(&account.currency), escape(&account.id), checked_attr(checked), escape(&account.name), escape(&account.currency)).expect("writing to a String cannot fail");
    }
    html.push_str("</fieldset><fieldset><legend>Expense categories <small>affect expense measures only</small></legend>");
    for category in &snapshot.dimensions.categories {
        let checked = snapshot.filters.category_ids.contains(&category.id);
        let currencies = category.currencies.join(" ");
        write!(html, "<label data-compatible-currency=\"{}\"><input name=\"category\" type=\"checkbox\" value=\"{}\" {}> {}</label>", escape(&currencies), escape(&category.id), checked_attr(checked), escape(&category.name)).expect("writing to a String cannot fail");
    }
    html.push_str("</fieldset><button class=\"primary\" type=\"button\" data-action=\"apply-filters\">Apply filters</button></div></section>");
}

fn checked_attr(checked: bool) -> &'static str {
    if checked {
        "checked"
    } else {
        ""
    }
}

fn render_summary(html: &mut String, measures: &Measures, currency: &str) {
    html.push_str("<section data-region=\"summary\" aria-labelledby=\"summary-heading\"><div class=\"section-heading\"><p class=\"eyebrow\">Opening position</p><h2 id=\"summary-heading\">Four measures, one currency</h2></div><ul class=\"summary-grid\">");
    summary_item(
        html,
        "Income",
        "income",
        &measures.income_minor,
        currency,
        "Canonical inflows",
    );
    summary_item(
        html,
        "Consumption expense",
        "consumption_expense",
        &measures.consumption_expense_minor,
        currency,
        "Consumption only",
    );
    summary_item(
        html,
        "Savings / net cash flow",
        "net_cash_flow",
        &measures.net_cash_flow_minor,
        currency,
        "Income minus consumption expense",
    );
    summary_item(
        html,
        "Investment contributions",
        "investment_contribution",
        &measures.investment_contribution_minor,
        currency,
        "Separate from expense",
    );
    html.push_str("</ul></section>");
}

fn summary_item(
    html: &mut String,
    label: &str,
    metric: &str,
    value: &str,
    currency: &str,
    note: &str,
) {
    write!(html, "<li><button type=\"button\" data-action=\"open-drawer\" data-metric=\"{metric}\"><span>{label}</span><strong data-minor=\"{}\">{} {}</strong><small>{note}</small></button></li>", escape(value), escape(value), escape(currency)).expect("writing to a String cannot fail");
}

fn render_monthly(html: &mut String, snapshot: &DashboardResponse, currency: &str) {
    html.push_str("<section data-region=\"monthly\" aria-labelledby=\"monthly-heading\"><div class=\"section-heading\"><p class=\"eyebrow\">Flow over time</p><h2 id=\"monthly-heading\">Monthly activity</h2><p id=\"trend-description\" class=\"visually-hidden\">Each month shows exact income and consumption expense amounts. The exact table follows the chart.</p></div><div class=\"trend\" role=\"group\" aria-label=\"Monthly income and consumption expense trend\" aria-describedby=\"trend-description\">");
    for month in &snapshot.monthly {
        write!(html, "<button type=\"button\" data-action=\"open-drawer\" data-metric=\"activity\" data-month=\"{}\"><span>{}</span><span class=\"trend-income\">Income <b>{} {}</b></span><span class=\"trend-expense\">Expense <b>{} {}</b></span></button>", escape(&month.month), escape(&month.month), escape(&month.measures.income_minor), escape(currency), escape(&month.measures.consumption_expense_minor), escape(currency)).expect("writing to a String cannot fail");
    }
    html.push_str("</div><div class=\"table-scroll\" tabindex=\"0\" role=\"region\" aria-label=\"Exact monthly amounts by month and currency; horizontally scrollable when needed\"><table><caption>Exact monthly amounts in minor units</caption><thead><tr><th scope=\"col\">Month</th><th scope=\"col\">Income</th><th scope=\"col\">Consumption expense</th><th scope=\"col\">Savings</th><th scope=\"col\">Investment contributions</th></tr></thead><tbody>");
    for month in &snapshot.monthly {
        write!(
            html,
            "<tr><th scope=\"row\">{}</th>{}{}{}{}</tr>",
            escape(&month.month),
            amount_cell(&month.measures.income_minor, currency),
            amount_cell(&month.measures.consumption_expense_minor, currency),
            amount_cell(&month.measures.net_cash_flow_minor, currency),
            amount_cell(&month.measures.investment_contribution_minor, currency)
        )
        .expect("writing to a String cannot fail");
    }
    html.push_str("</tbody></table></div></section>");
}

fn amount_cell(value: &str, currency: &str) -> String {
    format!(
        "<td data-minor=\"{}\">{} {}</td>",
        escape(value),
        escape(value),
        escape(currency)
    )
}

fn render_categories(html: &mut String, snapshot: &DashboardResponse, currency: &str) {
    html.push_str("<section data-region=\"categories\" aria-labelledby=\"categories-heading\"><div class=\"section-heading\"><p class=\"eyebrow\">Consumption map</p><h2 id=\"categories-heading\">Expense categories</h2></div><ul class=\"breakdown-list\">");
    for category in &snapshot.categories {
        write!(html, "<li><button type=\"button\" data-action=\"open-drawer\" data-metric=\"consumption_expense\" data-category=\"{}\"><span>{}</span><strong data-minor=\"{}\">{} {}</strong></button></li>", escape(&category.category_id), escape(&category.category_name), escape(&category.amount_minor), escape(&category.amount_minor), escape(currency)).expect("writing to a String cannot fail");
    }
    empty_list(
        html,
        snapshot.categories.is_empty(),
        "No expense categories in this scope.",
    );
    html.push_str("</ul></section>");
}

fn render_accounts(html: &mut String, snapshot: &DashboardResponse, currency: &str) {
    html.push_str("<section data-region=\"accounts\" aria-labelledby=\"accounts-heading\"><div class=\"section-heading\"><p class=\"eyebrow\">Source activity</p><h2 id=\"accounts-heading\">Accounts</h2></div><ul class=\"breakdown-list\">");
    for account in &snapshot.accounts {
        write!(html, "<li><button type=\"button\" data-action=\"open-drawer\" data-metric=\"activity\" data-account=\"{}\"><span>{}<small>{} canonical rows</small></span><strong data-minor=\"{}\">{} {}</strong></button></li>", escape(&account.account_id), escape(&account.account_name), account.row_count, escape(&account.measures.net_cash_flow_minor), escape(&account.measures.net_cash_flow_minor), escape(currency)).expect("writing to a String cannot fail");
    }
    empty_list(
        html,
        snapshot.accounts.is_empty(),
        "No account activity in this scope.",
    );
    html.push_str("</ul></section>");
}

fn render_alerts(html: &mut String, snapshot: &DashboardResponse) {
    html.push_str("<section data-region=\"alerts\" aria-labelledby=\"alerts-heading\"><div class=\"section-heading\"><p class=\"eyebrow\">Context, not command</p><h2 id=\"alerts-heading\">Freshness and reconciliation alerts</h2></div><ul class=\"alert-list\">");
    for (index, alert) in snapshot.alerts.iter().enumerate() {
        let position_index = snapshot
            .investments
            .closing_positions
            .iter()
            .position(|position| {
                alert.account_id.as_ref() == Some(&position.account_id)
                    && (alert.instrument_id.is_none()
                        || alert.instrument_id == position.instrument_id)
            });
        render_alert(html, index, position_index, alert);
    }
    empty_list(
        html,
        snapshot.alerts.is_empty(),
        "No contextual alerts in this scope.",
    );
    html.push_str("</ul></section>");
}

fn render_alert(
    html: &mut String,
    index: usize,
    position_index: Option<usize>,
    alert: &DashboardAlert,
) {
    let subject = alert
        .instrument_id
        .as_deref()
        .or(alert.account_id.as_deref())
        .unwrap_or("Ledger");
    let mut details = vec![alert.currency.clone()];
    if let Some(date) = &alert.effective_date {
        details.push(format!("effective {date}"));
    }
    if let Some(observed_at) = &alert.observed_at {
        details.push(format!("observed {observed_at}"));
    }
    if let Some(age_days) = alert.age_days {
        details.push(format!("age {age_days} days"));
    }
    if let Some(amount) = &alert.pending_amount_minor {
        details.push(format!("pending {amount} {}", alert.currency));
    }
    if let Some(difference) = &alert.quantity_difference {
        details.push(format!("quantity difference {difference}"));
    }
    if let Some(difference) = &alert.value_difference_minor {
        details.push(format!("value difference {difference} {}", alert.currency));
    }
    let account = alert
        .account_id
        .as_deref()
        .map(|account| format!(" data-account=\"{}\"", escape(account)))
        .unwrap_or_default();
    let affected_position = position_index
        .map(|position| format!(" data-position-index=\"{position}\""))
        .unwrap_or_default();
    let instrument = alert
        .instrument_id
        .as_deref()
        .map(|instrument| format!(" data-instrument=\"{}\"", escape(instrument)))
        .unwrap_or_default();
    write!(html, "<li><button type=\"button\" data-action=\"open-drawer\" data-detail=\"alert\" data-record-id=\"{}\" data-index=\"{index}\" data-metric=\"investment_contribution\"{account}{instrument}{affected_position}><span class=\"status status-{}\">{}</span><strong>{}</strong><small>{}</small></button></li>", escape(&alert.id), escape(&alert.severity), escape(&alert.kind.replace('_', " ")), escape(subject), escape(&details.join(" · "))).expect("writing to a String cannot fail");
}

fn render_investments(html: &mut String, snapshot: &DashboardResponse) {
    write!(html, "<section data-region=\"investments\" aria-labelledby=\"investments-heading\"><div class=\"section-heading\"><p class=\"eyebrow\">As-of closing state</p><h2 id=\"investments-heading\">Investment positions</h2><p class=\"meta\">State: {} · Pending allocation: {} {}</p></div><div class=\"table-scroll\"><table class=\"positions\"><caption>Exact quantity, cost, valuation, freshness, and reconciliation</caption><thead><tr><th scope=\"col\">Position</th><th scope=\"col\">Quantity</th><th scope=\"col\">Historical cost</th><th scope=\"col\">Observed value</th><th scope=\"col\">Effective date</th><th scope=\"col\">Freshness</th><th scope=\"col\">Reconciliation</th></tr></thead><tbody>", escape(snapshot.investments.state), escape(&snapshot.investments.pending_allocation_minor), snapshot.filters.currency.as_deref().map(escape).unwrap_or_default()).expect("writing to a String cannot fail");
    for (index, position) in snapshot.investments.closing_positions.iter().enumerate() {
        let instrument_name = position.instrument_id.as_ref().and_then(|id| {
            snapshot
                .dimensions
                .instruments
                .iter()
                .find(|instrument| &instrument.id == id)
                .map(|instrument| instrument.name.as_str())
        });
        render_position(html, index, instrument_name, position);
    }
    if snapshot.investments.closing_positions.is_empty() {
        html.push_str("<tr><td colspan=\"7\">No investment positions in this scope.</td></tr>");
    }
    html.push_str("</tbody></table></div></section>");
}

fn render_position(
    html: &mut String,
    index: usize,
    instrument_name: Option<&str>,
    position: &ClosingPosition,
) {
    let instrument = instrument_name
        .or(position.instrument_id.as_deref())
        .unwrap_or("Pending allocation");
    let quantity = position.quantity.as_deref().unwrap_or("Unavailable");
    let cost = paired(
        position.historical_cost_minor.as_deref(),
        position.cost_currency.as_deref(),
    );
    let valuation = paired(
        position.observed_value_minor.as_deref(),
        position.valuation_currency.as_deref(),
    );
    let instrument_id = position.instrument_id.as_deref().unwrap_or_default();
    write!(html, "<tr><th scope=\"row\"><button type=\"button\" data-action=\"open-drawer\" data-detail=\"position\" data-index=\"{index}\" data-metric=\"investment_contribution\" data-account=\"{}\" data-instrument=\"{}\">{}<small>{}</small></button></th><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><span class=\"status status-{}\">{}</span></td><td>{}</td></tr>", escape(&position.account_id), escape(instrument_id), escape(instrument), escape(&position.account_id), escape(quantity), escape(&cost), escape(&valuation), position.effective_date.as_deref().map(escape).unwrap_or_else(|| "Unavailable".into()), escape(&position.freshness), escape(&position.freshness), escape(&position.reconciliation_status)).expect("writing to a String cannot fail");
}

fn paired(value: Option<&str>, currency: Option<&str>) -> String {
    match (value, currency) {
        (Some(value), Some(currency)) => format!("{value} {currency}"),
        _ => "Unavailable".to_string(),
    }
}

fn empty_list(html: &mut String, empty: bool, message: &str) {
    if empty {
        write!(html, "<li class=\"empty-row\">{}</li>", escape(message))
            .expect("writing to a String cannot fail");
    }
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}
