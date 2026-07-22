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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/tracky"))
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

        dashboard = subprocess.Popen(
            [
                binary,
                "dashboard",
                "--db",
                str(database),
                "--start-date",
                "2026-01-01",
                "--end-date",
                "2026-07-31",
                "--currency",
                "COP",
                "--no-open",
            ],
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            deadline = time.monotonic() + 10
            url = None
            while time.monotonic() < deadline:
                line = dashboard.stdout.readline()
                if "Dashboard ready: " in line:
                    url = line.split("Dashboard ready: ", 1)[1].strip()
                    break
                if dashboard.poll() is not None:
                    break
            if not url:
                raise RuntimeError(f"dashboard did not become ready: {dashboard.stderr.read()}")

            run(
                ["playwright-cli", f"-s={session}", "open", url, "--browser=chrome"],
                env,
                cwd=root,
                capture_output=True,
            )
            flow = r'''async page => {
              const fail = message => { throw new Error(message); };
              const initialUrl = page.url();
              const initialHistoryLength = await page.evaluate(() => history.length);
              const expectedOrder = ["scope", "currency", "summary", "monthly", "categories", "accounts", "alerts", "investments"];
              const order = await page.locator("[data-region]").evaluateAll(nodes => nodes.map(node => node.dataset.region));
              if (JSON.stringify(order) !== JSON.stringify(expectedOrder)) fail(`wrong region order: ${order}`);
              if (!await page.getByText("500000 COP", { exact: true }).first().isVisible()) fail("missing exact SSR amount");

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
              if (page.url() !== initialUrl) fail("filters changed the page URL");
              const interactionMs = Number(await page.locator("[data-dashboard]").getAttribute("data-last-interaction-ms"));
              if (!(interactionMs <= 100)) fail(`local filter interaction exceeded 100ms: ${interactionMs}`);
              if (await page.evaluate(() => localStorage.length || sessionStorage.length || document.cookie.length)) fail("browser state persisted");

              await page.reload();
              await page.getByText("500000 COP", { exact: true }).first().waitFor();
              const opener = page.locator('[data-region="summary"] [data-metric="income"]');
              await opener.click();
              await page.getByRole("dialog", { name: "Read-only canonical drawer" }).waitFor();
              if (!await page.getByText("Synthetic monthly income", { exact: true }).isVisible()) fail("drawer omitted canonical row");
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
              if (await page.evaluate(() => history.length) !== initialHistoryLength) fail("internal actions repurposed browser history");

              await page.setViewportSize({ width: 320, height: 800 });
              const layout = await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth }));
              if (layout.scroll > layout.width) fail(`horizontal overflow at 320px: ${JSON.stringify(layout)}`);
              const external = await page.evaluate(() => performance.getEntriesByType("resource").map(entry => new URL(entry.name).origin).filter(origin => origin !== location.origin));
              if (external.length) fail(`external requests: ${external}`);
              return "monthly-ledger-browser-flow-ok";
            }'''
            result = run(
                ["playwright-cli", f"-s={session}", "run-code", flow],
                env,
                cwd=root,
                capture_output=True,
            )
            if "monthly-ledger-browser-flow-ok" not in result.stdout:
                raise RuntimeError(result.stdout)
        finally:
            subprocess.run(
                ["playwright-cli", f"-s={session}", "close"],
                env=env,
                cwd=root,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if dashboard.poll() is None:
                dashboard.send_signal(signal.SIGTERM)
            try:
                dashboard.wait(timeout=3)
            except subprocess.TimeoutExpired:
                dashboard.kill()
                dashboard.wait()
            if dashboard.returncode != 0:
                raise RuntimeError(f"dashboard exited {dashboard.returncode}: {dashboard.stderr.read()}")


if __name__ == "__main__":
    main()
