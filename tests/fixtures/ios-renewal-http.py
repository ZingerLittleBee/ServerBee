#!/usr/bin/env python3
"""Local client-acceptance transport. No recurrence, database or provider model.

Deadline literals: renewal_projection.rs enabling_overdue_month_end...,
scheduled_evaluation_advances_offline_month_end..., disabling_freezes...;
manual-date snapshot: renewal_edits.rs (see native UI evidence).
Opaque occurrence strings are fixture identities, never real Server credentials.
"""

import argparse
import copy
import json
import socketserver
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

SERVER_ID = "renewal-ui-server"
ACCESS_MARKER = "fixture-renewal-access"
NY_JANUARY = "2026-02-01T04:59:59.999999999Z"
NY_FEBRUARY = "2026-03-01T04:59:59.999999999Z"
NY_MANUAL = "2026-02-16T04:59:59.999999999Z"


def snapshot(enabled=False, date="2026-01-31", deadline=NY_JANUARY,
             origin="confirmed", confirmed=NY_JANUARY, occurrence="fixture-january"):
    return {
        "id": SERVER_ID, "name": "Renewal UI fixture", "billing_cycle": "monthly",
        "price": 10, "currency": "USD", "weight": 0, "hidden": False,
        "capabilities": 0, "agent_local_capabilities": 0, "effective_capabilities": 0,
        "agent_authority": {"status": "claimed", "outstanding_offer": None},
        "expired_at": deadline,
        "renewal": {
            "enabled": enabled, "billing_timezone": "America/New_York",
            "expiry_date": date, "confirmed_expired_at": confirmed,
            "deadline_origin": origin, "occurrence_id": occurrence,
        },
    }


CONFIRMED = snapshot()
PROJECTED = snapshot(True, "2026-02-28", NY_FEBRUARY, "projected",
                     occurrence="fixture-february")
FROZEN = copy.deepcopy(PROJECTED)
FROZEN["renewal"].update(enabled=False, deadline_origin="frozen")
TIMEZONE_SAVED = copy.deepcopy(CONFIRMED)
TIMEZONE_SAVED["expired_at"] = "2026-01-31T23:59:59.999999999Z"
TIMEZONE_SAVED["renewal"]["billing_timezone"] = "UTC"
MANUAL = snapshot(True, "2026-02-15", NY_MANUAL, "projected", NY_MANUAL,
                  "fixture-manual-february")
NAMED = copy.deepcopy(MANUAL)
NAMED["name"] = "Renamed renewal fixture"
SCENARIOS = {
    "timezone": (CONFIRMED, [({"billing_timezone": "UTC"}, TIMEZONE_SAVED)]),
    "switch": (CONFIRMED, [({"enabled": True}, PROJECTED), ({"enabled": False}, FROZEN)]),
    "manual": (PROJECTED, [({"expiry_date": "2026-02-15"}, MANUAL), (None, NAMED)]),
}
PUSH_SETUP = {
    "revision": 1, "registered": False, "delivery_available": False,
    "security_allowed": False, "tasks_allowed": False, "test_available": False,
    "task_failure_available": False,
    "preferences": {"enabled": False, "alerts": False, "security": False,
                    "task_failure": False, "task_success": False},
}


class Fixture:
    def __init__(self, evidence):
        self.evidence = evidence
        self.lock = threading.RLock()
        self.current = copy.deepcopy(CONFIRMED)
        self.scenario = None
        self.saves = []
        self.errors = []
        self.completed = {}
        self.reads = {}
        self.persist()

    def state(self):
        return {"scenario": self.scenario, "server": self.current,
                "saves": self.saves, "errors": self.errors, "completed": self.completed,
                "reads": self.reads}

    def persist(self):
        pending = self.evidence.joinpath("fixture-state.pending.json")
        pending.write_text(
            json.dumps(self.state(), indent=2) + "\n")
        pending.replace(self.evidence.joinpath("fixture-state.json"))

    def capture(self, method, path, body, status):
        # Deliberately never record headers, query values, access or refresh markers.
        record = {"method": method, "path": path, "body": body, "status": status,
                  "scenario": self.scenario}
        with self.evidence.joinpath("fixture-requests.jsonl").open("a") as stream:
            stream.write(json.dumps(record) + "\n")

    def fail(self, message):
        self.errors.append(message)
        self.persist()
        return 422, {"error": {"message": message}}

    def save(self, body):
        if self.scenario not in SCENARIOS:
            return self.fail("save without an active scenario")
        steps = SCENARIOS[self.scenario][1]
        index = len(self.saves)
        if index >= len(steps):
            return self.fail("unexpected additional save")
        expected, result = steps[index]
        if not isinstance(body, dict) or "expired_at" in body:
            return self.fail("invalid body or legacy expired_at date intent")
        if expected is None:
            if "renewal" in body or body.get("name") != NAMED["name"]:
                return self.fail("name-only save must omit renewal")
        elif body.get("renewal") != expected:
            return self.fail("renewal intent differs from the scenario contract")
        if expected and "enabled" in expected and type(body["renewal"]["enabled"]) is not bool:
            return self.fail("enabled intent must be a JSON boolean")
        if body.get("billing_cycle") != "monthly":
            return self.fail("billing cycle must remain monthly")
        allowed = {"name", "weight", "hidden", "remark", "public_remark", "group_id",
                   "currency", "billing_cycle", "traffic_limit_type", "price",
                   "billing_start_day", "traffic_limit", "renewal"}
        if set(body) - allowed:
            return self.fail("unrecognized server update fields")
        self.saves.append(copy.deepcopy(body))
        self.current = copy.deepcopy(result)
        if len(self.saves) == len(steps):
            self.completed[self.scenario] = copy.deepcopy(self.saves)
        self.persist()
        return 200, {"data": self.current}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        self.handle_request()

    def do_POST(self):
        self.handle_request()

    def do_PUT(self):
        self.handle_request()

    def do_DELETE(self):
        self.handle_request()

    def do_PATCH(self):
        self.handle_request()

    def do_OPTIONS(self):
        self.handle_request()

    def do_HEAD(self):
        self.handle_request()

    def handle_request(self):
        fixture = self.server.fixture
        with fixture.lock:
            route = urlsplit(self.path)
            path = route.path
            body = None
            try:
                if route.scheme or route.netloc or "%" in path or ".." in path:
                    raise ValueError("non-local request target")
                if self.headers.get("Host") != f"127.0.0.1:{self.server.server_port}":
                    raise ValueError("non-loopback host")
                length = int(self.headers.get("Content-Length", "0"))
                if length < 0 or length > 65536:
                    raise ValueError("invalid request length")
                if length:
                    body = json.loads(self.rfile.read(length))
                if path.startswith("/api/"):
                    authorized = self.headers.get("Authorization") == f"Bearer {ACCESS_MARKER}"
                    if not authorized:
                        raise ValueError("application request lacks fixture marker")
                status, reply = self.dispatch(path, body)
            except (ValueError, TypeError, json.JSONDecodeError) as error:
                status, reply = fixture.fail(str(error))
            # Unknown/provider writes may contain confidential bodies. Capture only
            # the approved editor PUT and reset fields; never an auth/register body.
            captured = body if path == f"/api/servers/{SERVER_ID}" and self.command == "PUT" else None
            fixture.capture(self.command, path, captured, status)
            encoded = json.dumps(reply).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

    def dispatch(self, path, body):
        fixture = self.server.fixture
        method = self.command
        if method == "GET" and path == "/__test/state":
            return 200, fixture.state()
        if method == "POST" and path == "/__test/reset":
            if not isinstance(body, dict) or body.get("scenario") not in SCENARIOS:
                return fixture.fail("unknown reset scenario")
            fixture.scenario = body["scenario"]
            fixture.saves = []
            fixture.reads = {}
            fixture.current = copy.deepcopy(SCENARIOS[fixture.scenario][0])
            # Errors and completed captures survive resets, so a failed earlier
            # case cannot disappear from the final runner report.
            fixture.persist()
            return 200, fixture.state()
        server_path = f"/api/servers/{SERVER_ID}"
        if method == "PUT" and path == server_path:
            return fixture.save(body)
        if method == "GET":
            fixture.reads[path] = fixture.reads.get(path, 0) + 1
            fixture.persist()
            if path == "/api/servers":
                return 200, {"data": [fixture.current]}
            if path == server_path:
                return 200, {"data": fixture.current}
            if path in {"/api/server-groups", "/api/alert-events",
                        server_path + "/tags", server_path + "/records"}:
                return 200, {"data": []}
            if path == "/api/mobile/push/settings":
                return 200, {"data": PUSH_SETUP}
            if path == "/api/ws/servers":
                # Intentional bounded unsupported local WS, not a live refresh proof.
                return 426, {"error": {"message": "fixture has no live WebSocket"}}
        return fixture.fail(f"unsupported fixture route: {method} {path}")


class LoopbackHTTPServer(ThreadingHTTPServer):
    def server_bind(self):
        # HTTPServer performs reverse DNS after bind. This numeric, IPv4-only
        # local transport must not wait for the hosted runner's resolver.
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    args.evidence_dir.mkdir(parents=True, exist_ok=True)
    def progress(stage):
        print(f"[renewal-fixture {time.monotonic():.3f}] {stage}", file=sys.stderr, flush=True)

    progress("binding numeric IPv4 loopback socket")
    server = LoopbackHTTPServer(("127.0.0.1", 0), Handler)
    progress("socket bound; initializing fixture evidence")
    server.fixture = Fixture(args.evidence_dir)
    progress("fixture initialized; publishing local URL")
    args.evidence_dir.joinpath("fixture-url.txt").write_text(
        f"http://127.0.0.1:{server.server_port}\n")
    progress("URL published; serving loopback HTTP")
    server.serve_forever()


if __name__ == "__main__":
    main()
