#!/usr/bin/env python3
"""Run packaged-dashboard release gates through a real W3C WebDriver."""

import argparse
import json
import os
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path


GATES = ("browser-flow", "progressive-no-javascript", "security", "lifecycle", "automated-accessibility")


def gate(result, name):
    return next(item for item in result["gates"] if item["gate"] == name)


def http_json(method, url, value=None, timeout=15):
    data = None if value is None else json.dumps(value).encode()
    request = urllib.request.Request(url, data=data, method=method,
                                     headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        body = response.read()
    return json.loads(body) if body else {}


def numeric_version(text):
    match = re.search(r"\d+(?:\.\d+)*", text or "")
    if not match:
        raise RuntimeError("version did not contain a numeric component")
    return tuple(int(item) for item in match.group().split("."))


def run_json(binary, env, *arguments):
    result = subprocess.run([str(binary), *arguments, "--json"], env=env, text=True,
                            capture_output=True, check=True)
    return json.loads(result.stdout)


def free_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


class Driver:
    def __init__(self, browser, executable, browser_binary, env, javascript_enabled=True):
        self.browser, self.port, self.session = browser, free_port(), None
        version = subprocess.run([executable, "--version"], env=env, text=True,
                                 capture_output=True).stdout.strip()
        self.version = version or subprocess.run([executable, "--version"], env=env, text=True,
                                                  capture_output=True).stderr.strip()
        arguments = [executable, "-p", str(self.port)] if browser == "safari" else \
                    [executable, "--port", str(self.port)] if browser == "firefox" else \
                    [executable, f"--port={self.port}"]
        self.process = subprocess.Popen(arguments, env=env, stdout=subprocess.DEVNULL,
                                        stderr=subprocess.PIPE, text=True)
        self.base = f"http://127.0.0.1:{self.port}"
        try:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                if self.process.poll() is not None:
                    raise RuntimeError("WebDriver exited during startup: " + self.process.stderr.read().strip())
                try:
                    http_json("GET", self.base + "/status", timeout=1)
                    break
                except (OSError, urllib.error.URLError):
                    time.sleep(.1)
            else:
                raise RuntimeError("WebDriver readiness timed out")

            always = {"browserName": {"safari": "safari", "firefox": "firefox",
                                       "chromium": "chrome"}[browser]}
            if browser == "firefox":
                always["moz:firefoxOptions"] = {"args": ["-headless"]}
                if not javascript_enabled:
                    always["moz:firefoxOptions"]["prefs"] = {"javascript.enabled": False}
                if browser_binary:
                    always["moz:firefoxOptions"]["binary"] = str(browser_binary)
            elif browser == "chromium":
                always["goog:chromeOptions"] = {"args": ["--headless=new", "--no-sandbox"]}
                if not javascript_enabled:
                    always["goog:chromeOptions"]["prefs"] = {
                        "profile.managed_default_content_settings.javascript": 2
                    }
                if browser_binary:
                    always["goog:chromeOptions"]["binary"] = str(browser_binary)
            elif browser_binary:
                raise RuntimeError("--browser-binary is not supported by safaridriver")
            created = http_json("POST", self.base + "/session",
                                {"capabilities": {"alwaysMatch": always}})
            value = created.get("value", created)
            self.session = value.get("sessionId") or created.get("sessionId")
            if not self.session:
                raise RuntimeError("WebDriver did not create a session: " + json.dumps(created))
            self.capabilities = value.get("capabilities", value.get("value", {}))
        except Exception:
            self.close()
            raise

    def command(self, method, suffix, value=None):
        result = http_json(method, f"{self.base}/session/{self.session}{suffix}", value)
        payload = result.get("value")
        if isinstance(payload, dict) and payload.get("error"):
            raise RuntimeError(f"WebDriver {payload['error']}: {payload.get('message', '')}")
        return payload

    def navigate(self, url):
        self.command("POST", "/url", {"url": url})

    def script(self, source, args=None):
        return self.command("POST", "/execute/sync", {"script": source, "args": args or []})

    def async_script(self, source, args=None):
        return self.command("POST", "/execute/async", {"script": source, "args": args or []})

    def body_text(self):
        element = self.command("POST", "/element", {"using": "css selector", "value": "body"})
        element_id = element.get("element-6066-11e4-a52e-4f735466cecf")
        if not element_id:
            raise RuntimeError("WebDriver did not return the document body")
        return self.command("GET", f"/element/{element_id}/text")

    def set_window_size(self, width, height):
        self.command("POST", "/window/rect", {"width": width, "height": height})

    def close(self):
        if self.session:
            try:
                http_json("DELETE", f"{self.base}/session/{self.session}", timeout=5)
            except OSError:
                pass
            self.session = None
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill(); self.process.wait()


def start_dashboard(binary, env, database):
    process = subprocess.Popen([str(binary), "dashboard", "--db", str(database),
                                "--start-date", "2026-01-01", "--end-date", "2026-07-31",
                                "--currency", "COP", "--no-open"], env=env, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    deadline, output = time.monotonic() + 15, ""
    while time.monotonic() < deadline:
        line = process.stdout.readline()
        output += line
        match = re.search(r"http://127\.0\.0\.1:\d+/c/[0-9a-f]{64}/", output)
        if match:
            return process, match.group(0)
        if process.poll() is not None:
            break
    raise RuntimeError("dashboard did not become ready: " + process.stderr.read().strip())


def stop_dashboard(process):
    process.send_signal(signal.SIGTERM)
    try:
        code = process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        process.kill(); process.wait()
        raise RuntimeError("dashboard ignored SIGTERM")
    if code:
        raise RuntimeError(f"dashboard exited with status {code}: {process.stderr.read().strip()}")


def axe_source(env):
    root = subprocess.run(["npm", "root", "--global"], env=env, text=True,
                          capture_output=True, check=True).stdout.strip()
    path = Path(root) / "axe-core" / "axe.min.js"
    if not path.is_file():
        raise RuntimeError(f"globally installed axe-core was not found at {path}")
    return path.read_text(encoding="utf-8")


def seed(binary, env, database):
    account = run_json(binary, env, "accounts", "register", "--db", str(database),
                       "--institution", "Release Bank", "--label", "Main account",
                       "--account-type", "checking", "--currency", "COP")["account"]["id"]
    source = run_json(binary, env, "income-sources", "create", "--db", str(database),
                      "--name", "Salary")["income_source"]["id"]
    category = run_json(binary, env, "categories", "create", "--db", str(database),
                        "--name", "Food")["category"]["id"]
    run_json(binary, env, "transactions", "add-income", "--db", str(database),
             "--account-id", account, "--posted-date", "2026-07-01", "--description",
             "Release salary", "--amount-minor", "500000", "--currency", "COP",
             "--income-source-id", source, "--income-kind", "salary")
    run_json(binary, env, "transactions", "add-expense", "--db", str(database),
             "--account-id", account, "--posted-date", "2026-07-10", "--description",
             "Release food", "--amount-minor=-170000", "--currency", "COP",
             "--category-id", category)
    return account, source


def rejected_response(url, method="GET", headers=None):
    request = urllib.request.Request(url, method=method, headers=headers or {})
    try:
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.status, response.headers, response.read().decode(errors="replace")
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read().decode(errors="replace")


def open_canonical_drawer(driver):
    return bool(driver.async_script("""
      const done=arguments[arguments.length-1];
      const monthly=document.querySelector('[data-region="monthly"] [data-month]');
      if(!monthly) return done(false); monthly.click();
      const deadline=Date.now()+5000;
      const timer=setInterval(()=>{
        const open=Boolean(document.querySelector('[data-drawer]')?.open);
        if(open||Date.now()>deadline){clearInterval(timer);done(open);}
      },50);
    """))


def responsive_state(driver):
    return driver.script("""return {width:document.documentElement.clientWidth,scroll:document.documentElement.scrollWidth,undersized_visible_buttons:[...document.querySelectorAll('button')].filter(n=>{const b=n.getBoundingClientRect();return n.getClientRects().length&&(b.width<24||b.height<24)}).length,storage:localStorage.length+sessionStorage.length+document.cookie.length,history:history.length};""")


def has_progressive_content(content, require_markup=True):
    normalized = content.replace("\u00a0", " ")
    required = ["COP $7.000,00", "2026-01-01", "2026-07-31", "COP"]
    if require_markup:
        required.extend(("<table", 'data-region="alerts"'))
    return all(token in normalized for token in required)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--browser", choices=("safari", "firefox", "chromium"), required=True)
    parser.add_argument("--browser-binary", type=Path)
    parser.add_argument("--driver", type=Path)
    parser.add_argument("--lane", required=True)
    parser.add_argument("--minimum-version", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--lockfile-sha256", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.commit):
        parser.error("--commit must be a full lowercase Git commit SHA")
    if not re.fullmatch(r"[0-9a-f]{64}", args.lockfile_sha256):
        parser.error("--lockfile-sha256 must be a lowercase SHA-256 digest")
    if args.browser_binary and (not args.browser_binary.is_file() or
                                not os.access(args.browser_binary, os.X_OK)):
        parser.error("--browser-binary must name an executable file")
    defaults = {"safari": "safaridriver", "firefox": "geckodriver", "chromium": "chromedriver"}
    executable = str(args.driver or shutil.which(defaults[args.browser]) or "")
    if not executable or not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        parser.error("--binary and the selected WebDriver must be executable")
    result = {"schema_version": 1, "lane": args.lane, "commit": args.commit,
              "lockfile_sha256": args.lockfile_sha256, "browser": {"name": args.browser, "version": ""},
              "driver": {"name": Path(executable).name, "version": ""},
              "command": shlex.join(["python3", "scripts/dashboard_release_browser.py", *sys.argv[1:]]),
              "gates": [{"gate": name, "status": "not-run"} for name in GATES]}
    dashboard = driver = None
    try:
        with tempfile.TemporaryDirectory(prefix="tracky-release-browser-") as raw:
            root = Path(raw)
            for name in ("home", "config", "state", "cache", "tmp"):
                (root / name).mkdir()
            env = {**os.environ, "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"),
                   "XDG_STATE_HOME": str(root / "state"), "XDG_CACHE_HOME": str(root / "cache"),
                   "TMPDIR": str(root / "tmp")}
            database = root / "dashboard.sqlite"
            account, source = seed(args.binary.resolve(), env, database)
            dashboard, url = start_dashboard(args.binary.resolve(), env, database)
            driver = Driver(args.browser, executable, args.browser_binary, env)
            result["driver"]["version"] = driver.version
            browser_version = str(driver.capabilities.get("browserVersion", ""))
            result["browser"]["version"] = browser_version
            if numeric_version(browser_version) < numeric_version(args.minimum_version):
                raise RuntimeError(f"browser {browser_version} is below minimum {args.minimum_version}")

            driver.navigate(url)
            flow = driver.script("""return {regions:[...document.querySelectorAll('[data-region]')].map(n=>n.dataset.region), text:document.body.innerText, storage:localStorage.length+sessionStorage.length+document.cookie.length, history:history.length};""")
            expected = ["scope", "currency", "summary", "monthly", "categories", "accounts", "alerts", "investments"]
            if flow["regions"] != expected or "COP\u00a0$5.000,00" not in flow["text"] or flow["storage"]:
                raise RuntimeError("browser-flow: dashboard content, region order, or ephemeral state failed")
            if not open_canonical_drawer(driver):
                raise RuntimeError("browser-flow: canonical drawer interaction failed")
            driver.navigate(url)
            filtered = driver.async_script("""
              const done=arguments[arguments.length-1];
              const button=[...document.querySelectorAll('button')].find(node=>node.textContent.trim()==='Filters');
              if(!button) return done(false); button.click();
              document.querySelectorAll('[name="account"]:checked').forEach(node=>node.click());
              const apply=[...document.querySelectorAll('button')].find(node=>node.textContent.trim()==='Apply filters');
              if(!apply) return done(false); apply.click();
              const deadline=Date.now()+5000; const timer=setInterval(()=>{const ok=document.body.innerText.includes('No activity matches these filters'); if(ok||Date.now()>deadline){clearInterval(timer);done(ok);}},50);
            """)
            if not filtered:
                raise RuntimeError("browser-flow: filters did not expose the valid empty state")
            driver.navigate(url)
            run_json(args.binary.resolve(), env, "transactions", "add-income", "--db", str(database),
                     "--account-id", account, "--posted-date", "2026-07-20", "--description",
                     "Externally added income", "--amount-minor", "200000", "--currency", "COP",
                     "--income-source-id", source, "--income-kind", "salary")
            refreshed = driver.async_script("""
              const done=arguments[arguments.length-1];
              const button=[...document.querySelectorAll('button')].find(node=>node.textContent.trim()==='Refresh');
              if(!button) return done(false); button.click();
              const deadline=Date.now()+5000; const timer=setInterval(()=>{const ok=document.body.innerText.includes('COP $7.000,00')&&document.body.innerText.includes('Dashboard refreshed'); if(ok||Date.now()>deadline){clearInterval(timer);done(ok);}},50);
            """)
            if not refreshed:
                raise RuntimeError("browser-flow: explicit refresh did not rebuild the snapshot")
            unavailable = database.with_name("dashboard-unavailable.sqlite")
            database.replace(unavailable)
            failed_refresh = driver.async_script("""
              const done=arguments[arguments.length-1];
              const button=[...document.querySelectorAll('button')].find(node=>node.textContent.trim()==='Refresh');
              if(!button) return done(false); button.click();
              const deadline=Date.now()+5000; const timer=setInterval(()=>{const text=document.body.innerText; const ok=text.includes('Refresh failed')&&text.includes('COP $7.000,00'); if(ok||Date.now()>deadline){clearInterval(timer);done(ok);}},50);
            """)
            unavailable.replace(database)
            if not failed_refresh:
                raise RuntimeError("browser-flow: failed refresh did not retain the last good snapshot")
            driver.set_window_size(320, 800)
            responsive = responsive_state(driver)
            if responsive["scroll"] > responsive["width"] or responsive["undersized_visible_buttons"] or responsive["storage"] or responsive["history"] != flow["history"]:
                raise RuntimeError("browser-flow: responsive, pointer-target, history, or ephemeral-state invariant failed")
            gate(result, "browser-flow")["status"] = "pass"

            with urllib.request.urlopen(url, timeout=10) as response:
                html, headers = response.read().decode(), response.headers
            if not has_progressive_content(html):
                raise RuntimeError("progressive-no-javascript: SSR content is incomplete")
            if args.browser in ("firefox", "chromium"):
                no_javascript = Driver(args.browser, executable, args.browser_binary, env, javascript_enabled=False)
                try:
                    no_javascript.navigate(url)
                    no_javascript_text = no_javascript.body_text()
                    if not has_progressive_content(no_javascript_text, require_markup=False):
                        raise RuntimeError("progressive-no-javascript: disabled browser omitted semantic content")
                finally:
                    no_javascript.close()
            gate(result, "progressive-no-javascript")["status"] = "pass"
            csp = headers.get("Content-Security-Policy", "")
            external = driver.script("return performance.getEntriesByType('resource').map(e=>new URL(e.name).origin).filter(o=>o!==location.origin);")
            capability = url.rstrip("/").rsplit("/", 1)[-1]
            adversarial = [
                rejected_response(url.replace(capability, "0" * 64)),
                rejected_response(url, headers={"Host": "evil.invalid"}),
                rejected_response(url, method="POST"),
                rejected_response(url + "%2e%2e/api/v1/dashboard"),
                rejected_response(url + "unknown"),
            ]
            protected_headers = ("Content-Security-Policy", "X-Frame-Options", "Referrer-Policy", "X-Content-Type-Options", "Cache-Control")
            rejected_safely = all(status == 404 and "COP\u00a0$7.000,00" not in body and all(response_headers.get(name) for name in protected_headers) for status, response_headers, body in adversarial)
            if (not csp or external or not rejected_safely
                    or headers.get("X-Content-Type-Options", "").lower() != "nosniff"
                    or headers.get("Access-Control-Allow-Origin")):
                raise RuntimeError("security: required headers or network isolation failed")
            gate(result, "security")["status"] = "pass"

            if dashboard.poll() is not None:
                raise RuntimeError("lifecycle: dashboard exited while browser was active")
            violations = driver.async_script(
                axe_source(env)
                + "\n; const done=arguments[arguments.length-1]; axe.run(document).then(r=>done(r.violations.map(v=>v.id))).catch(e=>done(['axe-error:'+e.message]));"
            )
            if violations:
                raise RuntimeError("automated-accessibility: axe violations: " + ", ".join(violations))
            gate(result, "automated-accessibility")["status"] = "pass"
            driver.close(); driver = None
            port = int(url.split(":", 2)[2].split("/", 1)[0])
            stop_dashboard(dashboard); dashboard = None
            with socket.socket() as probe:
                if probe.connect_ex(("127.0.0.1", port)) == 0:
                    raise RuntimeError("lifecycle: listener remained reachable after SIGTERM")
            gate(result, "lifecycle")["status"] = "pass"
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
        return 0
    except Exception as error:
        message = str(error)
        named = next((gate for gate in result["gates"]
                      if message.startswith(gate["gate"] + ":")), None)
        target = named or next((gate for gate in result["gates"]
                                if gate["status"] == "not-run"), None)
        if target:
            target["status"] = "fail"
            target["error"] = message
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
        print(error, file=sys.stderr)
        return 1
    finally:
        if driver:
            driver.close()
        if dashboard and dashboard.poll() is None:
            try: stop_dashboard(dashboard)
            except Exception: dashboard.kill(); dashboard.wait()


if __name__ == "__main__":
    raise SystemExit(main())
