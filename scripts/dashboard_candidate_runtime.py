#!/usr/bin/env python3
"""Measure a packaged Tracky dashboard without trusting or modifying its database."""

import argparse
import hashlib
import json
import math
import os
import queue
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import urllib.request
from pathlib import Path

TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu"}
LATENCY_LIMITS = {
    "readiness_p95_ms": 500, "initial_snapshot_p95_ms": 1500,
    "refresh_p95_ms": 1500, "navigation_p95_ms": 2000,
    "drill_down_p95_ms": 250, "filter_interaction_p95_ms": 100,
}
RESOURCE_LIMITS = {"idle_rss_bytes": 64 << 20, "peak_rss_bytes": 128 << 20,
                   "idle_cpu_percent": 1, "threads": 8, "descriptors": 32}
URL_RE = re.compile(r"http://127\.0\.0\.1:\d+/c/[0-9a-f]{64}/")


def p95(values):
    if not values:
        raise ValueError("p95 requires at least one sample")
    ordered = sorted(values)
    return ordered[math.ceil(len(ordered) * .95) - 1]


def parse_capability_url(text):
    match = URL_RE.search(text)
    if not match:
        raise ValueError("dashboard did not print a canonical loopback capability URL")
    return match.group(0)


def redact_url(value):
    return re.sub(r"(/c/)[0-9a-f]{64}", r"\1<redacted>", value)


def seed_fixture(connection, account_id, rows=100_000):
    """Seed exactly *rows* transactions across 120 calendar months."""
    insert = ("INSERT INTO canonical_transactions "
              "(id,account_id,posted_date,description,amount_minor,currency,transaction_kind) "
              "VALUES (?,?,?,?,?,?,?)")
    batch = []
    for index in range(rows):
        month_index = index % 120
        year, month = 2016 + month_index // 12, month_index % 12 + 1
        kind = "income" if index % 5 == 0 else "expense"
        amount = 500_000 if kind == "income" else -(1000 + index % 50_000)
        batch.append((f"runtime-{index:06d}", account_id, f"{year:04d}-{month:02d}-{index % 28 + 1:02d}",
                      "Deterministic runtime fixture", amount, "COP", kind))
        if len(batch) == 5000:
            connection.executemany(insert, batch)
            batch.clear()
    if batch:
        connection.executemany(insert, batch)
    connection.commit()


def check_budgets(latency, resources):
    failures = []
    for name, limit in LATENCY_LIMITS.items():
        if name not in latency or latency[name] > limit:
            failures.append(f"{name}={latency.get(name)!r} exceeds {limit}")
    for name, limit in RESOURCE_LIMITS.items():
        if name not in resources or resources[name] > limit:
            failures.append(f"{name}={resources.get(name)!r} exceeds {limit}")
    if resources.get("descriptor_growth", 1) > 0:
        failures.append(f"descriptor_growth={resources.get('descriptor_growth')!r} exceeds 0")
    if not (resources.get("memory_growth_bytes", 1 << 60) <= 8 << 20 or
            resources.get("memory_growth_percent", 1 << 60) <= 5):
        failures.append(
            "memory growth exceeds budget "
            f"(bytes={resources.get('memory_growth_bytes')!r}, "
            f"percent={resources.get('memory_growth_percent')!r})"
        )
    if failures:
        raise RuntimeError("; ".join(failures))


def file_state(db):
    result = {}
    for path in (db, Path(str(db) + "-wal"), Path(str(db) + "-shm"), Path(str(db) + "-journal")):
        if path.exists():
            result[path.name] = (path.stat().st_size, hashlib.sha256(path.read_bytes()).hexdigest())
    return result


def request_ms(url):
    start = time.perf_counter()
    request = urllib.request.Request(url, headers={"Host": urllib.parse.urlsplit(url).netloc,
                                                   "Sec-Fetch-Site": "none", "Sec-Fetch-Mode": "navigate"})
    with urllib.request.urlopen(request, timeout=10) as response:
        response.read()
        if response.status != 200:
            raise RuntimeError(f"HTTP {response.status} from {redact_url(url)}")
    return (time.perf_counter() - start) * 1000


def process_metrics(pid):
    if sys.platform.startswith("linux"):
        status = Path(f"/proc/{pid}/status").read_text()
        value = lambda key: int(re.search(rf"^{key}:\s+(\d+)", status, re.M).group(1))
        rss, threads = value("VmRSS") * 1024, value("Threads")
        descriptors = len(list(Path(f"/proc/{pid}/fd").iterdir()))
    elif sys.platform == "darwin":
        output = subprocess.check_output(["ps", "-o", "rss=", "-p", str(pid)], text=True).split()
        if len(output) != 1:
            raise RuntimeError("ps did not provide RSS")
        rss = int(output[0]) * 1024
        thread_listing = subprocess.check_output(["ps", "-M", "-p", str(pid)], text=True)
        threads = max(0, len(thread_listing.splitlines()) - 1)
        if threads == 0:
            raise RuntimeError("ps did not provide thread count")
        if not shutil.which("lsof"):
            raise RuntimeError("lsof is required to measure descriptors on macOS")
        listing = subprocess.run(["lsof", "-a", "-p", str(pid), "-d", "0-999"],
                                 text=True, capture_output=True, check=True).stdout
        descriptors = max(0, len(listing.splitlines()) - 1)
    else:
        raise RuntimeError("resource measurement is supported only on macOS and Linux")
    cpu_text = subprocess.check_output(["ps", "-o", "%cpu=", "-p", str(pid)], text=True).strip()
    if not cpu_text:
        raise RuntimeError("ps did not provide CPU utilization")
    return rss, float(cpu_text), threads, descriptors


def cpu_seconds(pid):
    if sys.platform.startswith("linux"):
        fields = Path(f"/proc/{pid}/stat").read_text().split()
        return (int(fields[13]) + int(fields[14])) / os.sysconf("SC_CLK_TCK")
    elapsed = subprocess.check_output(["ps", "-o", "time=", "-p", str(pid)], text=True).strip()
    parts = elapsed.split(":")
    if len(parts) == 2:
        return int(parts[0]) * 60 + float(parts[1])
    if len(parts) == 3:
        return int(parts[0]) * 3600 + int(parts[1]) * 60 + float(parts[2])
    raise RuntimeError("ps did not provide process CPU time")


def assert_loopback_network(pid):
    if not shutil.which("lsof"):
        raise RuntimeError("lsof is required to verify network isolation")
    result = subprocess.run(["lsof", "-Pan", "-p", str(pid), "-i"], text=True, capture_output=True)
    if result.returncode not in (0, 1):
        raise RuntimeError("lsof could not observe process network sockets")
    for line in result.stdout.splitlines()[1:]:
        if "TCP" in line or "UDP" in line:
            if "127.0.0.1:" not in line and "localhost:" not in line:
                raise RuntimeError("external network socket observed")


def start_dashboard(binary, db, env, timeout=15):
    started = time.perf_counter()
    process = subprocess.Popen([str(binary), "dashboard", "--db", str(db), "--start-date", "2016-01-01",
                                "--end-date", "2025-12-31", "--currency", "COP", "--no-open"],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env,
                               start_new_session=True)
    lines = queue.Queue()

    def read_stdout():
        for line in process.stdout:
            lines.put(line)
        lines.put(None)

    reader = threading.Thread(target=read_stdout, daemon=True)
    reader.start()
    deadline = time.monotonic() + timeout
    text = ""
    while time.monotonic() < deadline:
        try:
            line = lines.get(timeout=min(0.1, max(0.0, deadline - time.monotonic())))
        except queue.Empty:
            line = ""
        if line:
            text += line
            try:
                return process, parse_capability_url(text), (time.perf_counter() - started) * 1000
            except ValueError:
                pass
        elif line is None:
            raise RuntimeError("dashboard exited before printing its ready URL")
        if process.poll() is not None:
            raise RuntimeError("dashboard exited before readiness: " + process.stderr.read().strip())
    os.killpg(process.pid, signal.SIGKILL)
    process.wait()
    reader.join(timeout=1)
    process.stdout.close()
    process.stderr.close()
    raise RuntimeError("dashboard readiness timed out")


def stop(process):
    process.send_signal(signal.SIGTERM)
    try:
        code = process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill(); process.wait()
        raise RuntimeError("dashboard ignored SIGTERM")
    if code != 0:
        raise RuntimeError(f"dashboard exited with status {code}")


def run(args):
    binary = args.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("--binary must name an executable file")
    commands = []
    with tempfile.TemporaryDirectory(prefix="tracky runtime ü space ") as raw:
        root = Path(raw); db = root / "ledger fixture.sqlite"
        for name in ("home", "config", "cache", "tmp"):
            (root / name).mkdir()
        env = {**os.environ, "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"),
               "XDG_CACHE_HOME": str(root / "cache"), "TMPDIR": str(root / "tmp")}
        registry = [str(binary), "accounts", "register", "--db", str(db), "--institution", "runtime-bank",
                    "--label", "Runtime account", "--account-type", "checking", "--currency", "COP", "--json"]
        subprocess.run(registry, env=env, check=True, capture_output=True)
        commands.append("tracky accounts register --db <sandbox>/ledger-fixture.sqlite … --json")
        connection = sqlite3.connect(db)
        account_id = connection.execute("SELECT id FROM accounts WHERE label='Runtime account'").fetchone()[0]
        seed_fixture(connection, account_id); connection.close()
        before = file_state(db)
        commands.append("tracky dashboard --db <sandbox>/ledger-fixture.sqlite --start-date 2016-01-01 --end-date 2025-12-31 --currency COP --no-open")
        for _ in range(5):
            warm_process, _, _ = start_dashboard(binary, db, env)
            stop(warm_process)
        readiness_samples = []
        process = None
        for index in range(30):
            measured_process, measured_url, readiness = start_dashboard(binary, db, env)
            readiness_samples.append(readiness)
            if index == 29:
                process, url = measured_process, measured_url
            else:
                stop(measured_process)
        prefix = url.rstrip("/")
        endpoints = {
            "initial_snapshot_p95_ms": prefix + "/api/v1/dashboard",
            "refresh_p95_ms": prefix + "/api/v1/dashboard/refresh",
            "navigation_p95_ms": url,
            "drill_down_p95_ms": prefix + "/api/v1/transactions?metric=activity&month=2025-12&limit=50",
            "filter_interaction_p95_ms": prefix + "/api/v1/dashboard?start=2025-01-01&end=2025-12-31&currency=COP",
        }
        try:
            samples = {name: [] for name in endpoints}
            for _ in range(5):
                for endpoint in endpoints.values(): request_ms(endpoint)
            idle_rss, _, threads, descriptors = process_metrics(process.pid)
            idle_started = time.monotonic(); idle_cpu_started = cpu_seconds(process.pid)
            time.sleep(2)
            idle_cpu = (cpu_seconds(process.pid) - idle_cpu_started) * 100 / (time.monotonic() - idle_started)
            peak_rss = idle_rss
            for _ in range(30):
                for name, endpoint in endpoints.items(): samples[name].append(request_ms(endpoint))
                peak_rss = max(peak_rss, process_metrics(process.pid)[0])
            start_rss, _, _, start_descriptors = process_metrics(process.pid)
            for cycle in range(100):
                request_ms(endpoints["filter_interaction_p95_ms"] if cycle % 2 else endpoints["drill_down_p95_ms"])
                peak_rss = max(peak_rss, process_metrics(process.pid)[0])
            end_rss, _, end_threads, end_descriptors = process_metrics(process.pid)
            assert_loopback_network(process.pid)
            other, other_url, _ = start_dashboard(binary, db, env)
            try: request_ms(other_url)
            finally: stop(other)
            latency = {name: round(p95(values), 3) for name, values in samples.items()}
            latency.update(warmups=5, runs=30, readiness_p95_ms=round(p95(readiness_samples), 3))
            growth = end_rss - start_rss
            resources = {"idle_rss_bytes": idle_rss, "peak_rss_bytes": peak_rss,
                         "idle_cpu_percent": idle_cpu, "threads": max(threads, end_threads),
                         "descriptors": max(descriptors, end_descriptors), "cycles": 100,
                         "descriptor_growth": end_descriptors - start_descriptors,
                         "memory_growth_bytes": growth,
                         "memory_growth_percent": round(growth * 100 / start_rss, 3) if start_rss else 0}
            check_budgets(latency, resources)
        finally:
            stop(process)
        if file_state(db) != before:
            raise RuntimeError("dashboard modified the database or a SQLite sidecar")
        fragment = {"target": args.target, "latency": latency, "resources": resources, "commands": commands}
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(fragment, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--target", required=True, choices=sorted(TARGETS))
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try: run(args)
    except (OSError, ValueError, RuntimeError, sqlite3.Error, subprocess.SubprocessError) as error:
        parser.exit(1, f"runtime measurement failed: {error}\n")


if __name__ == "__main__": main()
