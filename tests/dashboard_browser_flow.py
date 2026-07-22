#!/usr/bin/env python3
"""Real-browser acceptance flow for Tracky's framework-free Monthly ledger."""

import argparse
import json
import os
import signal
import sqlite3
import subprocess
import tempfile
import time
from pathlib import Path


def run(command, env, **kwargs):
    return subprocess.run(command, env=env, check=True, text=True, **kwargs)


def response(binary, env, *arguments):
    result = run([binary, *arguments, "--json"], env, capture_output=True)
    return json.loads(result.stdout)


def run_script(session, script, sentinel, env, cwd):
    result = run(
        ["playwright-cli", f"-s={session}", "run-code", script],
        env,
        cwd=cwd,
        capture_output=True,
    )
    if sentinel not in result.stdout:
        raise RuntimeError(result.stdout)


def open_session(session, url, browser, env, cwd):
    run(
        ["playwright-cli", f"-s={session}", "open", url, f"--browser={browser}"],
        env,
        cwd=cwd,
        capture_output=True,
    )


def close_session(session, env, cwd):
    subprocess.run(
        ["playwright-cli", f"-s={session}", "close"],
        env=env,
        cwd=cwd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


class DashboardProcess:
    def __init__(self, binary, env, database, start, end, currency=None):
        command = [
            binary,
            "dashboard",
            "--db",
            str(database),
            "--start-date",
            start,
            "--end-date",
            end,
        ]
        if currency:
            command.extend(["--currency", currency])
        command.append("--no-open")
        self.child = subprocess.Popen(
            command,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        deadline = time.monotonic() + 10
        self.url = None
        while time.monotonic() < deadline:
            line = self.child.stdout.readline()
            if "Dashboard ready: " in line:
                self.url = line.split("Dashboard ready: ", 1)[1].strip()
                break
            if self.child.poll() is not None:
                break
        if not self.url:
            raise RuntimeError(f"dashboard did not become ready: {self.child.stderr.read()}")

    def stop(self):
        if self.child.poll() is None:
            self.child.send_signal(signal.SIGTERM)
        try:
            self.child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.child.kill()
            self.child.wait()
        if self.child.returncode != 0:
            raise RuntimeError(
                f"dashboard exited {self.child.returncode}: {self.child.stderr.read()}"
            )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/tracky"))
    parser.add_argument("--browser", choices=("chrome", "firefox", "webkit"), default="chrome")
    arguments = parser.parse_args()
    binary = str(arguments.binary.resolve())
    session = f"tracky-browser-{os.getpid()}"

    with tempfile.TemporaryDirectory(prefix="tracky-browser-") as directory:
        root = Path(directory)
        database = root / "ledger.sqlite"
        env = os.environ.copy()
        env.update({"HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config")})
        Path(env["HOME"]).mkdir()
        Path(env["XDG_CONFIG_HOME"]).mkdir()

        account = response(
            binary,
            env,
            "accounts",
            "register",
            "--db",
            str(database),
            "--institution",
            "Synthetic Bank",
            "--label",
            "Main account",
            "--account-type",
            "checking",
            "--currency",
            "COP",
        )["account"]["id"]
        usd_account = response(
            binary,
            env,
            "accounts",
            "register",
            "--db",
            str(database),
            "--institution",
            "Synthetic Bank",
            "--label",
            "USD account",
            "--account-type",
            "checking",
            "--currency",
            "USD",
        )["account"]["id"]
        source = response(
            binary,
            env,
            "income-sources",
            "create",
            "--db",
            str(database),
            "--name",
            "Synthetic salary",
        )["income_source"]["id"]
        category = response(
            binary,
            env,
            "categories",
            "create",
            "--db",
            str(database),
            "--name",
            "Food",
        )["category"]["id"]
        response(
            binary,
            env,
            "transactions",
            "add-income",
            "--db",
            str(database),
            "--account-id",
            account,
            "--posted-date",
            "2026-07-01",
            "--description",
            "Synthetic monthly income",
            "--amount-minor",
            "500000",
            "--currency",
            "COP",
            "--income-source-id",
            source,
            "--income-kind",
            "salary",
        )
        response(
            binary,
            env,
            "transactions",
            "add-income",
            "--db",
            str(database),
            "--account-id",
            usd_account,
            "--posted-date",
            "2026-07-02",
            "--description",
            "Synthetic USD income",
            "--amount-minor",
            "10000",
            "--currency",
            "USD",
            "--income-source-id",
            source,
            "--income-kind",
            "salary",
        )
        response(
            binary,
            env,
            "transactions",
            "add-expense",
            "--db",
            str(database),
            "--account-id",
            account,
            "--posted-date",
            "2026-07-10",
            "--description",
            "Synthetic food expense",
            "--amount-minor=-170000",
            "--currency",
            "COP",
            "--category-id",
            category,
        )
        connection = sqlite3.connect(database)
        connection.executescript(
            (Path(__file__).parent / "fixtures" / "dashboard" / "seeds" / "investment.sql").read_text()
        )
        connection.executemany(
            "INSERT INTO canonical_transactions(id, account_id, posted_date, description, amount_minor, currency, transaction_kind) VALUES (?, 'broker-cop', '2026-01-13', ?, -1000, 'COP', 'investment_contribution')",
            [(f"pending-{index:02}", f"Pending contribution {index:02}") for index in range(54)],
        )
        connection.commit()
        connection.close()

        axe_root = run(["npm", "root", "--global"], env, capture_output=True).stdout.strip()
        axe_path = Path(axe_root) / "axe-core" / "axe.min.js"
        if not axe_path.is_file():
            raise RuntimeError(f"pinned axe-core was not installed at {axe_path}")

        dashboard = DashboardProcess(
            binary, env, database, "2026-01-01", "2026-07-31", "COP"
        )
        try:
            open_session(session, dashboard.url, arguments.browser, env, root)
            axe_init = f'''async page => {{
              await page.addInitScript({{ path: {json.dumps(str(axe_path))} }});
              await page.reload();
              return "dashboard-axe-ready";
            }}'''
            run_script(session, axe_init, "dashboard-axe-ready", env, root)
            flow = r'''async page => {
              const fail = message => { throw new Error(message); };
              const initialUrl = page.url();
              const expectedOrder = ["scope", "currency", "summary", "monthly", "categories", "accounts", "alerts", "investments"];
              const order = await page.locator("[data-region]").evaluateAll(nodes => nodes.map(node => node.dataset.region));
              if (JSON.stringify(order) !== JSON.stringify(expectedOrder)) fail(`wrong region order: ${order}`);
              if (!await page.getByText("500000 COP", { exact: true }).first().isVisible()) fail("missing exact SSR amount");

              const noJsContext = await page.context().browser().newContext({ javaScriptEnabled: false });
              const noJsPage = await noJsContext.newPage();
              await noJsPage.goto(initialUrl);
              if (!await noJsPage.getByText("500000 COP", { exact: true }).first().isVisible()) fail("JavaScript-disabled SSR omitted exact amounts");
              if (!await noJsPage.getByText("2026-01-01", { exact: false }).first().isVisible()) fail("JavaScript-disabled SSR omitted period");
              if (!await noJsPage.getByText("stale", { exact: false }).first().isVisible()) fail("JavaScript-disabled SSR omitted freshness");
              if (!await noJsPage.getByRole("table").first().isVisible()) fail("JavaScript-disabled SSR omitted exact table");
              await noJsContext.close();

              await page.getByRole("button", { name: "USD", exact: true }).click();
              await page.getByText("10000 USD", { exact: true }).first().waitFor();
              if (page.url() !== initialUrl) fail("currency changed the page URL");
              await page.reload();
              await page.getByText("500000 COP", { exact: true }).first().waitFor();

              await page.locator('[data-region="monthly"] [data-month="2026-07"]').click();
              await page.getByRole("dialog", { name: "Read-only canonical drawer" }).waitFor();
              await page.keyboard.press("Escape");
              await page.locator('[data-region="categories"] [data-category]').first().click();
              await page.getByText("Synthetic food expense", { exact: true }).waitFor();
              await page.keyboard.press("Escape");

              await page.getByRole("button", { name: "Filters" }).click();
              await page.locator('[name="account"]:checked').evaluateAll(nodes => nodes.forEach(node => node.click()));
              await page.getByRole("button", { name: "Apply filters" }).click();
              await page.getByRole("heading", { name: "No activity matches these filters" }).waitFor();
              const filterEmptyAxe = await page.evaluate(async () => axe.run(document, { resultTypes: ["violations"] }));
              if (filterEmptyAxe.violations.length) fail(`filter-empty axe violations: ${filterEmptyAxe.violations.map(item => item.id).join(", ")}`);
              if (page.url() !== initialUrl) fail("filters changed the page URL");
              const interactionMs = Number(await page.locator("[data-dashboard]").getAttribute("data-last-interaction-ms"));
              if (!(interactionMs <= 100)) fail(`local filter interaction exceeded 100ms: ${interactionMs}`);
              if (await page.evaluate(() => localStorage.length || sessionStorage.length || document.cookie.length)) fail("browser state persisted");

              await page.reload();
              await page.getByText("500000 COP", { exact: true }).first().waitFor();
              const interactionHistoryLength = await page.evaluate(() => history.length);
              const opener = page.locator('[data-region="summary"] [data-metric="income"]');
              await opener.click();
              await page.getByRole("dialog", { name: "Read-only canonical drawer" }).waitFor();
              if (!await page.getByText("Synthetic monthly income", { exact: true }).isVisible()) fail("drawer omitted canonical row");
              await page.keyboard.press("Tab");
              if (!await page.evaluate(() => document.querySelector("[data-drawer]")?.contains(document.activeElement))) fail("Tab escaped the modal drawer");
              await page.keyboard.press("Escape");
              if (await opener.getAttribute("data-metric") !== await page.evaluate(() => document.activeElement?.dataset.metric)) fail("drawer focus was not restored");

              const alert = page.locator('[data-region="alerts"] [data-detail="alert"]').first();
              await alert.click();
              await page.getByRole("heading", { name: "Affected position" }).waitFor();
              await page.keyboard.press("Escape");
              const position = page.locator('[data-region="investments"] [data-detail="position"]').first();
              await position.click();
              const more = page.getByRole("button", { name: "Load more" });
              await more.waitFor();
              await more.click();
              await more.waitFor({ state: "detached" });
              const relatedRows = await page.locator('[data-drawer-content] tbody tr').count();
              if (relatedRows !== 55) fail(`position pagination returned ${relatedRows} rows`);
              await page.keyboard.press("Escape");
              if (await page.evaluate(() => history.length) !== interactionHistoryLength) fail("internal actions repurposed browser history");

              await page.evaluate(() => document.activeElement?.blur());
              await page.setViewportSize({ width: 320, height: 800 });
              const layout = await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth }));
              if (layout.scroll > layout.width) fail(`horizontal overflow at 320px: ${JSON.stringify(layout)}`);
              const external = await page.evaluate(() => performance.getEntriesByType("resource").map(entry => new URL(entry.name).origin).filter(origin => origin !== location.origin));
              if (external.length) fail(`external requests: ${external}`);
              return "monthly-ledger-browser-flow-ok";
            }'''
            run_script(session, flow, "monthly-ledger-browser-flow-ok", env, root)

            response(
                binary,
                env,
                "transactions",
                "add-income",
                "--db",
                str(database),
                "--account-id",
                account,
                "--posted-date",
                "2026-07-20",
                "--description",
                "Externally added income",
                "--amount-minor",
                "200000",
                "--currency",
                "COP",
                "--income-source-id",
                source,
                "--income-kind",
                "salary",
            )
            refresh_flow = r'''async page => {
              const fail = message => { throw new Error(message); };
              await page.setViewportSize({ width: 1280, height: 900 });
              const initialUrl = page.url();
              const initialHistoryLength = await page.evaluate(() => history.length);
              if (!await page.getByText("500000 COP", { exact: true }).first().isVisible()) fail("snapshot changed without an explicit refresh");
              const refresh = page.getByRole("button", { name: "Refresh", exact: true });
              await page.locator('[data-region="summary"] [data-metric="income"]').click();
              await page.getByRole("dialog", { name: "Read-only canonical drawer" }).waitFor();
              await page.getByRole("dialog", { name: "Read-only canonical drawer" }).getByRole("button", { name: "Refresh", exact: true }).click();
              await page.getByText("700000 COP", { exact: true }).first().waitFor();
              const refreshedDrawer = page.getByRole("dialog", { name: "Read-only canonical drawer" });
              await refreshedDrawer.waitFor();
              if (!await refreshedDrawer.getByText("Externally added income", { exact: true }).isVisible()) fail("successful refresh did not rebuild applicable drawer detail");
              if (!await page.evaluate(() => document.querySelector("[data-drawer]")?.contains(document.activeElement))) fail("rebuilt drawer focus was not contained");
              if (!await page.getByRole("status").getByText("Dashboard refreshed", { exact: false }).isVisible()) fail("refresh success was not announced");
              if (page.url() !== initialUrl || await page.evaluate(() => history.length) !== initialHistoryLength) fail("refresh changed URL or history");
              if (await page.evaluate(() => localStorage.length || sessionStorage.length || document.cookie.length)) fail("refresh persisted browser state");
              await page.keyboard.press("Escape");
              const alerts = page.locator('[data-region="alerts"] [data-detail="alert"]');
              if (await alerts.count() < 2) fail("fixture must expose colliding alerts for stable identity coverage");
              const chosenAlert = alerts.nth(1);
              const alertId = await chosenAlert.getAttribute("data-record-id");
              const siblingId = await alerts.nth(0).getAttribute("data-record-id");
              if (!alertId || alertId === siblingId) fail("alerts lack distinct stable record identities");
              await chosenAlert.click();
              const alertDrawer = page.getByRole("dialog", { name: "Read-only canonical drawer" });
              await alertDrawer.getByText(alertId, { exact: true }).waitFor();
              await alertDrawer.getByRole("button", { name: "Refresh", exact: true }).click();
              await alertDrawer.getByText(alertId, { exact: true }).waitFor();
              if (await alertDrawer.getByText(siblingId, { exact: true }).count()) fail("refresh reopened a sibling alert");
              return "dashboard-refresh-success-ok";
            }'''
            run_script(session, refresh_flow, "dashboard-refresh-success-ok", env, root)

            unavailable_database = root / "ledger-unavailable.sqlite"
            database.replace(unavailable_database)
            failure_flow = r'''async page => {
              const fail = message => { throw new Error(message); };
              const drawer = page.getByRole("dialog", { name: "Read-only canonical drawer" });
              await drawer.getByRole("button", { name: "Refresh", exact: true }).click();
              const status = page.locator("[data-refresh-status]");
              await status.getByText("Refresh failed", { exact: false }).waitFor();
              if (!await page.getByText("700000 COP", { exact: true }).first().isVisible()) fail("failed refresh replaced last-good values");
              if (!await drawer.getByRole("button", { name: "Retry", exact: true }).isVisible()) fail("failed refresh omitted retry path");
              const visible = await page.locator("body").innerText();
              if (/sqlite|ledger\.sqlite|tracky-browser-|capability/i.test(visible)) fail("refresh failure leaked storage or capability detail");
              if (await page.evaluate(() => document.activeElement?.textContent?.trim()) !== "Retry") fail("failed refresh did not leave useful focus");
              if (!await page.evaluate(() => document.querySelector("[data-drawer]")?.contains(document.activeElement))) fail("failed drawer refresh moved focus outside the modal");
              const failureAxe = await page.evaluate(async () => axe.run(document, { resultTypes: ["violations"] }));
              if (failureAxe.violations.length) fail(`refresh-failure axe violations: ${failureAxe.violations.map(item => item.id).join(", ")}`);
              return "dashboard-refresh-failure-ok";
            }'''
            run_script(session, failure_flow, "dashboard-refresh-failure-ok", env, root)
            unavailable_database.replace(database)

            accessibility_flow = f'''async page => {{
              const fail = message => {{ throw new Error(message); }};
              const retry = page.getByRole("dialog", {{ name: "Read-only canonical drawer" }}).getByRole("button", {{ name: "Retry", exact: true }});
              await retry.press("Enter");
              await page.getByRole("status").getByText("Dashboard refreshed", {{ exact: false }}).waitFor();
              const result = await page.evaluate(async () => axe.run(document, {{ resultTypes: ["violations"] }}));
              if (result.violations.length) fail(`axe violations: ${{result.violations.map(item => item.id).join(", ")}}`);
              await page.setViewportSize({{ width: 320, height: 800 }});
              const layout = await page.evaluate(() => ({{ width: innerWidth, scroll: document.documentElement.scrollWidth }}));
              if (layout.scroll > layout.width) fail(`horizontal overflow at 320px: ${{JSON.stringify(layout)}}`);
              const targets = await page.locator('button:visible').evaluateAll(nodes => nodes.filter(node => {{ const box = node.getBoundingClientRect(); return box.width < 24 || box.height < 24; }}).map(node => node.textContent.trim()));
              if (targets.length) fail(`undersized pointer targets: ${{targets}}`);
              await page.emulateMedia({{ reducedMotion: "reduce" }});
              const reduced = await page.locator("button").first().evaluate(node => getComputedStyle(node).transitionDuration);
              if (!reduced.endsWith("s") || Number.parseFloat(reduced) > 0.00001) fail(`reduced-motion CSS was not applied: ${{reduced}}`);
              await page.setViewportSize({{ width: 640, height: 800 }});
              await page.evaluate(() => {{ document.documentElement.style.zoom = "200%"; }});
              const zoomed = await page.evaluate(() => ({{ width: document.documentElement.clientWidth, scroll: document.documentElement.scrollWidth }}));
              if (zoomed.scroll > zoomed.width) fail(`horizontal overflow at 200% zoom: ${{JSON.stringify(zoomed)}}`);
              return "dashboard-accessibility-ok";
            }}'''
            run_script(session, accessibility_flow, "dashboard-accessibility-ok", env, root)
        finally:
            close_session(session, env, root)
            dashboard.stop()

        empty_database = root / "empty-ledger.sqlite"
        response(
            binary, env, "accounts", "register", "--db", str(empty_database),
            "--institution", "Empty Bank", "--label", "Empty account",
            "--account-type", "checking", "--currency", "COP",
        )
        empty_dashboard = DashboardProcess(
            binary, env, empty_database, "2026-01-01", "2026-07-31"
        )
        empty_session = f"{session}-empty"
        try:
            open_session(empty_session, empty_dashboard.url, arguments.browser, env, root)
            empty_flow = f'''async page => {{
              const fail = message => {{ throw new Error(message); }};
              await page.addInitScript({{ path: {json.dumps(str(axe_path))} }});
              await page.reload();
              await page.getByRole("heading", {{ name: "No currency activity" }}).waitFor();
              if (await page.getByText(/Pending allocation:/).count()) fail("valid-empty enhancement invented pending allocation");
              if (await page.locator('[data-region="summary"] [data-minor]').count()) fail("valid-empty enhancement invented zero summary metrics");
              const result = await page.evaluate(async () => axe.run(document, {{ resultTypes: ["violations"] }}));
              if (result.violations.length) fail(`valid-empty axe violations: ${{result.violations.map(item => item.id).join(", ")}}`);
              return "dashboard-valid-empty-ok";
            }}'''
            run_script(empty_session, empty_flow, "dashboard-valid-empty-ok", env, root)
        finally:
            close_session(empty_session, env, root)
            empty_dashboard.stop()


if __name__ == "__main__":
    main()
