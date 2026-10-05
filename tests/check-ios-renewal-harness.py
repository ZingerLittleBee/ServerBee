#!/usr/bin/env python3
"""Portable protocol/gate checks. These do not execute Swift or a Simulator."""

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import time
import unittest
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("renewal_results", ROOT / "tests/check-ios-renewal-results.py")
results = importlib.util.module_from_spec(spec)
spec.loader.exec_module(results)


class FixtureStartupTests(unittest.TestCase):
    def test_live_unready_process_reports_bounded_timeout(self):
        with tempfile.TemporaryDirectory(prefix="renewal-startup-timeout-") as directory:
            process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
            try:
                check = subprocess.run(
                    [sys.executable, str(ROOT / "tests/check-ios-renewal-startup.py"),
                     "--evidence-dir", directory, "--pid", str(process.pid),
                     "--timeout-seconds", "0.2"], capture_output=True, text=True, timeout=3)
                self.assertNotEqual(check.returncode, 0)
                report = json.loads((Path(directory) / "fixture-startup-check.json").read_text())
                self.assertEqual(report["outcome"], "timeout")
                self.assertFalse(report["urlPublished"])
                self.assertTrue(report["processStatus"])
                self.assertGreaterEqual(report["elapsedSeconds"], 0.2)
                self.assertLess(report["elapsedSeconds"], 1)
                self.assertIn("did not become ready", check.stderr)
            finally:
                process.terminate()
                process.wait(timeout=3)

    def test_exited_fixture_reports_process_failure_instead_of_waiting_for_timeout(self):
        with tempfile.TemporaryDirectory(prefix="renewal-startup-exit-") as directory:
            failed = subprocess.Popen([sys.executable, "-c", "raise SystemExit(7)"])
            failed.wait(timeout=2)
            started = time.monotonic()
            check = subprocess.run(
                [sys.executable, str(ROOT / "tests/check-ios-renewal-startup.py"),
                 "--evidence-dir", directory, "--pid", str(failed.pid)],
                capture_output=True, text=True, timeout=3)
            self.assertNotEqual(check.returncode, 0)
            report_file = Path(directory) / "fixture-startup-check.json"
            self.assertTrue(report_file.exists(), "missing fixture startup process diagnostics")
            report = json.loads(report_file.read_text())
            self.assertEqual(report["outcome"], "process-exited")
            self.assertFalse(report["urlPublished"])
            self.assertLess(time.monotonic() - started, 2)
            self.assertIn("process exited", check.stderr)

    def test_loopback_fixture_becomes_ready_without_reverse_dns(self):
        # Resolver availability is an external startup dependency. Exercise the
        # actual CLI, socket bind and HTTP state endpoint while that boundary
        # cannot return; no fixture protocol or application behavior is mocked.
        wrapper = """import runpy, socket, sys, time
def stalled_resolver(*args, **kwargs):
    time.sleep(60)
socket.getfqdn = stalled_resolver
sys.argv = sys.argv[1:]
runpy.run_path(sys.argv[0], run_name='__main__')
"""
        with tempfile.TemporaryDirectory(prefix="renewal-startup-") as directory:
            process = subprocess.Popen(
                [sys.executable, "-c", wrapper,
                 str(ROOT / "tests/fixtures/ios-renewal-http.py"),
                 "--evidence-dir", directory], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                url_file = Path(directory) / "fixture-url.txt"
                deadline = time.monotonic() + 2
                while not url_file.exists() and time.monotonic() < deadline:
                    if process.poll() is not None:
                        self.fail("fixture exited before publishing readiness")
                    time.sleep(0.02)
                self.assertTrue(url_file.exists(),
                                "Loopback renewal fixture did not become ready: reverse DNS stalled startup")
                with urllib.request.urlopen(url_file.read_text().strip() + "/__test/state", timeout=1) as response:
                    self.assertEqual(response.status, 200)
                    self.assertEqual(json.load(response)["server"]["renewal"]["expiry_date"], "2026-01-31")
            finally:
                process.terminate()
                process.communicate(timeout=5)


class FixtureProtocolTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="renewal-fixture-")
        self.directory = Path(self.temp.name)
        self.process = subprocess.Popen(
            [sys.executable, str(ROOT / "tests/fixtures/ios-renewal-http.py"),
             "--evidence-dir", self.temp.name], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
        self.addCleanup(self.cleanup_fixture)
        for _ in range(100):
            if self.directory.joinpath("fixture-url.txt").exists():
                self.url = self.directory.joinpath("fixture-url.txt").read_text().strip()
                return
            if self.process.poll() is not None:
                self.fail("fixture process exited before readiness")
            time.sleep(0.02)
        self.fail("fixture startup timed out")

    def cleanup_fixture(self):
        self.process.terminate()
        self.process.communicate(timeout=5)
        self.temp.cleanup()

    def test_ready_probe_preserves_success_evidence_using_only_loopback_transport(self):
        # Proxies are a runner boundary, not part of this loopback fixture.
        environment = {**os.environ, "http_proxy": "http://127.0.0.1:1",
                       "HTTP_PROXY": "http://127.0.0.1:1", "no_proxy": "", "NO_PROXY": ""}
        check = subprocess.run(
            [sys.executable, str(ROOT / "tests/check-ios-renewal-startup.py"),
             "--evidence-dir", self.temp.name, "--pid", str(self.process.pid)],
            env=environment, capture_output=True, text=True, timeout=3)
        self.assertEqual(check.returncode, 0, check.stderr)
        report = json.loads(self.directory.joinpath("fixture-startup-check.json").read_text())
        self.assertEqual(report["outcome"], "ready")
        self.assertTrue(report["urlPublished"])
        self.assertGreaterEqual(report["attempts"], 1)

    def request(self, path, method="GET", body=None, marker=True, host=None):
        headers = {"Content-Type": "application/json"}
        if marker:
            headers["Authorization"] = "Bearer fixture-renewal-access"
        if host:
            headers["Host"] = host
        request = urllib.request.Request(self.url + path, method=method, headers=headers,
                                         data=json.dumps(body).encode() if body is not None else None)
        try:
            response = urllib.request.urlopen(request, timeout=3)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            return response.status, json.load(response)

    def reset(self, scenario):
        status, state = self.request("/__test/reset", "POST", {"scenario": scenario})
        self.assertEqual(status, 200)
        return state

    def save(self, renewal, name="Renewal UI fixture"):
        body = {"name": name, "billing_cycle": "monthly"}
        if renewal is not None:
            body["renewal"] = renewal
        return self.request("/api/servers/renewal-ui-server", "PUT", body)

    def test_three_scenarios_capture_changed_intent_and_fixed_wire_responses(self):
        self.reset("timezone")
        for path in ["/api/servers", "/api/servers/renewal-ui-server", "/api/server-groups",
                     "/api/servers/renewal-ui-server/tags", "/api/servers/renewal-ui-server/records?from=x&to=y&interval=raw",
                     "/api/alert-events?limit=100", "/api/mobile/push/settings"]:
            self.assertEqual(self.request(path)[0], 200)
        self.assertEqual(self.request("/api/ws/servers")[0], 426)
        status, body = self.save({"billing_timezone": "UTC"})
        self.assertEqual(status, 200)
        self.assertEqual(body["data"]["renewal"]["expiry_date"], "2026-01-31")
        self.assertEqual(body["data"]["expired_at"], "2026-01-31T23:59:59.999999999Z")
        self.reset("switch")
        status, projected = self.save({"enabled": True})
        self.assertEqual(status, 200)
        self.assertEqual(projected["data"]["renewal"]["expiry_date"], "2026-02-28")
        status, frozen = self.save({"enabled": False})
        self.assertEqual(status, 200)
        self.assertEqual(frozen["data"]["expired_at"], "2026-03-01T04:59:59.999999999Z")
        self.assertEqual(frozen["data"]["renewal"]["deadline_origin"], "frozen")
        self.assertEqual(frozen["data"]["renewal"]["occurrence_id"], projected["data"]["renewal"]["occurrence_id"])
        self.reset("manual")
        status, manual = self.save({"expiry_date": "2026-02-15"})
        self.assertEqual(status, 200)
        self.assertEqual(manual["data"]["expired_at"], "2026-02-16T04:59:59.999999999Z")
        self.assertEqual(manual["data"]["renewal"]["confirmed_expired_at"], "2026-02-16T04:59:59.999999999Z")
        self.assertEqual(self.save(None, "Renamed renewal fixture")[0], 200)
        state = self.request("/__test/state")[1]
        self.assertEqual(results.validate_fixture(state)["saveCount"], 5)
        capture = self.directory.joinpath("fixture-requests.jsonl").read_text()
        self.assertNotIn("fixture-renewal-access", capture)
        self.assertNotIn("Authorization", capture)

    def test_explicit_date_resubmission_on_switch_save_is_rejected_and_error_survives_reset(self):
        self.reset("switch")
        self.assertEqual(self.save({"enabled": True, "expiry_date": "2026-01-31"})[0], 422)
        state = self.reset("manual")
        self.assertTrue(state["errors"])

    def test_dual_legacy_and_local_date_is_rejected(self):
        self.reset("manual")
        status, _ = self.request("/api/servers/renewal-ui-server", "PUT", {
            "billing_cycle": "monthly", "renewal": {"expiry_date": "2026-02-15"},
            "expired_at": "2026-02-15T00:00:00Z"})
        self.assertEqual(status, 422)

    def test_numeric_switch_value_cannot_stand_in_for_boolean_intent(self):
        self.reset("switch")
        self.assertEqual(self.save({"enabled": 1})[0], 422)

    def test_unknown_read_and_notification_write_fail_without_capturing_sensitive_body(self):
        self.assertEqual(self.request("/api/not-a-route")[0], 422)
        self.assertEqual(self.request("/api/mobile/push/encrypted-register", "POST", {"content_key": "must-not-capture"})[0], 422)
        self.assertNotIn("must-not-capture", self.directory.joinpath("fixture-requests.jsonl").read_text())

    def test_missing_marker_and_non_loopback_host_are_rejected(self):
        self.assertEqual(self.request("/api/servers", marker=False)[0], 422)
        self.assertEqual(self.request("/api/servers", host="example.com")[0], 422)


class ResultGateTests(unittest.TestCase):
    def ui_documents(self):
        summary = {"totalTestCount": 3, "passedTests": 3, "failedTests": 0, "skippedTests": 0}
        tree = {"testNodes": [{"nodeType": "Test Suite", "name": "ServerBeeUITests",
                              "children": [{"nodeType": "Test Suite", "name": "RenewalBillingUITests",
                                            "children": [{"nodeType": "Test Case", "name": name + "()",
                                                          "nodeIdentifier": "RenewalBillingUITests/" + name + "()",
                                                          "result": "Passed"} for name in sorted(results.UI_METHODS)]}]}]}
        return summary, tree

    def test_supported_sample_schema_passes_all_three_named_cases(self):
        summary, tree = self.ui_documents()
        self.assertEqual(results.validate_results(summary, tree, "ui")["counts"]["passedTests"], 3)

    def test_zero_skip_missing_method_unknown_schema_cannot_pass(self):
        summary, tree = self.ui_documents()
        for changed in [{}, {**summary, "totalTestCount": 0}, {**summary, "skippedTests": 1}]:
            with self.assertRaises(ValueError):
                results.validate_results(changed, tree, "ui")
        with self.assertRaises(ValueError):
            results.validate_results(summary, {"unexpectedNodes": []}, "ui")
        tree["testNodes"][0]["children"][0]["children"][0]["name"] = "testUnrelated()"
        with self.assertRaises(ValueError):
            results.validate_results(summary, tree, "ui")

    def test_successful_ui_counts_cannot_fill_unit_gate(self):
        summary, tree = self.ui_documents()
        with self.assertRaises(ValueError):
            results.validate_results(summary, tree, "unit", ROOT / "apps/ios/ServerBeeTests/RenewalDateTests.swift")

    def test_incomplete_or_error_fixture_cannot_pass(self):
        for state in [{}, {"errors": [], "completed": {}}, {"errors": ["unknown write"], "completed": {}}]:
            with self.assertRaises(ValueError):
                results.validate_fixture(state)


if __name__ == "__main__":
    unittest.main(verbosity=2)
