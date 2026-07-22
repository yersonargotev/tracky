#!/usr/bin/env python3
import importlib.util
import sqlite3
import stat
import tempfile
import time
import unittest
from pathlib import Path

MODULE = Path(__file__).parents[1] / "scripts" / "dashboard_candidate_runtime.py"
spec = importlib.util.spec_from_file_location("runtime", MODULE)
runtime = importlib.util.module_from_spec(spec); spec.loader.exec_module(runtime)


class RuntimeHarnessTests(unittest.TestCase):
    def test_p95_uses_nearest_rank(self):
        self.assertEqual(runtime.p95(range(1, 101)), 95)
        self.assertEqual(runtime.p95(range(30)), 28)
        with self.assertRaises(ValueError): runtime.p95([])

    def test_fixture_cardinality_and_month_span(self):
        with tempfile.TemporaryDirectory() as directory:
            connection = sqlite3.connect(Path(directory) / "fixture.sqlite")
            connection.execute("CREATE TABLE canonical_transactions (id TEXT, account_id TEXT, posted_date TEXT, description TEXT, amount_minor INTEGER, currency TEXT, transaction_kind TEXT)")
            runtime.seed_fixture(connection, "account")
            self.assertEqual(connection.execute("SELECT count(*) FROM canonical_transactions").fetchone()[0], 100_000)
            self.assertEqual(connection.execute("SELECT min(substr(posted_date,1,7)), max(substr(posted_date,1,7)), count(DISTINCT substr(posted_date,1,7)) FROM canonical_transactions").fetchone(), ("2016-01", "2025-12", 120))

    def test_capability_url_parse_and_redaction(self):
        capability = "a" * 64
        url = runtime.parse_capability_url(f"Dashboard ready: http://127.0.0.1:1234/c/{capability}/\n")
        self.assertEqual(runtime.redact_url(url), "http://127.0.0.1:1234/c/<redacted>/")
        for invalid in ("http://localhost:1/c/" + capability + "/", "http://127.0.0.1:1/c/secret/"):
            with self.assertRaises(ValueError): runtime.parse_capability_url(invalid)

    def test_budget_acceptance_and_rejection(self):
        latency = {name: limit for name, limit in runtime.LATENCY_LIMITS.items()}
        resources = {name: limit for name, limit in runtime.RESOURCE_LIMITS.items()}
        resources.update(descriptor_growth=0, memory_growth_bytes=8 << 20, memory_growth_percent=99)
        runtime.check_budgets(latency, resources)
        latency["refresh_p95_ms"] += .001
        with self.assertRaisesRegex(RuntimeError, "refresh_p95_ms"): runtime.check_budgets(latency, resources)

    def test_readiness_timeout_does_not_block_on_missing_newline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "silent"
            binary.write_text("#!/bin/sh\nsleep 5\n", encoding="utf-8")
            binary.chmod(binary.stat().st_mode | stat.S_IXUSR)
            started = time.monotonic()
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                runtime.start_dashboard(binary, root / "unused.sqlite", {}, timeout=0.1)
            self.assertLess(time.monotonic() - started, 1)


if __name__ == "__main__": unittest.main()
